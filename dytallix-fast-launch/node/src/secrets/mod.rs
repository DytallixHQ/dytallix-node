pub mod providers;
use crate::secrets::providers::KeyProvider; // bring trait into scope
use base64::{engine::general_purpose::STANDARD as B64, Engine};
#[cfg(feature = "pqc-fips204")]
use fips204::ml_dsa_65;
#[cfg(feature = "pqc-fips204")]
use fips204::ml_dsa_87;
#[cfg(feature = "pqc-fips204")]
use fips204::traits::{SerDes, Signer};

use once_cell::sync::OnceCell;
use zeroize::Zeroizing;

// Global holder for the validator private key material (zeroized on drop)
static VALIDATOR_KEY: OnceCell<Zeroizing<Vec<u8>>> = OnceCell::new();
static VALIDATOR_PUBLIC_KEY: OnceCell<Vec<u8>> = OnceCell::new();
static VALIDATOR_ADDRESS: OnceCell<String> = OnceCell::new();
static VALIDATOR_ALGORITHM: OnceCell<String> = OnceCell::new();

#[cfg(feature = "pqc-fips204")]
fn derive_identity(secret_key: &[u8]) -> Option<(Vec<u8>, String, &'static str)> {
    if secret_key.len() == ml_dsa_65::SK_LEN {
        let mut sk_bytes = [0u8; ml_dsa_65::SK_LEN];
        sk_bytes.copy_from_slice(secret_key);
        if let Ok(sk) = ml_dsa_65::PrivateKey::try_from_bytes(sk_bytes) {
            let pk = sk.get_public_key().into_bytes().to_vec();
            let address = crate::addr::canonical_address(&pk);
            return Some((pk, address, "mldsa65"));
        }
    }

    if secret_key.len() == ml_dsa_87::SK_LEN {
        let mut sk_bytes = [0u8; ml_dsa_87::SK_LEN];
        sk_bytes.copy_from_slice(secret_key);
        if let Ok(sk) = ml_dsa_87::PrivateKey::try_from_bytes(sk_bytes) {
            let pk = sk.get_public_key().into_bytes().to_vec();
            let address = crate::addr::canonical_address(&pk);
            return Some((pk, address, "dilithium5"));
        }
    }

    None
}

#[cfg(not(feature = "pqc-fips204"))]
fn derive_identity(_secret_key: &[u8]) -> Option<(Vec<u8>, String, &'static str)> {
    None
}

/// Initialize validator key from Vault or sealed keystore based on env.
/// Returns Ok(Some(len)) when a key was loaded, Ok(None) when no key configured, Err on failure.
pub async fn init_validator_key() -> anyhow::Result<Option<usize>> {
    let validator_id = std::env::var("VALIDATOR_ID").unwrap_or_else(|_| "default".to_string());

    // Prefer DYTALLIX_ envs, fallback to generic
    let vault_url = std::env::var("DYTALLIX_VAULT_URL")
        .ok()
        .or_else(|| std::env::var("VAULT_URL").ok());
    let vault_token = std::env::var("DYTALLIX_VAULT_TOKEN")
        .ok()
        .or_else(|| std::env::var("VAULT_TOKEN").ok());

    let maybe_key = if let (Some(url), Some(token)) = (vault_url, vault_token) {
        // Vault path config
        let mount =
            std::env::var("DYTALLIX_VAULT_KV_MOUNT").unwrap_or_else(|_| "secret".to_string());
        let base = std::env::var("DYTALLIX_VAULT_PATH_BASE")
            .unwrap_or_else(|_| "dytallix/validators".to_string());
        let provider = providers::VaultProvider::new(url, token, mount, base);
        Some(provider.get_validator_key(&validator_id).await?)
    } else {
        let dir = std::env::var("DYT_KEYSTORE_DIR").unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            format!("{home}/.dytallix/keystore")
        });
        let provider = providers::SealedKeystoreProvider::new(dir);
        Some(provider.get_validator_key(&validator_id).await?)
    };

    if let Some(bytes) = maybe_key {
        let len = bytes.len();
        if let Some((public_key, address, algorithm)) = derive_identity(&bytes) {
            let _ = VALIDATOR_PUBLIC_KEY.set(public_key.clone());
            let _ = VALIDATOR_ADDRESS.set(address);
            let _ = VALIDATOR_ALGORITHM.set(algorithm.to_string());
        } else if let Ok(address) = std::env::var("DYT_VALIDATOR_ADDRESS") {
            let _ = VALIDATOR_ADDRESS.set(address);
            let _ = VALIDATOR_ALGORITHM.set("configured-address".to_string());
        }
        let secret = Zeroizing::new(bytes);
        let _ = VALIDATOR_KEY.set(secret); // ignore if already set
        return Ok(Some(len));
    }
    Ok(None)
}

/// Returns a reference to the loaded validator key (if any).
pub fn validator_key() -> Option<&'static Zeroizing<Vec<u8>>> {
    VALIDATOR_KEY.get()
}

pub fn validator_public_key_b64() -> Option<String> {
    VALIDATOR_PUBLIC_KEY.get().map(|pk| B64.encode(pk))
}

pub fn validator_address() -> Option<&'static str> {
    VALIDATOR_ADDRESS.get().map(|address| address.as_str())
}

pub fn validator_algorithm() -> Option<&'static str> {
    VALIDATOR_ALGORITHM
        .get()
        .map(|algorithm| algorithm.as_str())
}
