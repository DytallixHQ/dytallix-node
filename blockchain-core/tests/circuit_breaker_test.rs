use anyhow::Result;
use dytallix_node::consensus::{AIOracleClient, AIServiceConfig, AIServiceType};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_ai_oracle_client_default_configuration_is_sane() {
    let client = AIOracleClient::default();
    let config = client.get_config();

    assert!(!config.base_url.is_empty());
    assert!(config.timeout_seconds > 0);
    assert!(config.max_retries > 0);
}

#[tokio::test]
async fn test_request_analysis_placeholder_response_contains_expected_fields() -> Result<()> {
    let client = AIOracleClient::new(AIServiceConfig {
        base_url: "http://localhost:8080".to_string(),
        timeout_seconds: 5,
        api_key: "test-key".to_string(),
        risk_threshold: 0.7,
        max_retries: 1,
        retry_delay_ms: 1,
    });

    let mut request_data = HashMap::new();
    request_data.insert("transaction_id".to_string(), json!("test_123"));
    request_data.insert("sender".to_string(), json!("test_sender"));

    let response = client
        .request_ai_analysis(AIServiceType::FraudDetection, request_data)
        .await?;

    assert_eq!(response.response.service_type, AIServiceType::FraudDetection);
    assert!(response.response.is_successful());
    assert_eq!(response.oracle_identity.oracle_id, "mock_oracle");
    assert!(response.expires_at > response.response.timestamp);

    Ok(())
}
