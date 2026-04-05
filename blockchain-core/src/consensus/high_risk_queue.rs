//! High-Risk Transaction Queue System
//!
//! This module implements a queuing system for transactions flagged as high-risk
//! by the AI analysis system. It provides manual review workflow capabilities,
//! notification systems, and bulk approval/rejection functionality.

use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

use crate::consensus::ai_integration::{AIVerificationResult, RiskProcessingDecision};
use crate::consensus::notification_system::{NotificationSystem, NotificationSystemConfig};
use crate::consensus::notification_types::NotificationType;
use crate::types::{Transaction, TxHash};

/// Status of a transaction in the high-risk queue
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReviewStatus {
    /// Transaction is pending manual review
    Pending,
    /// Transaction is currently being reviewed by an officer
    InReview {
        officer_id: String,
        started_at: DateTime<Utc>,
    },
    /// Transaction has been approved for processing
    Approved {
        officer_id: String,
        approved_at: DateTime<Utc>,
        notes: Option<String>,
    },
    /// Transaction has been rejected
    Rejected {
        officer_id: String,
        rejected_at: DateTime<Utc>,
        reason: String,
    },
    /// Transaction was auto-expired due to timeout
    Expired { expired_at: DateTime<Utc> },
}

/// Priority level for high-risk transactions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReviewPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Information about a transaction queued for review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedTransaction {
    pub queue_id: Uuid,
    pub transaction: Transaction,
    pub transaction_hash: TxHash,
    pub ai_result: AIVerificationResult,
    pub risk_decision: RiskProcessingDecision,
    pub priority: ReviewPriority,
    pub status: ReviewStatus,
    pub queued_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
    pub tags: Vec<String>,
    pub compliance_notes: Option<String>,
}

/// Statistics about the review queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStatistics {
    pub total_pending: usize,
    pub total_in_review: usize,
    pub total_approved_today: usize,
    pub total_rejected_today: usize,
    pub average_review_time_minutes: f64,
    pub oldest_pending_age_hours: f64,
    pub priority_breakdown: HashMap<ReviewPriority, usize>,
}

/// Configuration for the high-risk queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HighRiskQueueConfig {
    /// Maximum number of transactions in queue before rejecting new ones
    pub max_queue_size: usize,
    /// Maximum time a transaction can stay in queue before auto-expiring (hours)
    pub max_queue_time_hours: u64,
    /// Maximum time a transaction can be "in review" before timing out (hours)
    pub max_review_time_hours: u64,
    /// Enable email notifications to compliance officers
    pub enable_notifications: bool,
    /// Enable automatic prioritization based on risk scores
    pub enable_auto_prioritization: bool,
    /// Minimum risk score to escalate to high priority
    pub high_priority_threshold: f64,
    /// Minimum risk score to escalate to critical priority
    pub critical_priority_threshold: f64,
}

impl Default for HighRiskQueueConfig {
    fn default() -> Self {
        Self {
            max_queue_size: 1000,
            max_queue_time_hours: 72,  // 3 days
            max_review_time_hours: 24, // 1 day
            enable_notifications: true,
            enable_auto_prioritization: true,
            high_priority_threshold: 0.8,
            critical_priority_threshold: 0.9,
        }
    }
}

/// High-risk transaction queue manager
#[derive(Debug)]
pub struct HighRiskQueue {
    config: HighRiskQueueConfig,
    /// Queue of transactions pending review, ordered by priority and age
    pending_queue: Arc<RwLock<VecDeque<QueuedTransaction>>>,
    /// Map of all queued transactions by ID for fast lookup
    transactions: Arc<RwLock<HashMap<Uuid, QueuedTransaction>>>,
    /// Map of transactions by hash for deduplication
    hash_to_queue_id: Arc<RwLock<HashMap<TxHash, Uuid>>>,
    /// Notification system (placeholder for now)
    notification_queue: Arc<Mutex<VecDeque<NotificationType>>>,
    /// Statistics tracking
    stats: Arc<RwLock<QueueStatistics>>,
    /// Notification system integration
    notification_system: Option<Arc<NotificationSystem>>,
}

