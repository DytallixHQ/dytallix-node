use anyhow::Result;
use dytallix_node::consensus::{AIOracleClient, AIRequestPayload, AIServiceType, RequestPriority};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    env_logger::init();

    println!("=== Circuit Breaker Pattern Demo ===\n");

    // Create a client with circuit breaker configuration
    let client = AIOracleClient::with_circuit_breaker(
        "http://localhost:9999".to_string(), // Non-existent endpoint to trigger failures
        Duration::from_secs(2),
        0.5, // 50% failure threshold
        5,   // 5 second recovery time
    )?;

    println!("✅ Created AIOracleClient with circuit breaker:");
    println!("   - Base URL: http://localhost:9999 (non-existent)");
    println!("   - Failure threshold: 50%");
    println!("   - Recovery time: 5 seconds");
    println!("   - Timeout: 2 seconds\n");

    // Show initial circuit breaker status
    demonstrate_circuit_breaker_status(&client, "Initial").await?;

    // Phase 1: Make requests that will fail to trigger circuit breaker
    println!("🔄 Phase 1: Making failing requests to trigger circuit breaker...\n");

    for i in 1..=8 {
        println!("Making request #{}", i);

        let start = std::time::Instant::now();
        let result = client.get_with_fallback("health").await;
        let elapsed = start.elapsed();

        match result {
            Ok(_) => println!("   ✅ Request succeeded (unexpected)"),
            Err(e) => {
                if e.to_string().contains("circuit breaker") {
                    println!("   🚫 Request blocked by circuit breaker: {}", e);
                } else {
                    println!("   ❌ Request failed: {}", e);
                }
            }
        }

        println!("   ⏱️  Time taken: {:?}", elapsed);

        // Show circuit breaker status after each request
        demonstrate_circuit_breaker_status(&client, &format!("After request {}", i)).await?;

        sleep(Duration::from_millis(200)).await;
    }

    // Phase 2: Wait for circuit breaker recovery
    println!("\n🕐 Phase 2: Waiting for circuit breaker recovery...\n");

    let status = client.get_circuit_breaker_status()?;
    if status["state"] == "open" {
        println!("Circuit breaker is open. Waiting for recovery time...");

        // Show countdown
        for i in (1..=5).rev() {
            println!("   Recovery in {} seconds...", i);
            sleep(Duration::from_secs(1)).await;
        }

        println!("   Recovery time elapsed!\n");
    }

    // Phase 3: Test circuit breaker recovery
    println!("🔄 Phase 3: Testing circuit breaker recovery...\n");

    for i in 1..=3 {
        println!("Making recovery test request #{}", i);

        let start = std::time::Instant::now();
        let result = client.get_with_fallback("health").await;
        let elapsed = start.elapsed();

        match result {
            Ok(_) => println!("   ✅ Request succeeded (unexpected)"),
            Err(e) => {
                if e.to_string().contains("circuit breaker") {
                    println!("   🚫 Request blocked by circuit breaker: {}", e);
                } else {
                    println!("   ❌ Request failed: {}", e);
                }
            }
        }

        println!("   ⏱️  Time taken: {:?}", elapsed);

        // Show circuit breaker status after each request
        demonstrate_circuit_breaker_status(&client, &format!("After recovery test {}", i)).await?;

        sleep(Duration::from_millis(500)).await;
    }

    // Phase 4: Test AI request with circuit breaker
    println!("\n🔄 Phase 4: Testing AI request with circuit breaker...\n");

    let mut request_payload = AIRequestPayload::new(
        AIServiceType::FraudDetection,
        serde_json::json!({
            "transaction_id": "demo_tx_123",
            "amount": 1000.0,
            "sender": "demo_sender",
            "receiver": "demo_receiver"
        }),
    );
    request_payload.priority = RequestPriority::High;

    println!("Making AI request with circuit breaker protection...");
    let start = std::time::Instant::now();
    let response = client
        .send_ai_request_with_circuit_breaker(&request_payload)
        .await?;
    let elapsed = start.elapsed();

    println!("   ⏱️  Time taken: {:?}", elapsed);
    println!("   📊 Response ID: {}", response.id);
    println!("   📊 Service Type: {:?}", response.service_type);
    println!("   📊 Status: {:?}", response.status);

    if let Some(error) = &response.error {
        println!("   📊 Error: {} - {}", error.code, error.message);
        println!("   📊 Retryable: {}", error.retryable);
    }

    // Phase 5: Test fallback response creation
    println!("\n🔄 Phase 5: Testing fallback response creation...\n");

    let fallback = client.create_fallback_response(
        "fraud_detection",
        "Service temporarily unavailable due to circuit breaker protection",
    );

    println!("Created fallback response:");
    println!("   📊 Type: {}", fallback.response_type);
    println!("   📊 Message: {}", fallback.message);
    println!("   📊 Data: {}", fallback.data);
    println!("   📊 Timestamp: {}", fallback.timestamp);

    // Phase 6: Test circuit breaker reset
    println!("\n🔄 Phase 6: Testing circuit breaker reset...\n");

    demonstrate_circuit_breaker_status(&client, "Before reset").await?;

    println!("Resetting circuit breaker...");
    client.reset_circuit_breaker()?;

    demonstrate_circuit_breaker_status(&client, "After reset").await?;

    // Phase 7: Test health check with circuit breaker
    println!("\n🔄 Phase 7: Testing health check with circuit breaker...\n");

    println!("Performing health check with circuit breaker...");
    let start = std::time::Instant::now();
    let health_response = client.health_check_with_circuit_breaker().await?;
    let elapsed = start.elapsed();

    println!("   ⏱️  Time taken: {:?}", elapsed);
    println!("   📊 Health Status: {}", health_response.status);
    println!(
        "   📊 Response Time: {}ms",
        health_response.response_time_ms
    );

    if let Some(details) = &health_response.details {
        println!("   📊 Details: {}", details);
    }

    // Final status
    println!("\n🏁 Final Circuit Breaker Status:\n");
    demonstrate_circuit_breaker_status(&client, "Final").await?;

    println!("\n=== Circuit Breaker Demo Complete ===");
    println!("✅ Circuit breaker pattern successfully demonstrated!");
    println!("✅ Fallback behavior implemented and tested!");
    println!("✅ Health monitoring with circuit breaker integration working!");

    Ok(())
}

async fn demonstrate_circuit_breaker_status(client: &AIOracleClient, phase: &str) -> Result<()> {
    let status = client.get_circuit_breaker_status()?;

    println!("📊 Circuit Breaker Status ({}):", phase);
    println!("   State: {}", status["state"]);
    println!("   Success Count: {}", status["success_count"]);
    println!("   Failure Count: {}", status["failure_count"]);
    println!("   Total Requests: {}", status["total_requests"]);
    println!(
        "   Failure Rate: {:.2}%",
        status["failure_rate"].as_f64().unwrap_or(0.0) * 100.0
    );
    println!(
        "   Failure Threshold: {:.2}%",
        status["failure_threshold"].as_f64().unwrap_or(0.0) * 100.0
    );
    println!("   Recovery Time: {}s", status["recovery_time_seconds"]);

    if let Some(last_opened) = status["last_opened"].as_u64() {
        println!("   Last Opened: {}s ago", last_opened);
    }

    if let Some(last_closed) = status["last_closed"].as_u64() {
        println!("   Last Closed: {}s ago", last_closed);
    }

    println!();
    Ok(())
}
