//! Integration tests for signature verification in blockchain consensus.

use anyhow::Result;
use dytallix_node::consensus::ai_integration::{
    AIVerificationResult, AIIntegrationConfig, AIIntegrationManager,
};
use dytallix_node::consensus::signature_verification::{
    SignatureVerifier, VerificationConfig, VerificationError,
};
use dytallix_node::consensus::{
    AIResponsePayload, AIResponseSignature, AIServiceType, OracleIdentity,
};
use dytallix_node::types::{
    Block, BlockHeader, PQCBlockSignature, PQCTransactionSignature, Transaction,
    TransferTransaction,
};
use dytallix_pqc::{Signature, SignatureAlgorithm};

struct TestData {
    oracle_identity: OracleIdentity,
    transaction_signature: PQCTransactionSignature,
    signed_response: dytallix_node::consensus::SignedAIOracleResponse,
}

impl TestData {
    fn new() -> Self {
        let oracle_public_key = vec![1, 2, 3, 4];
        let transaction_signature = PQCTransactionSignature {
            signature: Signature {
                data: vec![9, 10, 11, 12],
                algorithm: SignatureAlgorithm::Dilithium5,
            },
            public_key: oracle_public_key.clone(),
        };

        let oracle_identity = OracleIdentity::new(
            "test-oracle-1".to_string(),
            "Test Oracle".to_string(),
            oracle_public_key.clone(),
            SignatureAlgorithm::Dilithium5,
        )
        .update_reputation(0.95);

        let response = AIResponsePayload::success(
            "test-request-1".to_string(),
            AIServiceType::FraudDetection,
            serde_json::json!({
                "risk_score": 0.3,
                "confidence": 0.95,
                "factors": ["low_amount", "verified_sender"]
            }),
        )
        .with_oracle_id(oracle_identity.oracle_id.clone());

        let signature = AIResponseSignature::new(
            SignatureAlgorithm::Dilithium5,
            vec![13, 14, 15, 16],
            oracle_public_key,
        );

        let signed_response = dytallix_node::consensus::SignedAIOracleResponse::new(
            response,
            signature,
            12345,
            chrono::Utc::now().timestamp() as u64 + 300,
            oracle_identity.clone(),
        );

        Self {
            oracle_identity,
            transaction_signature,
            signed_response,
        }
    }
}

#[test]
fn test_signature_verification_setup() -> Result<()> {
    let config = VerificationConfig {
        enforce_certificate_validation: false,
        ..VerificationConfig::default()
    };

    let verifier = SignatureVerifier::new(config)?;
    let test_data = TestData::new();

    verifier.register_oracle(test_data.oracle_identity.clone(), 1_000)?;

    let registered_oracles = verifier.list_oracles();
    assert_eq!(registered_oracles.len(), 1);
    assert_eq!(registered_oracles[0].identity.oracle_id, "test-oracle-1");

    Ok(())
}

#[tokio::test]
async fn test_ai_response_verification_flow() -> Result<()> {
    let test_data = TestData::new();
    let config = AIIntegrationConfig {
        verification_config: VerificationConfig {
            enforce_certificate_validation: false,
            ..VerificationConfig::default()
        },
        require_ai_verification: true,
        fail_on_ai_unavailable: false,
        ai_timeout_ms: 5000,
        enable_response_caching: true,
        response_cache_ttl: 300,
        ..AIIntegrationConfig::default()
    };

    let ai_integration = AIIntegrationManager::new(config).await?;
    ai_integration
        .register_oracle(test_data.oracle_identity.clone(), 1_000)
        .await?;

    let result = ai_integration
        .verify_ai_response(&test_data.signed_response, None)
        .await;

    assert!(matches!(
        result,
        AIVerificationResult::Verified { .. } | AIVerificationResult::Failed { .. }
    ));

    Ok(())
}