impl HighRiskQueue {
    /// Create a new high-risk transaction queue
    pub fn new(config: HighRiskQueueConfig) -> Self {
        let notification_system = NotificationSystem::new(NotificationSystemConfig {
            enable_email: true,  // Enable email notifications by default
            enable_in_app: true, // Enable in-app notifications
            ..Default::default()
        });

        Self {
            config,
            pending_queue: Arc::new(RwLock::new(VecDeque::new())),
            transactions: Arc::new(RwLock::new(HashMap::new())),
            hash_to_queue_id: Arc::new(RwLock::new(HashMap::new())),
            notification_queue: Arc::new(Mutex::new(VecDeque::new())),
            stats: Arc::new(RwLock::new(QueueStatistics {
                total_pending: 0,
                total_in_review: 0,
                total_approved_today: 0,
                total_rejected_today: 0,
                average_review_time_minutes: 0.0,
                oldest_pending_age_hours: 0.0,
                priority_breakdown: HashMap::new(),
            })),
            notification_system: Some(Arc::new(notification_system)),
        }
    }

    /// Add a transaction to the high-risk queue
    pub async fn enqueue_transaction(
        &self,
        transaction: Transaction,
        transaction_hash: TxHash,
        ai_result: AIVerificationResult,
        risk_decision: RiskProcessingDecision,
    ) -> Result<Uuid> {
        // Check if we already have this transaction
        {
            let hash_map = self.hash_to_queue_id.read().await;
            if hash_map.contains_key(&transaction_hash) {
                return Err(anyhow!(
                    "Transaction already in queue: {}",
                    hex::encode(&transaction_hash)
                ));
            }
        }

        // Check queue capacity
        {
            let pending = self.pending_queue.read().await;
            if pending.len() >= self.config.max_queue_size {
                self.send_notification(NotificationType::QueueCapacityWarning {
                    current_size: pending.len(),
                    max_size: self.config.max_queue_size,
                    warning_level: 3, // Medium warning level
                })
                .await;
                return Err(anyhow!(
                    "Queue is at capacity: {}/{}",
                    pending.len(),
                    self.config.max_queue_size
                ));
            }
        }

        let queue_id = Uuid::new_v4();
        let now = Utc::now();

        // Determine priority based on AI result
        let priority = self.calculate_priority(&ai_result);

        // Create tags based on the risk decision and AI result
        let tags = self.generate_tags(&risk_decision, &ai_result);

        let queued_transaction = QueuedTransaction {
            queue_id,
            transaction,
            transaction_hash: transaction_hash.clone(),
            ai_result: ai_result.clone(),
            risk_decision: risk_decision.clone(),
            priority: priority.clone(),
            status: ReviewStatus::Pending,
            queued_at: now,
            last_updated: now,
            tags,
            compliance_notes: None,
        };

        // Add to all tracking structures
        {
            let mut pending = self.pending_queue.write().await;
            let mut transactions = self.transactions.write().await;
            let mut hash_map = self.hash_to_queue_id.write().await;

            // Insert in priority order (highest priority first)
            let insert_position = pending
                .iter()
                .position(|t| t.priority < priority)
                .unwrap_or(pending.len());

            pending.insert(insert_position, queued_transaction.clone());
            transactions.insert(queue_id, queued_transaction);
            hash_map.insert(transaction_hash.clone(), queue_id);
        }

        // Update statistics
        let _ = self.update_stats().await;

        // Send notification for high/critical priority transactions
        if matches!(priority, ReviewPriority::High | ReviewPriority::Critical) {
            let risk_score = match &ai_result {
                AIVerificationResult::Verified {
                    risk_score: Some(score),
                    ..
                } => *score,
                AIVerificationResult::Verified {
                    fraud_probability: Some(prob),
                    ..
                } => *prob,
                _ => 0.5, // Default risk score if not available
            };

            self.send_notification(NotificationType::NewHighRiskTransaction {
                queue_id,
                transaction_hash: hex::encode(&transaction_hash),
                risk_score,
                priority: priority.clone(),
            })
            .await;
        }

        info!(
            "Transaction {} queued for review with priority {:?} (queue ID: {})",
            hex::encode(&transaction_hash),
            priority,
            queue_id
        );

        Ok(queue_id)
    }

    /// Get the next transaction for review
    pub async fn get_next_for_review(&self) -> Option<QueuedTransaction> {
        let mut pending = self.pending_queue.write().await;
        pending.pop_front()
    }

