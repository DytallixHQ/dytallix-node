use dytallix_node::consensus::{
    AIOracleClient, AIRequestMetadata, AIRequestPayload, AIServiceConfig, AIServiceType,
    RequestPriority,
};
use serde_json::json;
use std::collections::HashMap;

#[tokio::test]
async fn test_ai_request_payload_demo() {
    let config = AIServiceConfig {
        base_url: "https://httpbin.org".to_string(),
        timeout_seconds: 30,
        api_key: "test-key".to_string(),
        risk_threshold: 0.7,
        max_retries: 1,
        retry_delay_ms: 1,
    };
    let client = AIOracleClient::new(config.clone());

    let metadata = AIRequestMetadata {
        client_version: Some("1.0.0".to_string()),
        request_source: Some("blockchain_consensus".to_string()),
        context: Some(json!({"block_height": 1_234_567})),
        tags: Some(vec!["demo".to_string(), "fraud_detection".to_string()]),
        session_id: Some("batch_001".to_string()),
    };

    let payload = AIRequestPayload {
        id: "req_demo_payload".to_string(),
        service_type: AIServiceType::FraudDetection,
        request_data: json!({
            "transaction_id": "tx_12345",
            "from_address": "0x1234567890abcdef",
            "to_address": "0xfedcba0987654321",
            "amount": 1000.50,
            "timestamp": 1672531200,
            "gas_price": 20,
            "network": "ethereum"
        }),
        timestamp: chrono::Utc::now().timestamp() as u64,
        priority: RequestPriority::High,
        timeout_ms: 30_000,
        metadata: Some(metadata),
        callback_url: Some("https://callback.example.com/webhook".to_string()),
        requester_id: "validator_node_01".to_string(),
        correlation_id: Some("batch_001".to_string()),
        nonce: "nonce_demo_payload".to_string(),
    };

    assert!(payload.to_json().is_ok());
    let json_str = payload.to_json().unwrap();
    let roundtrip = AIRequestPayload::from_json(&json_str).unwrap();
    assert_eq!(roundtrip.service_type, AIServiceType::FraudDetection);
    assert_eq!(roundtrip.priority, RequestPriority::High);
    assert!(roundtrip.metadata.is_some());

    let mut request_data = HashMap::new();
    request_data.insert("transaction_id".to_string(), json!("tx_12345"));
    request_data.insert("network".to_string(), json!("ethereum"));

    let response = client
        .request_ai_analysis(AIServiceType::FraudDetection, request_data)
        .await
        .expect("mock analysis request should succeed");

    assert_eq!(client.get_config().base_url, config.base_url);
    assert_eq!(response.response.service_type, AIServiceType::FraudDetection);
    assert_eq!(response.oracle_identity.oracle_id, "mock_oracle");
}

#[test]
fn test_ai_request_payload_builder_patterns() {
    let fraud_payload = AIRequestPayload::new(
        AIServiceType::FraudDetection,
        json!({"tx": "123"}),
    );
    assert_eq!(fraud_payload.service_type, AIServiceType::FraudDetection);
    assert_eq!(fraud_payload.priority, RequestPriority::Normal);

    let risk_payload = AIRequestPayload::new(
        AIServiceType::RiskScoring,
        json!({"score": 0.75}),
    );
    assert_eq!(risk_payload.service_type, AIServiceType::RiskScoring);

    let contract_payload = AIRequestPayload::new(
        AIServiceType::ContractAnalysis,
        json!({"code": "0x1234"}),
    );
    assert_eq!(contract_payload.service_type, AIServiceType::ContractAnalysis);

    let validation_payload = AIRequestPayload::new(
        AIServiceType::TransactionValidation,
        json!({"tx": "456"}),
    );
    assert_eq!(
        validation_payload.service_type,
        AIServiceType::TransactionValidation
    );

    let metadata = AIRequestMetadata {
        client_version: Some("2.0".to_string()),
        request_source: Some("compliance".to_string()),
        context: Some(json!({"requester": "compliance_officer"})),
        tags: Some(vec!["aml".to_string()]),
        session_id: Some("compliance-session".to_string()),
    };

    let full_payload = AIRequestPayload {
        priority: RequestPriority::Critical,
        timeout_ms: 45_000,
        callback_url: Some("https://callback.example.com/webhook".to_string()),
        metadata: Some(metadata),
        requester_id: "compliance_officer".to_string(),
        correlation_id: Some("aml-review-001".to_string()),
        nonce: "nonce_full_payload".to_string(),
        ..AIRequestPayload::new(
            AIServiceType::AML,
            json!({"address": "0x123", "amount": 50000}),
        )
    };

    assert_eq!(full_payload.service_type, AIServiceType::AML);
    assert_eq!(full_payload.priority, RequestPriority::Critical);
    assert_eq!(full_payload.timeout_ms, 45_000);
    assert!(full_payload.callback_url.is_some());
    assert!(full_payload.metadata.is_some());
}