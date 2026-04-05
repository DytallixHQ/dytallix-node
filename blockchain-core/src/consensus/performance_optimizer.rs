//! Performance Optimization and Fallback System
//!
//! This module implements performance optimizations for AI service integration including:
//! - AI request batching for multiple transactions
//! - Intelligent caching based on transaction patterns
//! - Fallback validation when AI service is unavailable
//! - Graceful degradation with reduced AI features
//! - Performance monitoring and optimization metrics

use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, Semaphore};
use uuid::Uuid;

use crate::consensus::ai_integration::{AIVerificationResult, RiskProcessingDecision};
use crate::types::{Transaction, TxHash};

/// Configuration for performance optimization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceConfig {
    /// Enable request batching
    pub enable_batching: bool,
    /// Maximum batch size for AI requests
    pub max_batch_size: usize,
    /// Batch timeout in milliseconds
    pub batch_timeout_ms: u64,
    /// Enable intelligent caching
    pub enable_caching: bool,
    /// Maximum cache size (number of entries)
    pub max_cache_size: usize,
    /// Cache TTL in seconds
    pub cache_ttl_seconds: u64,
    /// Enable fallback validation
    pub enable_fallback: bool,
    /// Fallback timeout threshold in seconds
    pub fallback_timeout_threshold: u64,
    /// Enable graceful degradation
    pub enable_degradation: bool,
    /// Maximum concurrent AI requests
    pub max_concurrent_requests: usize,
    /// Performance monitoring interval in seconds
    pub monitoring_interval_seconds: u64,
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enable_batching: true,
            max_batch_size: 10,
            batch_timeout_ms: 100,
            enable_caching: true,
            max_cache_size: 1000,
            cache_ttl_seconds: 300, // 5 minutes
            enable_fallback: true,
            fallback_timeout_threshold: 5,
            enable_degradation: true,
            max_concurrent_requests: 50,
            monitoring_interval_seconds: 60,
        }
    }
}

/// Batch request containing multiple transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchRequest {
    pub batch_id: Uuid,
    pub transactions: Vec<(TxHash, Transaction)>,
    pub created_at: DateTime<Utc>,
    pub priority: BatchPriority,
}

/// Priority levels for batch processing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum BatchPriority {
    Low,
    Normal,
    High,
    Critical,
}

/// Cached AI response entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    pub transaction_hash: TxHash,
    pub result: AIVerificationResult,
    pub cached_at: DateTime<Utc>,
    pub access_count: u64,
    pub last_accessed: DateTime<Utc>,
    pub cache_key: String,
}

/// Fallback validation mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FallbackMode {
    /// Use basic validation only
    BasicOnly,
    /// Use pattern-based risk assessment
    PatternBased,
    /// Use historical data for risk scoring
    HistoricalBased,
    /// Conservative approach - flag all as high risk
    Conservative,
}

/// Performance metrics tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Total requests processed
    pub total_requests: u64,
    /// Requests served from cache
    pub cache_hits: u64,
    /// Cache miss count
    pub cache_misses: u64,
    /// Total batched requests
    pub batched_requests: u64,
    /// Average batch size
    pub average_batch_size: f64,
    /// Average response time in milliseconds
    pub average_response_time_ms: f64,
    /// Fallback activations
    pub fallback_activations: u64,
    /// Degradation mode activations
    pub degradation_activations: u64,
    /// AI service timeout count
    pub timeout_count: u64,
    /// AI service error count
    pub error_count: u64,
    /// Current concurrent requests
    pub current_concurrent_requests: u64,
    /// Peak concurrent requests
    pub peak_concurrent_requests: u64,
    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            total_requests: 0,
            cache_hits: 0,
            cache_misses: 0,
            batched_requests: 0,
            average_batch_size: 0.0,
            average_response_time_ms: 0.0,
            fallback_activations: 0,
            degradation_activations: 0,
            timeout_count: 0,
            error_count: 0,
            current_concurrent_requests: 0,
            peak_concurrent_requests: 0,
            last_updated: Utc::now(),
        }
    }
}

