//! Node configuration using the secrets management system
//!
//! This module demonstrates how to integrate the secrets system into
//! existing Dytallix components.

use crate::policy::SignaturePolicy;
use crate::secrets::{SecretError, SecretManager, SecretResult};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Node configuration loaded from secrets and environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    // Network configuration
    pub bind_address: String,
    pub port: u16,
    pub p2p_port: u16,

    // Database configuration
    pub database_url: String,
    pub database_pool_size: u32,

    // API configuration
    pub api_key: String,
    pub jwt_secret: String,
    pub rate_limit: u32,

    // Logging configuration
    pub log_level: String,
    pub debug_mode: bool,

    // Security configuration
    pub require_tls: bool,
    pub min_tls_version: String,
    pub audit_logging: bool,

    // PQC configuration
    pub pqc_keys_path: String,
    pub pqc_algorithm: String,

    // Signature Policy configuration
    pub signature_policy: SignaturePolicy,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0".to_string(),
            port: 8080,
            p2p_port: 30303,
            database_url: "sqlite://dytallix.db".to_string(),
            database_pool_size: 10,
            api_key: "placeholder_api_key".to_string(),
            jwt_secret: "placeholder_jwt_secret".to_string(),
            rate_limit: 1000,
            log_level: "info".to_string(),
            debug_mode: false,
            require_tls: true,
            min_tls_version: "1.2".to_string(),
            audit_logging: true,
            pqc_keys_path: "./pqc_keys.json".to_string(),
            pqc_algorithm: "Dilithium3".to_string(),
            signature_policy: SignaturePolicy::default(),
        }
    }
}

impl NodeConfig {
    /// Load configuration using the secrets manager
    ///
    /// This method demonstrates the recommended pattern for loading
    /// configuration with secrets fallback.
    pub async fn load_with_secrets(secret_manager: &SecretManager) -> SecretResult<Self> {
        info!("Loading node configuration using secrets manager");

        let mut config = Self::default();

        // Network configuration
        config.bind_address = secret_manager
            .get_secret_or_default("BIND_ADDRESS", &config.bind_address)
            .await;

        if let Ok(port_str) = secret_manager.get_secret("PORT").await {
            if let Ok(port) = port_str.parse::<u16>() {
                config.port = port;
            } else {
                warn!("Invalid PORT value in secrets: {}", port_str);
            }
        }

        if let Ok(p2p_port_str) = secret_manager.get_secret("P2P_PORT").await {
            if let Ok(p2p_port) = p2p_port_str.parse::<u16>() {
                config.p2p_port = p2p_port;
            } else {
                warn!("Invalid P2P_PORT value in secrets: {}", p2p_port_str);
            }
        }

        // Database configuration - construct URL from components
        let db_host = secret_manager
            .get_secret_or_default("database/host", "localhost")
            .await;
        let db_port = secret_manager
            .get_secret_or_default("database/port", "5432")
            .await;
        let db_name = secret_manager
            .get_secret_or_default("database/database", "dytallix")
            .await;
        let db_username = secret_manager
            .get_secret_or_default("database/username", "dytallix")
            .await;

        // Get password from secrets (this should never have a default)
        match secret_manager.get_secret("database/password").await {
            Ok(db_password) => {
                config.database_url = format!(
                    "postgresql://{db_username}:{db_password}@{db_host}:{db_port}/{db_name}"
                );
                debug!("Database URL constructed from secrets");
            }
            Err(_) => {
                warn!("Database password not found in secrets, using default SQLite");
                // Keep default SQLite URL
            }
        }

        // API configuration
        match secret_manager.get_secret("api/api_key").await {
            Ok(api_key) => {
                config.api_key = api_key;
                debug!("API key loaded from secrets");
            }
            Err(_) => {
                warn!("API key not found in secrets, using placeholder");
            }
        }

        match secret_manager.get_secret("api/jwt_secret").await {
            Ok(jwt_secret) => {
                config.jwt_secret = jwt_secret;
                debug!("JWT secret loaded from secrets");
            }
            Err(_) => {
                warn!("JWT secret not found in secrets, using placeholder");
            }
        }

        if let Ok(rate_limit_str) = secret_manager.get_secret("api/rate_limit").await {
            if let Ok(rate_limit) = rate_limit_str.parse::<u32>() {
                config.rate_limit = rate_limit;
            }
        }

        // Logging configuration
        config.log_level = secret_manager
            .get_secret_or_default("config/log_level", &config.log_level)
            .await;

        if let Ok(debug_str) = secret_manager.get_secret("config/debug_mode").await {
            config.debug_mode = debug_str.parse::<bool>().unwrap_or(false);
        }

        // Security configuration
        config.require_tls = secret_manager
            .get_secret_or_default("REQUIRE_TLS", &config.require_tls.to_string())
            .await
            .parse::<bool>()
            .unwrap_or(true);

        config.min_tls_version = secret_manager
            .get_secret_or_default("MIN_TLS_VERSION", &config.min_tls_version)
            .await;

        config.audit_logging = secret_manager
            .get_secret_or_default("AUDIT_LOGGING", &config.audit_logging.to_string())
            .await
            .parse::<bool>()
            .unwrap_or(true);

        // PQC configuration
        config.pqc_keys_path = secret_manager
            .get_secret_or_default("PQC_KEYS_PATH", &config.pqc_keys_path)
            .await;

        config.pqc_algorithm = secret_manager
            .get_secret_or_default("PREFERRED_SIGNATURE_ALGORITHM", &config.pqc_algorithm)
            .await;

        // Signature Policy configuration
        if let Ok(reject_legacy_str) = secret_manager
            .get_secret("SIGNATURE_POLICY_REJECT_LEGACY")
            .await
        {
            config.signature_policy.reject_legacy =
                reject_legacy_str.parse::<bool>().unwrap_or(true);
        }

        if let Ok(enforce_mempool_str) = secret_manager
            .get_secret("SIGNATURE_POLICY_ENFORCE_MEMPOOL")
            .await
        {
            config.signature_policy.enforce_at_mempool =
                enforce_mempool_str.parse::<bool>().unwrap_or(true);
        }

        if let Ok(enforce_consensus_str) = secret_manager
            .get_secret("SIGNATURE_POLICY_ENFORCE_CONSENSUS")
            .await
        {
            config.signature_policy.enforce_at_consensus =
                enforce_consensus_str.parse::<bool>().unwrap_or(true);
        }

        // Parse allowed algorithms from comma-separated list
        if let Ok(allowed_algs_str) = secret_manager
            .get_secret("SIGNATURE_POLICY_ALLOWED_ALGORITHMS")
            .await
        {
            use std::collections::HashSet;

            let mut allowed = HashSet::new();
            for alg_name in allowed_algs_str.split(',') {
                let alg_name = alg_name.trim();
                match config.signature_policy.validate_algorithm_name(alg_name) {
                    Ok(alg) => {
                        allowed.insert(alg);
                    }
                    Err(e) => warn!("Invalid algorithm in config '{}': {}", alg_name, e),
                }
            }
            if !allowed.is_empty() {
                config.signature_policy.allowed_algorithms = allowed;
            }
        }

        info!("Node configuration loaded successfully");
        debug!("Configuration: {:?}", config);

        Ok(config)
    }