    /// Start reviewing a transaction
    pub async fn start_review(&self, queue_id: Uuid, officer_id: String) -> Result<()> {
        let mut transactions = self.transactions.write().await;

        let transaction = transactions
            .get_mut(&queue_id)
            .ok_or_else(|| anyhow!("Transaction not found in queue: {}", queue_id))?;

        if !matches!(transaction.status, ReviewStatus::Pending) {
            return Err(anyhow!(
                "Transaction is not in pending status: {:?}",
                transaction.status
            ));
        }

        transaction.status = ReviewStatus::InReview {
            officer_id: officer_id.clone(),
            started_at: Utc::now(),
        };
        transaction.last_updated = Utc::now();

        info!("Officer {officer_id} started reviewing transaction {queue_id}");

        let _ = self.update_stats().await;
        Ok(())
    }

    /// Approve a transaction
    pub async fn approve_transaction(
        &self,
        queue_id: Uuid,
        officer_id: String,
        notes: Option<String>,
    ) -> Result<QueuedTransaction> {
        let mut transactions = self.transactions.write().await;
        let mut hash_map = self.hash_to_queue_id.write().await;

        let transaction = transactions
            .get_mut(&queue_id)
            .ok_or_else(|| anyhow!("Transaction not found in queue: {}", queue_id))?;

        transaction.status = ReviewStatus::Approved {
            officer_id: officer_id.clone(),
            approved_at: Utc::now(),
            notes,
        };
        transaction.last_updated = Utc::now();

        let approved_transaction = transaction.clone();

        // Remove from tracking (approved transactions are processed)
        hash_map.remove(&transaction.transaction_hash);

        info!("Officer {officer_id} approved transaction {queue_id}");

        let _ = self.update_stats().await;
        Ok(approved_transaction)
    }

    /// Reject a transaction
    pub async fn reject_transaction(
        &self,
        queue_id: Uuid,
        officer_id: String,
        reason: String,
    ) -> Result<()> {
        let mut transactions = self.transactions.write().await;
        let mut hash_map = self.hash_to_queue_id.write().await;

        let transaction = transactions
            .get_mut(&queue_id)
            .ok_or_else(|| anyhow!("Transaction not found in queue: {}", queue_id))?;

        transaction.status = ReviewStatus::Rejected {
            officer_id: officer_id.clone(),
            rejected_at: Utc::now(),
            reason,
        };
        transaction.last_updated = Utc::now();

        // Remove from tracking (rejected transactions are discarded)
        hash_map.remove(&transaction.transaction_hash);

        info!("Officer {officer_id} rejected transaction {queue_id}");

        let _ = self.update_stats().await;
        Ok(())
    }

    /// Get all pending transactions
    pub async fn get_pending_transactions(&self) -> Vec<QueuedTransaction> {
        let pending = self.pending_queue.read().await;
        pending.iter().cloned().collect()
    }

    /// Get transaction by queue ID
    pub async fn get_transaction(&self, queue_id: Uuid) -> Option<QueuedTransaction> {
        let transactions = self.transactions.read().await;
        transactions.get(&queue_id).cloned()
    }

