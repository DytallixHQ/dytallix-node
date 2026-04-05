//! AI Oracle Client Module
//!
//! This module provides the HTTP client for communicating with external AI services
//! and handles all AI Oracle-related operations including health checks, service discovery,
//! and analysis requests.

use anyhow::{anyhow, Result};
use chrono;
use log::{info, warn};
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tokio::time::Duration;

use crate::consensus::types::AIServiceType;
use crate::consensus::SignedAIOracleResponse;

/// AI Analysis Result structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIAnalysisResult {
    pub service_type: AIServiceType,
    pub risk_score: f64,
    pub fraud_probability: f64,
    pub reputation_score: u32,
    pub compliance_flags: Vec<String>,
    pub recommendations: Vec<String>,
    pub metadata: HashMap<String, String>,
}

/// AI Service Information for discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIServiceInfo {
    pub service_id: String,
    pub service_type: AIServiceType,
    pub endpoint: String,
    pub version: String,
    pub capabilities: Vec<String>,
    pub availability_score: f64,
    pub last_heartbeat: u64,
}

/// AI Analysis Request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIAnalysisRequest {
    pub request_id: String,
    pub service_type: AIServiceType,
    pub data: HashMap<String, Value>,
    pub requester_id: String,
    pub timestamp: u64,
    pub priority: u8,
}

/// AI Service Configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIServiceConfig {
    /// Base URL for AI Oracle services
    pub base_url: String,
    /// Timeout for AI requests in seconds
    pub timeout_seconds: u64,
    /// API key for authentication
    pub api_key: String,
    /// Risk threshold for AI analysis confidence
    pub risk_threshold: f64,
    /// Maximum retries for failed requests
    pub max_retries: u32,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for AIServiceConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:8080".to_string(),
            timeout_seconds: 30,
            api_key: "default_api_key".to_string(),
            risk_threshold: 0.7,
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// HTTP client for communicating with external AI services
#[derive(Debug)]
pub struct AIOracleClient {
    client: Client,
    config: AIServiceConfig,
}

impl AIOracleClient {
    /// Create new AI Oracle client with configuration
    pub fn new(config: AIServiceConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    /// Generic POST request to AI service
    pub async fn post<P: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        endpoint: &str,
        payload: &P,
    ) -> Result<R> {
        let url = format!("{}/{}", self.config.base_url, endpoint);
        let mut attempts = 0;

        while attempts < self.config.max_retries {
            let response = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.config.api_key))
                .header("Content-Type", "application/json")
                .json(payload)
                .send()
                .await?;