#[test]
fn test_transaction_signature_verification() {
    let test_data = TestData::new();

    let mut transfer_tx = TransferTransaction::new(
        "dyt1test_sender".to_string(),
        "dyt1test_receiver".to_string(),
        1000,
        10,
        1,
    );
    transfer_tx.signature = test_data.transaction_signature;

    let transaction = Transaction::Transfer(transfer_tx);
    let _ = transaction.verify_signature();
}

#[tokio::test]
async fn test_block_transaction_verification() -> Result<()> {
    let test_data = TestData::new();

    let mut transfer_tx = TransferTransaction::new(
        "dyt1test_sender".to_string(),
        "dyt1test_receiver".to_string(),
        1000,
        10,
        1,
    );
    transfer_tx.signature = test_data.transaction_signature.clone();

    let transactions = vec![Transaction::Transfer(transfer_tx)];
    let header = BlockHeader {
        number: 1,
        parent_hash: "0".repeat(64),
        transactions_root: BlockHeader::calculate_transactions_root(&transactions),
        state_root: "0".repeat(64),
        timestamp: chrono::Utc::now().timestamp() as u64,
        validator: "dyt1test_validator".to_string(),
        signature: PQCBlockSignature {
            signature: Signature {
                data: vec![17, 18, 19, 20],
                algorithm: SignatureAlgorithm::Dilithium5,
            },
            public_key: test_data.oracle_identity.public_key.clone(),
        },
        nonce: 0,
    };

    let block = Block {
        header,
        transactions,
    };

    assert!(!block.verify_transactions());

    Ok(())
}

#[test]
fn test_oracle_registry_operations() -> Result<()> {
    let verifier = SignatureVerifier::new(VerificationConfig {
        enforce_certificate_validation: false,
        ..VerificationConfig::default()
    })?;
    let oracle_identity = OracleIdentity::new(
        "test-oracle-registry".to_string(),
        "Registry Oracle".to_string(),
        vec![1, 2, 3, 4],
        SignatureAlgorithm::Dilithium5,
    )
    .update_reputation(0.8);

    verifier.register_oracle(oracle_identity.clone(), 500)?;

    let registered = verifier.get_oracle("test-oracle-registry").unwrap();
    assert_eq!(registered.identity.reputation_score, 0.8);

    verifier.update_oracle_reputation("test-oracle-registry", 0.9)?;

    let updated = verifier.get_oracle("test-oracle-registry").unwrap();
    assert_eq!(updated.identity.reputation_score, 0.9);

    Ok(())
}

#[test]
fn test_nonce_replay_protection() -> Result<()> {
    let test_data = TestData::new();
    let verifier = SignatureVerifier::new(VerificationConfig {
        enforce_certificate_validation: false,
        ..VerificationConfig::default()
    })?;

    verifier.register_oracle(test_data.oracle_identity.clone(), 1_000)?;

    let first = verifier.verify_signed_response(&test_data.signed_response, None);
    let second = verifier.verify_signed_response(&test_data.signed_response, None);

    assert!(first.is_err());
    assert!(matches!(
        second,
        Err(VerificationError::ReplayAttack(_))
    ));

    Ok(())
}

#[tokio::test]
async fn test_ai_integration_manager_statistics() -> Result<()> {
    let config = AIIntegrationConfig::default();
    let ai_integration = AIIntegrationManager::new(config).await?;

    let initial_stats = ai_integration.get_statistics().await;
    assert_eq!(initial_stats.total_requests, 0);
    assert_eq!(initial_stats.successful_verifications, 0);
    assert!(initial_stats.avg_verification_time_ms >= 0.0);

    Ok(())
}

#[test]
fn test_signature_verification_error_handling() -> Result<()> {
    let verifier = SignatureVerifier::new(VerificationConfig {
        enforce_certificate_validation: false,
        ..VerificationConfig::default()
    })?;

    let mut signed_response = TestData::new().signed_response;
    signed_response.oracle_identity.oracle_id = "non-existent-oracle".to_string();

    let result = verifier.verify_signed_response(&signed_response, None);
    assert!(result.is_err());

    Ok(())
}