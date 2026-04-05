use dytallix_node::consensus::{
    AIOracleClient, AIRequestMetadata, AIRequestPayload, AIServiceType, RequestPriority,
};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
async fn test_ai_request_payload_demo() {
    // Create an AI Oracle client
    let client = AIOracleClient::new("https://httpbin.org".to_string()).unwrap();

    // Create a fraud detection request payload
    let transaction_data = json!({
        "transaction_id": "tx_12345",
        "from_address": "0x1234567890abcdef",
        "to_address": "0xfedcba0987654321",
        "amount": 1000.50,
        "timestamp": 1672531200,
        "gas_price": 20,
        "network": "ethereum"
    });

    let metadata = AIRequestMetadata::new("blockchain_consensus".to_string(), "1.0.0".to_string())
        .with_correlation_id("batch_001".to_string())
        .with_requester("validator_node_01".to_string())
        .with_context(json!({"block_height": 1234567}));

    let payload = AIRequestPayload::fraud_detection(transaction_data)
        .with_priority(RequestPriority::High)
        .with_timeout(30)
        .with_metadata(metadata)
        .with_signature("mock_signature_123".to_string());

    // Validate the payload
    assert!(payload.validate().is_ok());

    // Test serialization
    let json_str = payload.to_json().unwrap();
    println!(
        "Request Payload JSON:\n{}",
        payload.to_json_pretty().unwrap()
    );

    // Verify structure
    assert!(!payload.id.is_empty());
    assert_eq!(payload.service_type, AIServiceType::FraudDetection);
    assert_eq!(payload.priority, RequestPriority::High);
    assert_eq!(payload.timeout, Some(30));
    assert!(payload.metadata.is_some());
    assert!(payload.signature.is_some());

    // Test that it can be sent (this will fail because httpbin doesn't have AI endpoints)
    let result = client.send_ai_request(&payload).await;
    // The request should be attempted but will fail with 404
    println!("Request attempt status: {:?}", result.is_ok());
}

#[test]
fn test_ai_request_payload_builder_patterns() {
    // Test different service types
    let fraud_payload = AIRequestPayload::fraud_detection(json!({"tx": "123"}));
    assert_eq!(fraud_payload.service_type, AIServiceType::FraudDetection);

    let risk_payload = AIRequestPayload::risk_scoring(json!({"score": 0.75}));
    assert_eq!(risk_payload.service_type, AIServiceType::RiskScoring);

    let contract_payload = AIRequestPayload::contract_analysis(json!({"code": "0x1234"}));
    assert_eq!(
        contract_payload.service_type,
        AIServiceType::ContractAnalysis
    );

    let validation_payload = AIRequestPayload::transaction_validation(json!({"tx": "456"}));
    assert_eq!(
        validation_payload.service_type,
        AIServiceType::TransactionValidation
    );

    // Test builder pattern with all options
    let full_payload = AIRequestPayload::new(
        AIServiceType::AML,
        json!({"address": "0x123", "amount": 50000}),
    )
    .with_priority(RequestPriority::Critical)
    .with_timeout(45)
    .with_callback("https://callback.example.com/webhook".to_string())
    .with_metadata(
        AIRequestMetadata::new("compliance".to_string(), "2.0".to_string())
            .with_requester("compliance_officer".to_string()),
    );

    assert_eq!(full_payload.service_type, AIServiceType::AML);
    assert_eq!(full_payload.priority, RequestPriority::Critical);
    assert_eq!(full_payload.timeout, Some(45));
    assert!(full_payload.callback_url.is_some());
    assert!(full_payload.metadata.is_some());

    println!("Full payload created successfully with all features");
}
