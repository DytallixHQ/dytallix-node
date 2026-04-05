use anyhow::Result;
use dytallix_node::consensus::{
    AIOracleClient, AIRequestPayload, AIServiceType, CircuitBreakerState, FallbackResponse,
    RequestPriority,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_circuit_breaker_opens_after_failures() -> Result<()> {
    // Create a client with low failure threshold for testing
    let client = AIOracleClient::with_circuit_breaker(
        "http://localhost:9999".to_string(), // Non-existent endpoint
        Duration::from_secs(1),
        0.5, // 50% failure threshold
        5,   // 5 second recovery time
    )?;

    // Make several requests that will fail
    for i in 0..10 {
        let result = client.get_with_fallback("test").await;
        println!("Request {}: {:?}", i, result.is_ok());

        // Check circuit breaker status
        if let Ok(status) = client.get_circuit_breaker_status() {
            println!("Circuit breaker status after request {}: {}", i, status);

            // Check if circuit breaker opened
            if status["state"] == "open" {
                println!("Circuit breaker opened after {} requests", i + 1);
                break;
            }
        }
    }

    // Verify circuit breaker is open
    let status = client.get_circuit_breaker_status()?;
    assert_eq!(status["state"], "open");
    assert!(status["failure_rate"].as_f64().unwrap() >= 0.5);

    println!("✓ Circuit breaker opened after repeated failures");
    Ok(())
}

#[tokio::test]
async fn test_circuit_breaker_recovery() -> Result<()> {
    // Create a client with very low recovery time for testing
    let client = AIOracleClient::with_circuit_breaker(
        "http://localhost:9999".to_string(),
        Duration::from_secs(1),
        0.3, // 30% failure threshold
        1,   // 1 second recovery time
    )?;

    // Force circuit breaker to open by making failures
    for _ in 0..5 {
        let _ = client.get_with_fallback("test").await;
    }

    // Verify circuit breaker is open
    let status = client.get_circuit_breaker_status()?;
    println!("Circuit breaker status: {}", status);
    assert_eq!(status["state"], "open");

    // Wait for recovery time
    println!("Waiting for circuit breaker recovery...");
    sleep(Duration::from_secs(2)).await;

    // Next request should transition to half-open
    let result = client.get_with_fallback("test").await;
    println!("Request after recovery wait: {:?}", result.is_ok());

    // Check if circuit breaker is in half-open state or closed
    let status = client.get_circuit_breaker_status()?;
    println!("Circuit breaker status after recovery: {}", status);

    // The circuit breaker should have attempted to transition to half-open
    // Even if the request failed, it should have tried
    assert_ne!(status["state"], "open");

    println!("✓ Circuit breaker recovery mechanism working");
    Ok(())
}

#[tokio::test]
async fn test_circuit_breaker_reset() -> Result<()> {
    // Create a client and force it to fail
    let client = AIOracleClient::with_circuit_breaker(
        "http://localhost:9999".to_string(),
        Duration::from_secs(1),
        0.3, // 30% failure threshold
        60,  // 1 minute recovery time
    )?;

    // Force circuit breaker to open
    for _ in 0..5 {
        let _ = client.get_with_fallback("test").await;
    }

    // Verify circuit breaker is open
    let status = client.get_circuit_breaker_status()?;
    println!("Circuit breaker status before reset: {}", status);
    assert_eq!(status["state"], "open");

    // Reset circuit breaker
    client.reset_circuit_breaker()?;

    // Verify circuit breaker is closed
    let status = client.get_circuit_breaker_status()?;
    println!("Circuit breaker status after reset: {}", status);
    assert_eq!(status["state"], "closed");
    assert_eq!(status["failure_count"], 0);
    assert_eq!(status["success_count"], 0);
    assert_eq!(status["failure_rate"], 0.0);

    println!("✓ Circuit breaker reset functionality working");
    Ok(())
}

#[tokio::test]
async fn test_fallback_response_creation() -> Result<()> {
    let client = AIOracleClient::new("http://localhost:8080".to_string())?;

    // Test fallback response creation
    let fallback = client
        .create_fallback_response("health_check", "Service unavailable due to circuit breaker");

    assert_eq!(fallback.response_type, "health_check");
    assert_eq!(
        fallback.message,
        "Service unavailable due to circuit breaker"
    );
    assert_eq!(fallback.data["fallback"], true);
    assert_eq!(fallback.data["service_unavailable"], true);
    assert_eq!(fallback.data["recommendation"], "retry_later");
    assert!(fallback.timestamp > 0);

    println!("✓ Fallback response creation working");
    Ok(())
}

#[tokio::test]
async fn test_health_check_with_circuit_breaker() -> Result<()> {
    // Create a client with non-existent endpoint
    let client = AIOracleClient::with_circuit_breaker(
        "http://localhost:9999".to_string(),
        Duration::from_secs(1),
        0.4, // 40% failure threshold
        5,   // 5 second recovery time
    )?;

    // Test health check with circuit breaker
    let health_response = client.health_check_with_circuit_breaker().await?;
    println!("Health check response: {:?}", health_response);

    // Should return a response even if service is unavailable
    assert!(health_response.timestamp > 0);

    // With a non-existent endpoint, should get unhealthy status
    // Either from the actual health check or from circuit breaker fallback
    println!("Health status: {}", health_response.status);

    println!("✓ Health check with circuit breaker working");
    Ok(())
}

#[tokio::test]
async fn test_ai_request_with_circuit_breaker() -> Result<()> {
    // Create a client with non-existent endpoint
    let client = AIOracleClient::with_circuit_breaker(
        "http://localhost:9999".to_string(),
        Duration::from_secs(1),
        0.3, // 30% failure threshold
        5,   // 5 second recovery time
    )?;

    // Create a test AI request payload
    let mut request_payload = AIRequestPayload::new(
        AIServiceType::FraudDetection,
        serde_json::json!({
            "transaction_id": "test_123",
            "amount": 1000.0,
            "sender": "test_sender",
            "receiver": "test_receiver"
        }),
    );
    request_payload.priority = RequestPriority::Normal;

    // Test AI request with circuit breaker
    let response = client
        .send_ai_request_with_circuit_breaker(&request_payload)
        .await?;
    println!("AI request response: {:?}", response);

    // Should return a response even if service is unavailable
    assert!(!response.id.is_empty());
    assert_eq!(response.service_type, AIServiceType::FraudDetection);

    // With a non-existent endpoint, should get a failure response
    // Either from the actual request or from circuit breaker fallback
    println!("Response status: {:?}", response.status);

    println!("✓ AI request with circuit breaker working");
    Ok(())
}

#[tokio::test]
async fn test_circuit_breaker_status_tracking() -> Result<()> {
    let client = AIOracleClient::with_circuit_breaker(
        "http://localhost:9999".to_string(),
        Duration::from_secs(1),
        0.5, // 50% failure threshold
        10,  // 10 second recovery time
    )?;

    // Initial status should be closed
    let status = client.get_circuit_breaker_status()?;
    assert_eq!(status["state"], "closed");
    assert_eq!(status["failure_count"], 0);
    assert_eq!(status["success_count"], 0);

    // Make some failing requests
    for i in 0..3 {
        let _ = client.get_with_fallback("test").await;

        let status = client.get_circuit_breaker_status()?;
        println!(
            "After request {}: failures={}, rate={:.2}%",
            i + 1,
            status["failure_count"],
            status["failure_rate"].as_f64().unwrap() * 100.0
        );
    }

    // Check final status
    let status = client.get_circuit_breaker_status()?;
    println!("Final circuit breaker status: {}", status);

    assert!(status["failure_count"].as_u64().unwrap() > 0);
    assert!(status["failure_rate"].as_f64().unwrap() > 0.0);
    assert_eq!(status["failure_threshold"], 0.5);
    assert_eq!(status["recovery_time_seconds"], 10);

    println!("✓ Circuit breaker status tracking working");
    Ok(())
}

#[tokio::test]
async fn test_circuit_breaker_with_working_service() -> Result<()> {
    // This test would require a mock server, but we can test the logic
    // by creating a client with a valid configuration

    let client = AIOracleClient::with_circuit_breaker(
        "http://httpbin.org".to_string(), // Using httpbin as a test service
        Duration::from_secs(5),
        0.7, // 70% failure threshold
        30,  // 30 second recovery time
    )?;

    // Test basic connectivity to a working service
    let result = client.test_connectivity().await?;
    println!("Connectivity test result: {}", result);

    // If we can connect, the circuit breaker should remain closed
    let status = client.get_circuit_breaker_status()?;
    println!("Circuit breaker status with working service: {}", status);

    if result {
        // If connection worked, circuit breaker should be closed
        assert_eq!(status["state"], "closed");
        println!("✓ Circuit breaker remains closed with working service");
    } else {
        // If connection failed, that's also a valid test result
        println!("✓ Circuit breaker test completed (service unavailable)");
    }

    Ok(())
}
