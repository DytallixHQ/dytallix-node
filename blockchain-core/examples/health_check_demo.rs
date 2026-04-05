// Example demonstrating health check functionality

use dytallix_node::consensus::{AIOracleClient, AIServiceConfig, AIServiceStatus};
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("=== Dytallix AI Service Health Check Demo ===\n");

    // Test with httpbin.org which has various testing endpoints
    let client = AIOracleClient::new(AIServiceConfig {
        base_url: "https://httpbin.org".to_string(),
        ..AIServiceConfig::default()
    });

    println!("1. Testing basic connectivity...");
    let connectivity = client.health_check().await?;
    println!("   Basic connectivity: {}\n", connectivity);

    println!("2. Performing detailed health check...");
    let health = client.health_check().await?;
    println!("   Status: {}", health.status);
    println!("   Response time: {}ms", health.response_time_ms);
    println!("   Timestamp: {}", health.timestamp);

    if let Some(ref version) = health.version {
        println!("   Version: {}", version);
    }

    if let Some(ref endpoints) = health.endpoints {
        println!("   Available endpoints: {:?}", endpoints);
    }

    if let Some(ref load) = health.load {
        println!("   Service load:");
        if let Some(cpu) = load.cpu_usage {
            println!("     CPU: {:.1}%", cpu);
        }
        if let Some(memory) = load.memory_usage {
            println!("     Memory: {:.1}%", memory);
        }
        if let Some(queue) = load.queue_size {
            println!("     Queue size: {}", queue);
        }
        if let Some(rps) = load.requests_per_second {
            println!("     RPS: {:.1}", rps);
        }
    }

    if let Some(ref details) = health.details {
        println!("   Details: {}", details);
    }

    println!("\n3. Testing health check with custom timeout...");
    let health_timeout = client.health_check().await?; // simplified
    println!("   Status with 1s timeout: {}", health_timeout.status);
    println!("   Response time: {}ms", health_timeout.response_time_ms);

    println!("\n4. Starting background health monitoring for 10 seconds...");
    let monitor_handle = client.start_background_health_monitoring(3); // Check every 3 seconds

    // Let it run for 10 seconds
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Stop monitoring
    monitor_handle.abort();
    println!("   Background monitoring stopped.\n");

    println!("=== Health Check Demo Complete ===");

    Ok(())
}