            if response.status().is_success() {
                let result = response.json::<R>().await?;
                return Ok(result);
            } else if response.status().is_server_error() && attempts < self.config.max_retries - 1
            {
                attempts += 1;
                warn!("Server error on attempt {attempts}, retrying...");
                tokio::time::sleep(Duration::from_millis(self.config.retry_delay_ms)).await;
                continue;
            } else {
                return Err(anyhow!(
                    "HTTP error {}: {}",
                    response.status(),
                    response.text().await?
                ));
            }
        }

        Err(anyhow!("Max retries exceeded"))
    }

    /// Health check endpoint for AI services
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/health", self.config.base_url);
        let response = self.client.get(&url).send().await?;
        Ok(response.status().is_success())
    }

    /// Service discovery - get available AI services and their capabilities
    pub async fn discover_services(&self) -> Result<Vec<AIServiceInfo>> {
        let services: Vec<AIServiceInfo> = self.post("services", &serde_json::json!({})).await?;
        Ok(services)
    }

    /// Submit AI analysis request and get signed response
    pub async fn request_analysis(
        &self,
        request: &AIAnalysisRequest,
    ) -> Result<SignedAIOracleResponse> {
        // TODO: Implement actual HTTP request to AI service
        // For now, return a placeholder response

        let analysis_result = AIAnalysisResult {
            service_type: request.service_type.clone(),
            risk_score: 0.1,
            fraud_probability: 0.05,
            reputation_score: 85,
            compliance_flags: Vec::new(),
            recommendations: vec!["Transaction appears legitimate".to_string()],
            metadata: HashMap::new(),
        };

        let oracle_identity = crate::consensus::types::OracleIdentity::new(
            "mock_oracle".to_string(),
            "Mock Oracle".to_string(),
            Vec::new(), // mock public key
            dytallix_pqc::SignatureAlgorithm::Dilithium5,
        );

        let signature = crate::consensus::types::AIResponseSignature::new(
            dytallix_pqc::SignatureAlgorithm::Dilithium5,
            Vec::new(), // mock signature
            Vec::new(), // mock public key
        );

        let payload = crate::consensus::types::AIResponsePayload::success(
            request.request_id.clone(),
            request.service_type.clone(),
            serde_json::to_value(analysis_result)?,
        );

        Ok(SignedAIOracleResponse::new(
            payload,
            signature,
            chrono::Utc::now().timestamp_millis() as u64, // nonce
            (chrono::Utc::now().timestamp() + 3600) as u64, // expires_at (1 hour from now)
            oracle_identity,
        ))
    }

    /// Get current configuration
    pub fn get_config(&self) -> &AIServiceConfig {
        &self.config
    }

    /// Update timeout configuration
    pub fn set_timeout(&mut self, timeout_seconds: u64) {
        self.config.timeout_seconds = timeout_seconds;
    }

    /// Request AI analysis for a transaction or data with retry logic
    pub async fn request_ai_analysis(
        &self,
        service_type: AIServiceType,
        data: HashMap<String, Value>,
    ) -> Result<SignedAIOracleResponse> {
        let request = AIAnalysisRequest {
            request_id: format!("req_{}", chrono::Utc::now().timestamp_millis()),
            service_type,
            data,
            requester_id: "consensus_engine".to_string(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            priority: 5, // Medium priority
        };

        let response = self.request_analysis(&request).await?;

        // Validate response confidence score from metadata
        if let Some(metadata) = &response.response.metadata {
            if let Some(confidence) = metadata.confidence_score {
                if confidence < self.config.risk_threshold {
                    warn!("AI analysis confidence score below threshold: {confidence}");
                }
            }
        }

        info!(
            "AI analysis completed: service_type={:?}, response_id={}",
            response.response.service_type, response.response.id
        );

        Ok(response)
    }

    /// Batch request multiple AI analyses
    pub async fn batch_request_analyses(
        &self,
        requests: Vec<AIAnalysisRequest>,
    ) -> Result<Vec<SignedAIOracleResponse>> {
        let batch_payload = serde_json::json!({
            "requests": requests
        });

        let responses: Vec<SignedAIOracleResponse> =
            self.post("batch_analyze", &batch_payload).await?;
        Ok(responses)
    }

    /// Get service statistics from AI Oracle
    pub async fn get_service_stats(&self) -> Result<HashMap<String, Value>> {
        let stats: HashMap<String, Value> = self.post("stats", &serde_json::json!({})).await?;
        Ok(stats)
    }

    /// Check if specific AI service is available
    pub async fn is_service_available(&self, service_type: AIServiceType) -> Result<bool> {
        let services = self.discover_services().await?;
        Ok(services
            .iter()
            .any(|s| s.service_type == service_type && s.availability_score > 0.5))
    }

    /// Get the best available service for a specific type
    pub async fn get_best_service(
        &self,
        service_type: AIServiceType,
    ) -> Result<Option<AIServiceInfo>> {
        let services = self.discover_services().await?;
        let best_service = services
            .into_iter()
            .filter(|s| s.service_type == service_type)
            .max_by(|a, b| {
                a.availability_score
                    .partial_cmp(&b.availability_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

        Ok(best_service)
    }

    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.config.timeout_seconds)
    }
}

impl Default for AIOracleClient {
    fn default() -> Self {
        Self::new(AIServiceConfig::default())
    }
}
