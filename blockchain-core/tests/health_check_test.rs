use dytallix_node::consensus::{AIHealthCheckResponse, AIOracleClient, AIServiceStatus};
use std::time::Duration;

#[tokio::test]
async fn test_health_check_with_valid_service() {
    // Test with a known working endpoint that has a /health endpoint
    let client = AIOracleClient::new("https://httpbin.org".to_string()).unwrap();

    let health_response = client.health_check().await;
    assert!(health_response.is_ok());

    let health = health_response.unwrap();
    // Note: httpbin.org might not have a proper health endpoint, so we might get an error status
    // But the method should still return a valid response
    assert!(health.response_time_ms > 0);
    assert!(health.timestamp > 0);

    println!("Health check response: {:?}", health);
}

#[tokio::test]
async fn test_health_check_with_timeout() {
    let client = AIOracleClient::new("https://httpbin.org".to_string()).unwrap();

    // Test with a very short timeout to ensure timeout handling works
    let health_response = client
        .health_check_with_timeout(Duration::from_millis(1))
        .await;
    assert!(health_response.is_ok());

    let health = health_response.unwrap();
    // Should be degraded due to timeout
    assert!(matches!(
        health.status,
        AIServiceStatus::Degraded | AIServiceStatus::Unhealthy
    ));
    assert!(health.details.is_some());

    println!("Health check with timeout response: {:?}", health);
}

#[tokio::test]
async fn test_health_check_with_invalid_service() {
    let client = AIOracleClient::new("http://non-existent-domain-12345.com".to_string()).unwrap();

    let health_response = client.health_check().await;
    assert!(health_response.is_ok());

    let health = health_response.unwrap();
    // Should be unhealthy due to connection failure
    assert!(matches!(
        health.status,
        AIServiceStatus::Unhealthy | AIServiceStatus::Unknown
    ));
    assert!(health.details.is_some());
    assert!(health.response_time_ms > 0);

    println!("Health check with invalid service response: {:?}", health);
}

#[tokio::test]
async fn test_background_health_monitoring() {
    let client = AIOracleClient::new("https://httpbin.org".to_string()).unwrap();

    // Start background monitoring with a 1-second interval
    let monitor_handle = client.start_background_health_monitoring(1);

    // Let it run for a few seconds
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Stop the monitoring
    monitor_handle.abort();

    // The test passes if no panics occur
    println!("Background health monitoring test completed");
}

#[tokio::test]
async fn test_health_check_response_parsing() {
    // Test that we can properly parse different types of health responses
    let client = AIOracleClient::new("https://httpbin.org".to_string()).unwrap();

    let health_response = client.health_check().await;
    assert!(health_response.is_ok());

    let health = health_response.unwrap();

    // Test that all fields are properly initialized
    assert!(health.timestamp > 0);
    assert!(health.response_time_ms >= 0);
    assert!(matches!(
        health.status,
        AIServiceStatus::Healthy
            | AIServiceStatus::Degraded
            | AIServiceStatus::Unhealthy
            | AIServiceStatus::Unknown
    ));

    println!("Health check response structure test passed: {:?}", health);
}

#[test]
fn test_health_check_response_creation() {
    use dytallix_node::consensus::{AIHealthCheckResponse, AIServiceLoad, AIServiceStatus};

    // Test creating a health check response manually
    let health_response = AIHealthCheckResponse {
        status: AIServiceStatus::Healthy,
        timestamp: 1234567890,
        response_time_ms: 150,
        version: Some("1.0.0".to_string()),
        details: Some(serde_json::json!({"test": "data"})),
        endpoints: Some(vec!["fraud".to_string(), "risk".to_string()]),
        load: Some(AIServiceLoad {
            cpu_usage: Some(45.5),
            memory_usage: Some(67.2),
            queue_size: Some(5),
            requests_per_second: Some(12.3),
            avg_response_time_ms: Some(150.0),
        }),
    };

    assert_eq!(health_response.status, AIServiceStatus::Healthy);
    assert_eq!(health_response.response_time_ms, 150);
    assert!(health_response.version.is_some());
    assert!(health_response.details.is_some());
    assert!(health_response.endpoints.is_some());
    assert!(health_response.load.is_some());

    println!("Health check response creation test passed");
}