    /// Get queue statistics
    pub async fn get_statistics(&self) -> QueueStatistics {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Bulk approve transactions
    pub async fn bulk_approve(
        &self,
        queue_ids: Vec<Uuid>,
        officer_id: String,
    ) -> Result<Vec<QueuedTransaction>> {
        let mut approved = Vec::new();

        for queue_id in queue_ids {
            match self
                .approve_transaction(queue_id, officer_id.clone(), None)
                .await
            {
                Ok(transaction) => approved.push(transaction),
                Err(e) => warn!("Failed to approve transaction {queue_id}: {e}"),
            }
        }

        info!(
            "Officer {} bulk approved {} transactions",
            officer_id,
            approved.len()
        );
        Ok(approved)
    }

    /// Bulk reject transactions
    pub async fn bulk_reject(
        &self,
        queue_ids: Vec<Uuid>,
        officer_id: String,
        reason: String,
    ) -> Result<usize> {
        let mut rejected_count = 0;

        for queue_id in queue_ids {
            match self
                .reject_transaction(queue_id, officer_id.clone(), reason.clone())
                .await
            {
                Ok(_) => rejected_count += 1,
                Err(e) => warn!("Failed to reject transaction {queue_id}: {e}"),
            }
        }

        info!("Officer {officer_id} bulk rejected {rejected_count} transactions");
        Ok(rejected_count)
    }

    /// Clean up expired transactions
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let mut expired_count = 0;
        let now = Utc::now();
        let max_age = chrono::Duration::hours(self.config.max_queue_time_hours as i64);
        let max_review_time = chrono::Duration::hours(self.config.max_review_time_hours as i64);

        let mut to_expire = Vec::new();

        // Find expired transactions
        {
            let transactions = self.transactions.read().await;
            for (queue_id, transaction) in transactions.iter() {
                let should_expire = match &transaction.status {
                    ReviewStatus::Pending => (now - transaction.queued_at) > max_age,
                    ReviewStatus::InReview { started_at, .. } => {
                        (now - *started_at) > max_review_time
                    }
                    _ => false,
                };

                if should_expire {
                    to_expire.push(*queue_id);
                }
            }
        }

        // Expire the transactions
        {
            let mut transactions = self.transactions.write().await;
            let mut hash_map = self.hash_to_queue_id.write().await;
            let mut pending = self.pending_queue.write().await;

            for queue_id in to_expire {
                if let Some(transaction) = transactions.get_mut(&queue_id) {
                    transaction.status = ReviewStatus::Expired { expired_at: now };
                    hash_map.remove(&transaction.transaction_hash);

                    // Remove from pending queue if present
                    pending.retain(|t| t.queue_id != queue_id);

                    self.send_notification(NotificationType::TransactionExpired {
                        queue_id,
                        transaction_hash: hex::encode(&transaction.transaction_hash),
                        expiry_time: now,
                    })
                    .await;

                    expired_count += 1;
                }
            }
        }

        if expired_count > 0 {
            info!("Expired {expired_count} transactions from queue");
            let _ = self.update_stats().await;
        }

        Ok(expired_count)
    }

    /// Calculate priority based on AI result
    fn calculate_priority(&self, ai_result: &AIVerificationResult) -> ReviewPriority {
        if !self.config.enable_auto_prioritization {
            return ReviewPriority::Medium;
        }

        let risk_score = match ai_result {
            AIVerificationResult::Verified { risk_score, .. } => risk_score.unwrap_or(0.5),
            AIVerificationResult::Failed { .. } => 1.0, // Max risk for failed verification
            AIVerificationResult::Unavailable { .. } => 0.7, // High risk for unavailable
            AIVerificationResult::Skipped { .. } => 0.3, // Low risk for skipped
        };

        if risk_score >= self.config.critical_priority_threshold {
            ReviewPriority::Critical
        } else if risk_score >= self.config.high_priority_threshold {
            ReviewPriority::High
        } else if risk_score >= 0.6 {
            ReviewPriority::Medium
        } else {
            ReviewPriority::Low
        }
    }

    /// Generate tags based on risk decision and AI result
    fn generate_tags(
        &self,
        risk_decision: &RiskProcessingDecision,
        ai_result: &AIVerificationResult,
    ) -> Vec<String> {
        let mut tags = Vec::new();

        // Add tag based on decision reason
        if let RiskProcessingDecision::RequireReview { reason } = risk_decision {
            tags.push(format!("review-reason:{reason}"));
        }

        // Add tags based on AI result
        match ai_result {
            AIVerificationResult::Verified {
                risk_score,
                fraud_probability,
                ..
            } => {
                if let Some(score) = risk_score {
                    if *score > 0.8 {
                        tags.push("high-risk".to_string());
                    }
                }
                if let Some(fraud_prob) = fraud_probability {
                    if *fraud_prob > 0.7 {
                        tags.push("fraud-risk".to_string());
                    }
                }
            }
            AIVerificationResult::Failed { .. } => {
                tags.push("verification-failed".to_string());
            }
            AIVerificationResult::Unavailable { .. } => {
                tags.push("ai-unavailable".to_string());
            }
            AIVerificationResult::Skipped { .. } => {
                tags.push("ai-skipped".to_string());
            }
        }

        tags
    }