    /// Load configuration the traditional way (for comparison)
    pub fn load_from_env() -> Self {
        use std::env;

        let mut config = Self::default();

        if let Ok(bind_address) = env::var("DYTALLIX_BIND_ADDRESS") {
            config.bind_address = bind_address;
        }

        if let Ok(port_str) = env::var("DYTALLIX_PORT") {
            if let Ok(port) = port_str.parse::<u16>() {
                config.port = port;
            }
        }

        // ... more environment variable loading ...
        // This approach requires manual handling of each variable
        // and doesn't support hierarchical secrets or multiple providers

        config
    }

    /// Validate the configuration
    pub fn validate(&self) -> SecretResult<()> {
        if self.port == 0 {
            return Err(SecretError::ConfigError {
                message: "Port cannot be 0".to_string(),
            });
        }

        if self.api_key == "placeholder_api_key" {
            warn!("Using placeholder API key - this is insecure for production");
        }

        if self.jwt_secret == "placeholder_jwt_secret" {
            warn!("Using placeholder JWT secret - this is insecure for production");
        }

        if self.jwt_secret.len() < 32 {
            return Err(SecretError::ConfigError {
                message: "JWT secret must be at least 32 characters".to_string(),
            });
        }

        Ok(())
    }

    /// Check if configuration is production-ready
    pub fn is_production_ready(&self) -> bool {
        !self.api_key.contains("placeholder")
            && !self.jwt_secret.contains("placeholder")
            && self.jwt_secret.len() >= 32
            && self.require_tls
            && self.audit_logging
    }
}

/// Configuration loader utility
pub struct ConfigLoader {
    secret_manager: SecretManager,
}

impl ConfigLoader {
    /// Create a new configuration loader
    pub async fn new() -> SecretResult<Self> {
        let mut secret_manager = SecretManager::from_env()?;
        secret_manager.initialize().await?;

        Ok(Self { secret_manager })
    }

    /// Load node configuration
    pub async fn load_node_config(&self) -> SecretResult<NodeConfig> {
        NodeConfig::load_with_secrets(&self.secret_manager).await
    }

    /// Get the underlying secret manager
    pub fn secret_manager(&self) -> &SecretManager {
        &self.secret_manager
    }

    /// Perform health check on secret providers
    pub async fn health_check(&self) -> std::collections::HashMap<String, bool> {
        self.secret_manager.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::SecretConfig;
    use std::env;

    #[tokio::test]
    async fn test_node_config_loading() {
        // Set up test environment
        env::set_var("TEST_BIND_ADDRESS", "127.0.0.1");
        env::set_var("TEST_PORT", "9090");
        env::set_var("TEST_API_KEY", "test_api_key_123");

        let config = SecretConfig::for_testing();
        let mut manager = SecretManager::new(config).unwrap();
        manager.initialize().await.unwrap();

        let node_config = NodeConfig::load_with_secrets(&manager).await.unwrap();

        assert_eq!(node_config.bind_address, "127.0.0.1");
        assert_eq!(node_config.port, 9090);
        assert_eq!(node_config.api_key, "test_api_key_123");

        // Clean up
        env::remove_var("TEST_BIND_ADDRESS");
        env::remove_var("TEST_PORT");
        env::remove_var("TEST_API_KEY");
    }

    #[tokio::test]
    async fn test_config_validation() {
        let config = NodeConfig::default();
        assert!(config.validate().is_ok());

        let invalid_config = NodeConfig {
            port: 0,
            ..Default::default()
        };
        assert!(invalid_config.validate().is_err());

        let short_jwt_config = NodeConfig {
            jwt_secret: "short".to_string(),
            ..Default::default()
        };
        assert!(short_jwt_config.validate().is_err());
    }

    #[test]
    fn test_production_readiness() {
        let dev_config = NodeConfig::default();
        assert!(!dev_config.is_production_ready());

        let prod_config = NodeConfig {
            api_key: "real_api_key_12345678901234567890".to_string(),
            jwt_secret: "real_jwt_secret_1234567890123456789012345678901234567890".to_string(),
            ..Default::default()
        };
        assert!(prod_config.is_production_ready());
    }
}