/// Main performance optimizer
#[derive(Debug)]
pub struct PerformanceOptimizer {
    pub config: PerformanceConfig,
    /// Pending batch requests
    pending_batches: Arc<RwLock<VecDeque<BatchRequest>>>,
    /// Response cache
    cache: Arc<RwLock<HashMap<String, CacheEntry>>>,
    /// Performance metrics
    metrics: Arc<RwLock<PerformanceMetrics>>,
    /// Concurrent request limiter
    request_semaphore: Arc<Semaphore>,
    /// Current fallback mode
    fallback_mode: Arc<RwLock<Option<FallbackMode>>>,
    /// Service health status
    service_healthy: Arc<RwLock<bool>>,
    /// Response time history for averaging
    response_times: Arc<Mutex<VecDeque<u64>>>,
}

impl PerformanceOptimizer {
    /// Create a new performance optimizer
    pub fn new(config: PerformanceConfig) -> Self {
        let request_semaphore = Arc::new(Semaphore::new(config.max_concurrent_requests));

        Self {
            config,
            pending_batches: Arc::new(RwLock::new(VecDeque::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
            metrics: Arc::new(RwLock::new(PerformanceMetrics::default())),
            request_semaphore,
            fallback_mode: Arc::new(RwLock::new(None)),
            service_healthy: Arc::new(RwLock::new(true)),
            response_times: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    /// Check cache for existing result
    pub async fn get_cached_result(
        &self,
        transaction_hash: &TxHash,
    ) -> Option<AIVerificationResult> {
        if !self.config.enable_caching {
            return None;
        }

        let cache_key = self.generate_cache_key(transaction_hash);
        let mut cache = self.cache.write().await;

        if let Some(entry) = cache.get_mut(&cache_key) {
            // Check if cache entry is still valid
            let now = Utc::now();
            let age = (now - entry.cached_at).num_seconds() as u64;

            if age <= self.config.cache_ttl_seconds {
                // Update access statistics
                entry.access_count += 1;
                entry.last_accessed = now;

                // Update metrics
                let mut metrics = self.metrics.write().await;
                metrics.cache_hits += 1;

                debug!(
                    "Cache hit for transaction {}",
                    hex::encode(transaction_hash)
                );
                return Some(entry.result.clone());
            } else {
                // Remove expired entry
                cache.remove(&cache_key);
                debug!(
                    "Cache entry expired for transaction {}",
                    hex::encode(transaction_hash)
                );
            }
        }

        // Update cache miss metrics
        let mut metrics = self.metrics.write().await;
        metrics.cache_misses += 1;

        None
    }

    /// Cache an AI verification result
    pub async fn cache_result(
        &self,
        transaction_hash: &TxHash,
        result: &AIVerificationResult,
    ) -> Result<()> {
        if !self.config.enable_caching {
            return Ok(());
        }

        let cache_key = self.generate_cache_key(transaction_hash);
        let entry = CacheEntry {
            transaction_hash: transaction_hash.clone(),
            result: result.clone(),
            cached_at: Utc::now(),
            access_count: 0,
            last_accessed: Utc::now(),
            cache_key: cache_key.clone(),
        };

        let mut cache = self.cache.write().await;

        // Check cache size limit
        if cache.len() >= self.config.max_cache_size {
            self.evict_cache_entries(&mut cache).await;
        }

        cache.insert(cache_key, entry);
        debug!(
            "Cached result for transaction {}",
            hex::encode(transaction_hash)
        );

        Ok(())
    }

    /// Add transaction to batch queue
    pub async fn add_to_batch(
        &self,
        transaction_hash: TxHash,
        transaction: Transaction,
    ) -> Result<Uuid> {
        if !self.config.enable_batching {
            return Err(anyhow!("Batching is disabled"));
        }

        let batch_id = Uuid::new_v4();
        let priority = self.determine_batch_priority(&transaction);

        let batch = BatchRequest {
            batch_id,
            transactions: vec![(transaction_hash, transaction)],
            created_at: Utc::now(),
            priority: priority.clone(),
        };

        let mut batches = self.pending_batches.write().await;

        // Try to merge with existing batch of same priority
        let mut merged = false;
        for existing_batch in batches.iter_mut() {
            if existing_batch.priority == priority
                && existing_batch.transactions.len() < self.config.max_batch_size
            {
                existing_batch
                    .transactions
                    .extend(batch.transactions.clone());
                merged = true;
                break;
            }
        }

        if !merged {
            // Insert in priority order
            let insert_pos = batches
                .iter()
                .position(|b| b.priority < priority)
                .unwrap_or(batches.len());
            batches.insert(insert_pos, batch);
        }

        debug!("Added transaction to batch queue (merged: {merged})");
        Ok(batch_id)
    }

    /// Get next batch for processing
    pub async fn get_next_batch(&self) -> Option<BatchRequest> {
        if !self.config.enable_batching {
            return None;
        }

        let mut batches = self.pending_batches.write().await;

        // Check for ready batches (either full or timed out)
        let now = Utc::now();
        let timeout = Duration::milliseconds(self.config.batch_timeout_ms as i64);

        for (index, batch) in batches.iter().enumerate() {
            let is_full = batch.transactions.len() >= self.config.max_batch_size;
            let is_timed_out = (now - batch.created_at) >= timeout;

            if is_full || is_timed_out {
                let batch = batches.remove(index).unwrap();

                // Update metrics
                let mut metrics = self.metrics.write().await;
                metrics.batched_requests += batch.transactions.len() as u64;

                debug!(
                    "Processing batch {} with {} transactions (full: {}, timeout: {})",
                    batch.batch_id,
                    batch.transactions.len(),
                    is_full,
                    is_timed_out
                );

                return Some(batch);
            }
        }

        None
    }

    /// Activate fallback mode
    pub async fn activate_fallback(&self, mode: FallbackMode) -> Result<()> {
        if !self.config.enable_fallback {
            return Err(anyhow!("Fallback mode is disabled"));
        }

        {
            let mut fallback = self.fallback_mode.write().await;
            *fallback = Some(mode.clone());
        }

        {
            let mut service_healthy = self.service_healthy.write().await;
            *service_healthy = false;
        }

        {
            let mut metrics = self.metrics.write().await;
            metrics.fallback_activations += 1;
        }

        warn!("Activated fallback mode: {mode:?}");
        Ok(())
    }

    /// Deactivate fallback mode
    pub async fn deactivate_fallback(&self) -> Result<()> {
        {
            let mut fallback = self.fallback_mode.write().await;
            *fallback = None;
        }

        {
            let mut service_healthy = self.service_healthy.write().await;
            *service_healthy = true;
        }

        info!("Deactivated fallback mode - AI service restored");
        Ok(())
    }

    /// Perform fallback validation
    pub async fn fallback_validation(
        &self,
        transaction_hash: &TxHash,
        transaction: &Transaction,
    ) -> Result<AIVerificationResult> {
        let fallback_mode = {
            let fallback = self.fallback_mode.read().await;
            fallback.clone()
        };

        let mode = fallback_mode.unwrap_or(FallbackMode::BasicOnly);

        match mode {
            FallbackMode::BasicOnly => {
                // Minimal validation - assume low risk
                Ok(AIVerificationResult::Verified {
                    oracle_id: "fallback_basic".to_string(),
                    response_id: Uuid::new_v4().to_string(),
                    risk_score: Some(0.1),
                    confidence: Some(0.5),
                    processing_decision: RiskProcessingDecision::AutoApprove,
                    fraud_probability: Some(0.1),
                })
            }
            FallbackMode::PatternBased => {
                // Pattern-based risk assessment
                let risk_score = self.assess_pattern_risk(transaction).await;
                let decision = if risk_score > 0.8 {
                    RiskProcessingDecision::RequireReview {
                        reason: "High pattern-based risk score".to_string(),
                    }
                } else {
                    RiskProcessingDecision::AutoApprove
                };

                Ok(AIVerificationResult::Verified {
                    oracle_id: "fallback_pattern".to_string(),
                    response_id: Uuid::new_v4().to_string(),
                    risk_score: Some(risk_score),
                    confidence: Some(0.7),
                    processing_decision: decision,
                    fraud_probability: Some(risk_score * 0.8),
                })
            }
            FallbackMode::HistoricalBased => {
                // Historical data-based assessment
                let risk_score = self
                    .assess_historical_risk(transaction_hash, transaction)
                    .await;
                let decision = if risk_score > 0.7 {
                    RiskProcessingDecision::RequireReview {
                        reason: "High historical risk indicator".to_string(),
                    }
                } else {
                    RiskProcessingDecision::AutoApprove
                };

                Ok(AIVerificationResult::Verified {
                    oracle_id: "fallback_historical".to_string(),
                    response_id: Uuid::new_v4().to_string(),
                    risk_score: Some(risk_score),
                    confidence: Some(0.6),
                    processing_decision: decision,
                    fraud_probability: Some(risk_score * 0.9),
                })
            }
            FallbackMode::Conservative => {
                // Conservative approach - flag everything for review
                Ok(AIVerificationResult::Verified {
                    oracle_id: "fallback_conservative".to_string(),
                    response_id: Uuid::new_v4().to_string(),
                    risk_score: Some(0.8),
                    confidence: Some(0.9),
                    processing_decision: RiskProcessingDecision::RequireReview {
                        reason: "Conservative fallback mode - manual review required".to_string(),
                    },
                    fraud_probability: Some(0.7),
                })
            }
        }
    }

    /// Check if service should use graceful degradation
    pub async fn should_degrade(&self) -> bool {
        if !self.config.enable_degradation {
            return false;
        }

        let metrics = self.metrics.read().await;
        let error_rate = if metrics.total_requests > 0 {
            (metrics.error_count + metrics.timeout_count) as f64 / metrics.total_requests as f64
        } else {
            0.0
        };

        // Degrade if error rate is high or too many concurrent requests
        error_rate > 0.1
            || metrics.current_concurrent_requests
                > (self.config.max_concurrent_requests as u64 * 8 / 10)
    }

    /// Record request metrics
    pub async fn record_request_metrics(&self, response_time_ms: u64, success: bool) {
        // Update response times
        {
            let mut times = self.response_times.lock().await;
            times.push_back(response_time_ms);
            if times.len() > 100 {
                times.pop_front();
            }
        }

        // Update metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.total_requests += 1;

            if !success {
                metrics.error_count += 1;
            }

            // Calculate average response time
            let times = self.response_times.lock().await;
            if !times.is_empty() {
                metrics.average_response_time_ms =
                    times.iter().sum::<u64>() as f64 / times.len() as f64;
            }

            metrics.last_updated = Utc::now();
        }
    }

    /// Record timeout
    pub async fn record_timeout(&self) {
        let mut metrics = self.metrics.write().await;
        metrics.timeout_count += 1;
        metrics.total_requests += 1;
    }

    /// Get current performance metrics
    pub async fn get_metrics(&self) -> PerformanceMetrics {
        let metrics = self.metrics.read().await;
        metrics.clone()
    }

    /// Cleanup expired cache entries and optimize
    pub async fn cleanup_cache(&self) -> Result<usize> {
        let mut cache = self.cache.write().await;
        let now = Utc::now();
        let mut removed_count = 0;

        cache.retain(|_, entry| {
            let age = (now - entry.cached_at).num_seconds() as u64;
            if age > self.config.cache_ttl_seconds {
                removed_count += 1;
                false
            } else {
                true
            }
        });

        if removed_count > 0 {
            info!("Cleaned up {removed_count} expired cache entries");
        }

        Ok(removed_count)
    }

    /// Acquire request permit (for concurrency limiting)
    pub async fn acquire_request_permit(&self) -> Result<tokio::sync::SemaphorePermit> {
        let permit = self
            .request_semaphore
            .acquire()
            .await
            .map_err(|e| anyhow!("Failed to acquire request permit: {}", e))?;

        // Update concurrent request metrics
        {
            let mut metrics = self.metrics.write().await;
            metrics.current_concurrent_requests += 1;
            if metrics.current_concurrent_requests > metrics.peak_concurrent_requests {
                metrics.peak_concurrent_requests = metrics.current_concurrent_requests;
            }
        }

        Ok(permit)
    }

    /// Release request permit
    pub async fn release_request_permit(&self) {
        let mut metrics = self.metrics.write().await;
        if metrics.current_concurrent_requests > 0 {
            metrics.current_concurrent_requests -= 1;
        }
    }

    /// Check if AI service is healthy
    pub async fn is_service_healthy(&self) -> bool {
        let healthy = self.service_healthy.read().await;
        *healthy
    }

    /// Generate cache key for transaction
    fn generate_cache_key(&self, transaction_hash: &TxHash) -> String {
        // For now, use transaction hash as cache key
        // In production, might want to include transaction content hash
        hex::encode(transaction_hash)
    }

    /// Determine batch priority based on transaction
    fn determine_batch_priority(&self, transaction: &Transaction) -> BatchPriority {
        match transaction {
            Transaction::Transfer(tx) => {
                if tx.amount > 1000000 {
                    BatchPriority::High
                } else if tx.amount > 100000 {
                    BatchPriority::Normal
                } else {
                    BatchPriority::Low
                }
            }
            Transaction::Deploy(_) => BatchPriority::High,
            Transaction::Call(_) => BatchPriority::Normal,
            Transaction::Stake(tx) => {
                if tx.amount > 500000 {
                    BatchPriority::Critical
                } else {
                    BatchPriority::High
                }
            }
            Transaction::AIRequest(_) => BatchPriority::Normal,
        }
    }

    /// Evict least recently used cache entries
    async fn evict_cache_entries(&self, cache: &mut HashMap<String, CacheEntry>) {
        let evict_count = cache.len() / 4; // Evict 25% of entries

        // Sort by last accessed time and remove oldest
        let mut entries: Vec<_> = cache
            .iter()
            .map(|(k, v)| (k.clone(), v.last_accessed))
            .collect();
        entries.sort_by_key(|(_, last_accessed)| *last_accessed);

        for (key, _) in entries.iter().take(evict_count) {
            cache.remove(key);
        }

        debug!("Evicted {evict_count} cache entries");
    }

    /// Assess risk based on transaction patterns
    async fn assess_pattern_risk(&self, transaction: &Transaction) -> f64 {
        // Simplified pattern-based risk assessment
        match transaction {
            Transaction::Transfer(tx) => {
                let mut risk: f64 = 0.0;

                // High amount transactions are riskier
                if tx.amount > 1000000 {
                    risk += 0.3;
                } else if tx.amount > 100000 {
                    risk += 0.1;
                }

                // TODO: Add more sophisticated pattern analysis
                // - Time-based patterns
                // - Address reputation
                // - Transaction frequency

                risk.min(1.0)
            }
            Transaction::Deploy(_) => 0.4, // Contract deployments have moderate risk
            Transaction::Call(_) => 0.2,   // Contract calls have low-moderate risk
            Transaction::Stake(_) => 0.1,  // Staking has low risk
            Transaction::AIRequest(_) => 0.05, // AI requests have very low risk
        }
    }

    /// Assess risk based on historical data
    async fn assess_historical_risk(
        &self,
        _transaction_hash: &TxHash,
        transaction: &Transaction,
    ) -> f64 {
        // Simplified historical risk assessment
        // In production, this would query historical transaction data
        match transaction {
            Transaction::Transfer(tx) => {
                if tx.amount > 500000 {
                    0.6 // Historical data shows large transfers have higher risk
                } else {
                    0.2
                }
            }
            _ => 0.3, // Default moderate risk for other transaction types
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{PQCTransactionSignature, TransferTransaction};
    use dytallix_pqc::{Signature, SignatureAlgorithm};

    fn create_test_transaction(amount: u64) -> Transaction {
        Transaction::Transfer(TransferTransaction {
            hash: "test_tx".to_string(),
            from: "sender".to_string(),
            to: "recipient".to_string(),
            amount: amount as u128,
            fee: 10,
            nonce: 1,
            timestamp: Utc::now().timestamp() as u64,
            signature: PQCTransactionSignature {
                signature: Signature {
                    data: vec![],
                    algorithm: SignatureAlgorithm::Dilithium5,
                },
                public_key: vec![],
            },
            ai_risk_score: None,
        })
    }

    #[tokio::test]
    async fn test_cache_operations() {
        let config = PerformanceConfig::default();
        let optimizer = PerformanceOptimizer::new(config);

        let tx_hash = "test_hash".to_string();

        // Should return None for uncached result
        let result = optimizer.get_cached_result(&tx_hash).await;
        assert!(result.is_none());

        // Cache a result
        let ai_result = AIVerificationResult::Verified {
            oracle_id: "test".to_string(),
            response_id: "test".to_string(),
            risk_score: Some(0.5),
            confidence: Some(0.9),
            processing_decision: RiskProcessingDecision::AutoApprove,
            fraud_probability: Some(0.3),
        };

        optimizer.cache_result(&tx_hash, &ai_result).await.unwrap();

        // Should now return cached result
        let cached = optimizer.get_cached_result(&tx_hash).await;
        assert!(cached.is_some());
    }

    #[tokio::test]
    async fn test_batch_processing() {
        let config = PerformanceConfig::default();
        let optimizer = PerformanceOptimizer::new(config);

        let tx1 = create_test_transaction(1000);
        let tx2 = create_test_transaction(2000);

        // Add transactions to batch
        optimizer
            .add_to_batch("tx1".to_string(), tx1)
            .await
            .unwrap();
        optimizer
            .add_to_batch("tx2".to_string(), tx2)
            .await
            .unwrap();

        // Should have pending batch
        let batch = optimizer.get_next_batch().await;
        assert!(batch.is_some());

        let batch = batch.unwrap();
        assert_eq!(batch.transactions.len(), 2);
    }

    #[tokio::test]
    async fn test_fallback_modes() {
        let config = PerformanceConfig::default();
        let optimizer = PerformanceOptimizer::new(config);

        let tx = create_test_transaction(5000);
        let tx_hash = "test_tx".to_string();

        // Test different fallback modes
        for mode in [
            FallbackMode::BasicOnly,
            FallbackMode::PatternBased,
            FallbackMode::HistoricalBased,
            FallbackMode::Conservative,
        ] {
            optimizer.activate_fallback(mode).await.unwrap();

            let result = optimizer.fallback_validation(&tx_hash, &tx).await;
            assert!(result.is_ok());

            optimizer.deactivate_fallback().await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_performance_metrics() {
        let config = PerformanceConfig::default();
        let optimizer = PerformanceOptimizer::new(config);

        // Record some metrics
        optimizer.record_request_metrics(100, true).await;
        optimizer.record_request_metrics(200, false).await;
        optimizer.record_timeout().await;

        let metrics = optimizer.get_metrics().await;
        assert_eq!(metrics.total_requests, 3);
        assert_eq!(metrics.error_count, 1);
        assert_eq!(metrics.timeout_count, 1);
        assert_eq!(metrics.average_response_time_ms, 150.0);
    }

    #[tokio::test]
    async fn test_concurrency_limiting() {
        let mut config = PerformanceConfig::default();
        config.max_concurrent_requests = 2;
        let optimizer = PerformanceOptimizer::new(config);

        // Acquire permits
        let permit1 = optimizer.acquire_request_permit().await.unwrap();
        let permit2 = optimizer.acquire_request_permit().await.unwrap();

        let metrics = optimizer.get_metrics().await;
        assert_eq!(metrics.current_concurrent_requests, 2);

        // Release permits
        drop(permit1);
        optimizer.release_request_permit().await;
        drop(permit2);
        optimizer.release_request_permit().await;

        let metrics = optimizer.get_metrics().await;
        assert_eq!(metrics.current_concurrent_requests, 0);
    }
}
