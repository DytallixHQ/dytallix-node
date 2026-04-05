use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose;
use base64::Engine as _;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

// Async traits for providers
#[async_trait::async_trait]
pub trait KeyProvider {
    async fn get_validator_key(&self, id: &str) -> Result<Vec<u8>>;
    async fn put_validator_key(&self, id: &str, key: &[u8]) -> Result<()>;
}

// -------------------- Vault Provider --------------------
pub struct VaultProvider {
    base_url: String,
    token: String,
    kv_mount: String,
    path_base: String,
    client: reqwest::Client,
}

impl VaultProvider {
    pub fn new(base_url: String, token: String, kv_mount: String, path_base: String) -> Self {
        let client = reqwest::Client::new();
        Self {
            base_url,
            token,
            kv_mount,
            path_base,
            client,
        }
    }

    fn data_url(&self, id: &str) -> String {
        // KV v2 path: /v1/<mount>/data/<path>
        format!(
            "{}/v1/{}/data/{}/{}",
            self.base_url.trim_end_matches('/'),
            self.kv_mount.trim_matches('/'),
            self.path_base.trim_matches('/'),
            id
        )
    }
}

#[derive(Serialize, Debug)]
struct VaultWrite<'a> {
    data: VaultData<'a>,
}
#[derive(Serialize, Debug)]
struct VaultData<'a> {
    private_key: &'a str,
}
#[derive(Deserialize, Debug)]
struct VaultReadOuter {
    data: VaultReadData,
}
#[derive(Deserialize, Debug)]
struct VaultReadData {
    data: VaultReadInner,
}
#[derive(Deserialize, Debug)]
struct VaultReadInner {
    private_key: String,
}

#[async_trait::async_trait]
impl KeyProvider for VaultProvider {
    async fn get_validator_key(&self, id: &str) -> Result<Vec<u8>> {
        let url = self.data_url(id);
        let res = self
            .client
            .get(url)
            .header("X-Vault-Token", &self.token)
            .send()
            .await
            .context("vault get request failed")?;
        if !res.status().is_success() {
            return Err(anyhow!("vault get failed: {}", res.status()));
        }
        let body: VaultReadOuter = res.json().await.context("invalid vault json")?;
        let decoded = general_purpose::STANDARD
            .decode(body.data.data.private_key)
            .context("invalid base64 in vault secret")?;
        Ok(decoded)
    }

    async fn put_validator_key(&self, id: &str, key: &[u8]) -> Result<()> {
        let url = self.data_url(id);
        let b64 = general_purpose::STANDARD.encode(key);
        let payload = VaultWrite {
            data: VaultData { private_key: &b64 },
        };
        let res = self
            .client
            .post(url)
            .header("X-Vault-Token", &self.token)
            .json(&payload)
            .send()
            .await
            .context("vault put request failed")?;
        if !res.status().is_success() {
            return Err(anyhow!("vault put failed: {}", res.status()));
        }
        Ok(())
    }
}

// -------------------- Plain Keystore Provider (no passphrase) --------------------
use std::fs;
use std::path::PathBuf;

// Backward-compat evidence format note: we still write a proof file alongside keys.
// Not used for encryption anymore.
pub struct SealedKeystoreProvider {
    dir: PathBuf,
}

impl SealedKeystoreProvider {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file_path(&self, id: &str) -> PathBuf {
        let mut p = self.dir.clone();
        p.push(format!("validator-{id}.seal"));
        p
    }

    fn write_proof(&self, path: &PathBuf) {
        let proof_dir = PathBuf::from("launch-evidence/secrets");
        let _ = fs::create_dir_all(&proof_dir);
        let proof_path = proof_dir.join("keystore_proof.txt");
        if let Ok(meta) = fs::metadata(path) {
            let size = meta.len();
            use sha2::Digest;
            let sha = sha2::Sha256::digest(fs::read(path).unwrap_or_default());
            let _ = fs::write(
                proof_path,
                format!(
                    "path: {}\nsize: {} bytes\nsha256: 0x{}\n",
                    path.display(),
                    size,
                    hex::encode(sha)
                ),
            );
        }
    }
}

#[async_trait::async_trait]
impl KeyProvider for SealedKeystoreProvider {
    async fn get_validator_key(&self, id: &str) -> Result<Vec<u8>> {
        let path = self.file_path(id);
        fs::create_dir_all(&self.dir).ok();
        if !path.exists() {
            // Dev-friendly behavior: generate new key material without passphrase and store plaintext.
            let mut key_bytes = vec![0u8; 64];
            OsRng.fill_bytes(&mut key_bytes);
            fs::write(&path, &key_bytes).context("write keystore failed")?;
            self.write_proof(&path);
            Ok(key_bytes)
        } else {
            let pt = fs::read(&path).context("read keystore failed")?;
            self.write_proof(&path);
            Ok(pt)
        }
    }

    async fn put_validator_key(&self, id: &str, key: &[u8]) -> Result<()> {
        let path = self.file_path(id);
        fs::create_dir_all(&self.dir).ok();
        fs::write(&path, key).context("write keystore failed")?;
        self.write_proof(&path);
        Ok(())
    }
}
// Note: interactive passphrase prompting removed; keystore is unencrypted (dev-only).
