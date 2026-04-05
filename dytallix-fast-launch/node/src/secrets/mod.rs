pub mod providers;
use crate::secrets::providers::KeyProvider; // bring trait into scope

use once_cell::sync::OnceCell;
use zeroize::Zeroizing;

// Global holder for the validator private key material (zeroized on drop)
static VALIDATOR_KEY: OnceCell<Zeroizing<Vec<u8>>> = OnceCell::new();

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