    /// Send a notification (placeholder implementation)
    async fn send_notification(&self, notification: NotificationType) {
        if !self.config.enable_notifications {
            return;
        }

        let mut queue = self.notification_queue.lock().await;
        queue.push_back(notification.clone());

        // Log the notification for now
        match notification {
            NotificationType::NewHighRiskTransaction {
                queue_id,
                transaction_hash: _,
                risk_score: _,
                ref priority,
            } => {
                warn!("🚨 New {priority:?} priority transaction queued for review: {queue_id}");
            }
            NotificationType::TransactionExpired { queue_id, .. } => {
                warn!("⏰ Transaction expired in queue: {queue_id}");
            }
            NotificationType::ReviewTimeout {
                queue_id,
                ref officer_id,
                ..
            } => {
                warn!("⏰ Review timeout for transaction {queue_id} (officer: {officer_id})");
            }
            NotificationType::QueueCapacityWarning {
                current_size,
                max_size,
                ..
            } => {
                warn!("⚠️ Queue approaching capacity: {current_size}/{max_size}");
            }
            _ => {
                // Handle other notification types generically
                info!("Notification sent: {}", notification.title());
            }
        }

        // Send notification through the notification system
        if let Some(notification_system) = &self.notification_system {
            let _ = notification_system.send_notification(notification).await;
        }
    }

    /// Update queue statistics
    async fn update_stats(&self) -> Result<()> {
        let mut stats = self.stats.write().await;
        let transactions = self.transactions.read().await;
        let now = Utc::now();

        // Reset counters
        stats.total_pending = 0;
        stats.total_in_review = 0;
        stats.priority_breakdown.clear();

        let mut oldest_pending: Option<DateTime<Utc>> = None;
        let mut review_times = Vec::new();

        // Calculate statistics
        for transaction in transactions.values() {
            match &transaction.status {
                ReviewStatus::Pending => {
                    stats.total_pending += 1;
                    if oldest_pending.is_none() || transaction.queued_at < oldest_pending.unwrap() {
                        oldest_pending = Some(transaction.queued_at);
                    }
                    *stats
                        .priority_breakdown
                        .entry(transaction.priority.clone())
                        .or_insert(0) += 1;
                }
                ReviewStatus::InReview { .. } => {
                    stats.total_in_review += 1;
                    *stats
                        .priority_breakdown
                        .entry(transaction.priority.clone())
                        .or_insert(0) += 1;
                }
                ReviewStatus::Approved { approved_at, .. } => {
                    if approved_at.date_naive() == now.date_naive() {
                        stats.total_approved_today += 1;
                    }
                    let review_time = (*approved_at - transaction.queued_at).num_minutes() as f64;
                    review_times.push(review_time);
                }
                ReviewStatus::Rejected { rejected_at, .. } => {
                    if rejected_at.date_naive() == now.date_naive() {
                        stats.total_rejected_today += 1;
                    }
                    let review_time = (*rejected_at - transaction.queued_at).num_minutes() as f64;
                    review_times.push(review_time);
                }
                _ => {}
            }
        }

        // Calculate average review time
        if !review_times.is_empty() {
            stats.average_review_time_minutes =
                review_times.iter().sum::<f64>() / review_times.len() as f64;
        }

        // Calculate oldest pending age
        if let Some(oldest) = oldest_pending {
            stats.oldest_pending_age_hours = (now - oldest).num_hours() as f64;
        }

        Ok(())
    }

