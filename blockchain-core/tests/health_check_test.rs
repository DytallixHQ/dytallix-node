use dytallix_node::consensus::{AIOracleClient, AIServiceConfig, AIServiceType};
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_ai_oracle_client_configuration_accessors() {
    let mut client = AIOracleClient::new(AIServiceConfig {
        base_url: "https://httpbin.org".to_string(),
        timeout_seconds: 15,
        api_key: "test-key".to_string(),
        risk_threshold: 0.7,
        max_retries: 2,
        retry_delay_ms: 50,
    });

    assert_eq!(client.base_url(), "https://httpbin.org");
    assert_eq!(client.timeout().as_secs(), 15);

    client.set_timeout(5);
    assert_eq!(client.get_config().timeout_seconds, 5);
}

#[tokio::test]
async fn test_request_ai_analysis_returns_mock_signed_response() {
    let client = AIOracleClient::default();
    let mut data = HashMap::new();
    data.insert("transaction_id".to_string(), json!("tx_123"));
    data.insert("amount".to_string(), json!(1000));

    let response = client
        .request_ai_analysis(AIServiceType::FraudDetection, data)
        .await
        .expect("mock request analysis should succeed");

    assert_eq!(response.response.service_type, AIServiceType::FraudDetection);
    assert_eq!(response.oracle_identity.oracle_id, "mock_oracle");
    assert!(!response.response.id.is_empty());
}