    /// Get pending notifications (for compliance officers)
    pub async fn get_notifications(&self) -> Vec<NotificationType> {
        let mut queue = self.notification_queue.lock().await;
        let notifications: Vec<_> = queue.drain(..).collect();
        notifications
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::ai_integration::RiskProcessingDecision;
    use crate::types::TransferTransaction;

    fn create_test_transaction() -> Transaction {
        Transaction::Transfer(TransferTransaction {
            hash: "test_tx_123".to_string(),
            from: "sender123".to_string(),
            to: "recipient456".to_string(),
            amount: 1000,
            fee: 10,
            nonce: 1,
            timestamp: Utc::now().timestamp() as u64,
            signature: crate::types::PQCTransactionSignature {
                signature: dytallix_pqc::Signature {
                    data: vec![0x01, 0x02, 0x03],
                    algorithm: dytallix_pqc::SignatureAlgorithm::Dilithium5,
                },
                public_key: vec![0x04, 0x05, 0x06],
            },
            ai_risk_score: Some(0.75),
        })
    }

    fn create_test_ai_result(risk_score: f64) -> AIVerificationResult {
        AIVerificationResult::Verified {
            risk_score: Some(risk_score),
            confidence: Some(0.95),
            oracle_id: "test-oracle".to_string(),
            response_id: "test-response".to_string(),
            fraud_probability: Some(risk_score * 0.8),
            processing_decision: RiskProcessingDecision::RequireReview {
                reason: "High risk score".to_string(),
            },
        }
    }

    #[tokio::test]
    async fn test_enqueue_transaction() {
        let config = HighRiskQueueConfig::default();
        let queue = HighRiskQueue::new(config);

        let transaction = create_test_transaction();
        let tx_hash = [0u8; 32];
        let ai_result = create_test_ai_result(0.85);
        let risk_decision = RiskProcessingDecision::RequireReview {
            reason: "High risk score".to_string(),
        };

        let queue_id = queue
            .enqueue_transaction(transaction, hex::encode(tx_hash), ai_result, risk_decision)
            .await
            .unwrap();

        let stats = queue.get_statistics().await;
        assert_eq!(stats.total_pending, 1);

        let pending = queue.get_pending_transactions().await;
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].queue_id, queue_id);
    }

    #[tokio::test]
    async fn test_priority_ordering() {
        let config = HighRiskQueueConfig::default();
        let queue = HighRiskQueue::new(config);

        // Add transactions with different risk scores
        let tx1 = create_test_transaction();
        let tx2 = create_test_transaction();
        let tx3 = create_test_transaction();

        let low_risk = create_test_ai_result(0.3);
        let high_risk = create_test_ai_result(0.85);
        let critical_risk = create_test_ai_result(0.95);

        let risk_decision = RiskProcessingDecision::RequireReview {
            reason: "Test".to_string(),
        };

        // Add in reverse priority order
        queue
            .enqueue_transaction(tx1, hex::encode([1u8; 32]), low_risk, risk_decision.clone())
            .await
            .unwrap();
        queue
            .enqueue_transaction(
                tx2,
                hex::encode([2u8; 32]),
                high_risk,
                risk_decision.clone(),
            )
            .await
            .unwrap();
        queue
            .enqueue_transaction(tx3, hex::encode([3u8; 32]), critical_risk, risk_decision)
            .await
            .unwrap();

        let pending = queue.get_pending_transactions().await;
        assert_eq!(pending.len(), 3);

        // Should be ordered by priority: Critical, High, Low
        assert_eq!(pending[0].priority, ReviewPriority::Critical);
        assert_eq!(pending[1].priority, ReviewPriority::High);
        assert_eq!(pending[2].priority, ReviewPriority::Low);
    }

    #[tokio::test]
    async fn test_review_workflow() {
        let config = HighRiskQueueConfig::default();
        let queue = HighRiskQueue::new(config);

        let transaction = create_test_transaction();
        let tx_hash = [0u8; 32];
        let ai_result = create_test_ai_result(0.85);
        let risk_decision = RiskProcessingDecision::RequireReview {
            reason: "High risk score".to_string(),
        };

        let queue_id = queue
            .enqueue_transaction(transaction, hex::encode(tx_hash), ai_result, risk_decision)
            .await
            .unwrap();

        // Start review
        queue
            .start_review(queue_id, "officer1".to_string())
            .await
            .unwrap();

        let tx = queue.get_transaction(queue_id).await.unwrap();
        assert!(matches!(tx.status, ReviewStatus::InReview { .. }));

        // Approve transaction
        let approved = queue
            .approve_transaction(
                queue_id,
                "officer1".to_string(),
                Some("Looks good".to_string()),
            )
            .await
            .unwrap();

        assert!(matches!(approved.status, ReviewStatus::Approved { .. }));
    }

    #[tokio::test]
    async fn test_bulk_operations() {
        let config = HighRiskQueueConfig::default();
        let queue = HighRiskQueue::new(config);

        let mut queue_ids = Vec::new();
        let risk_decision = RiskProcessingDecision::RequireReview {
            reason: "Bulk test".to_string(),
        };

        // Add multiple transactions
        for i in 0..5 {
            let transaction = create_test_transaction();
            let mut tx_hash = [0u8; 32];
            tx_hash[0] = i;
            let ai_result = create_test_ai_result(0.75);

            let queue_id = queue
                .enqueue_transaction(
                    transaction,
                    hex::encode(tx_hash),
                    ai_result,
                    risk_decision.clone(),
                )
                .await
                .unwrap();
            queue_ids.push(queue_id);
        }

        // Bulk approve first 3
        let approved = queue
            .bulk_approve(queue_ids[0..3].to_vec(), "officer1".to_string())
            .await
            .unwrap();
        assert_eq!(approved.len(), 3);

        // Bulk reject last 2
        let rejected_count = queue
            .bulk_reject(
                queue_ids[3..5].to_vec(),
                "officer1".to_string(),
                "Bulk rejection test".to_string(),
            )
            .await
            .unwrap();
        assert_eq!(rejected_count, 2);
    }

    #[cfg(test)]
    mod integration_tests {
        use super::*;
        use crate::consensus::ai_integration::{AIVerificationResult, RiskProcessingDecision};
        use crate::types::{PQCTransactionSignature, TransferTransaction};

        #[tokio::test]
        async fn test_end_to_end_queue_workflow() {
            // Create a queue with default config
            let config = HighRiskQueueConfig::default();
            let queue = HighRiskQueue::new(config);

            // Create a sample transaction
            let tx = Transaction::Transfer(TransferTransaction {
                hash: "test_tx_123".to_string(),
                from: "alice".to_string(),
                to: "bob".to_string(),
                amount: 1000,
                fee: 10,
                nonce: 1,
                timestamp: 1234567890,
                signature: PQCTransactionSignature {
                    signature: dytallix_pqc::Signature {
                        data: vec![],
                        algorithm: dytallix_pqc::SignatureAlgorithm::Dilithium5,
                    },
                    public_key: vec![],
                },
                ai_risk_score: Some(0.9),
            });

            // Create AI result indicating high risk requiring review
            let ai_result = AIVerificationResult::Verified {
                risk_score: Some(0.9),
                processing_decision: RiskProcessingDecision::RequireReview {
                    reason: "High fraud probability detected".to_string(),
                },
                fraud_probability: Some(0.85),
                confidence: Some(0.95),
                oracle_id: "test_oracle".to_string(),
                response_id: "test_response".to_string(),
            };

            // Enqueue the transaction
            let queue_id = queue
                .enqueue_transaction(
                    tx,
                    "test_tx_123".to_string(),
                    ai_result,
                    RiskProcessingDecision::RequireReview {
                        reason: "High fraud probability detected".to_string(),
                    },
                )
                .await
                .expect("Failed to enqueue transaction");

            // Verify the transaction is in the queue
            let pending = queue.get_pending_transactions().await;
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].queue_id, queue_id);

            // Approve the transaction
            let result = queue
                .approve_transaction(
                    queue_id,
                    "compliance_officer_1".to_string(),
                    Some("Approved after review".to_string()),
                )
                .await;
            assert!(result.is_ok());

            // Verify the transaction is no longer pending
            let pending_after = queue.get_pending_transactions().await;
            assert_eq!(pending_after.len(), 0);

            // Check statistics
            let stats = queue.get_statistics().await;
            assert_eq!(stats.total_approved_today, 1);
            assert_eq!(stats.total_pending, 0);
        }

        #[tokio::test]
        async fn test_notification_integration() {
            let config = HighRiskQueueConfig::default();
            let queue = HighRiskQueue::new(config);

            // Create a transaction that will trigger notifications
            let tx = Transaction::Transfer(TransferTransaction {
                hash: "notification_test_tx".to_string(),
                from: "user1".to_string(),
                to: "user2".to_string(),
                amount: 5000,
                fee: 20,
                nonce: 1,
                timestamp: 1234567890,
                signature: PQCTransactionSignature {
                    signature: dytallix_pqc::Signature {
                        data: vec![],
                        algorithm: dytallix_pqc::SignatureAlgorithm::Dilithium5,
                    },
                    public_key: vec![],
                },
                ai_risk_score: Some(0.95),
            });

            let ai_result = AIVerificationResult::Verified {
                risk_score: Some(0.95),
                processing_decision: RiskProcessingDecision::RequireReview {
                    reason: "Suspicious transaction pattern".to_string(),
                },
                fraud_probability: Some(0.9),
                confidence: Some(0.98),
                oracle_id: "test_oracle".to_string(),
                response_id: "test_response".to_string(),
            };

            // This should trigger a notification
            let _queue_id = queue
                .enqueue_transaction(
                    tx,
                    "notification_test_tx".to_string(),
                    ai_result,
                    RiskProcessingDecision::RequireReview {
                        reason: "Suspicious transaction pattern".to_string(),
                    },
                )
                .await
                .expect("Failed to enqueue transaction");

            // Verify notification was queued (basic check)
            let notification_queue = queue.notification_queue.lock().await;
            assert!(!notification_queue.is_empty());
        }
    }
}
