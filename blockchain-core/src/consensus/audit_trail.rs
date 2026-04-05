//! AI Audit Trail and Compliance System
//!
//! This module implements comprehensive audit logging and compliance features
//! for all AI interactions within the blockchain. It provides:
//! - Immutable audit trail storage for all AI decisions
//! - Compliance reporting and querying capabilities
//! - Data retention policy management
//! - Regulatory export functionality

use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use log::info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::consensus::ai_integration::{AIVerificationResult, RiskProcessingDecision};
use crate::consensus::notification_types::ReviewPriority;
use crate::types::{Address, Transaction, TxHash};

/// Comprehensive audit entry for AI decisions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    /// Unique identifier for this audit entry
    pub audit_id: Uuid,
    /// Transaction hash this audit relates to
    pub transaction_hash: TxHash,
    /// Block number where transaction was processed
    pub block_number: Option<u64>,
    /// Timestamp when AI decision was made
    pub timestamp: DateTime<Utc>,
    /// AI verification result and risk assessment
    pub ai_result: AIVerificationResult,
    /// Risk processing decision made
    pub risk_decision: RiskProcessingDecision,
    /// Risk priority assigned to transaction
    pub risk_priority: ReviewPriority,
    /// Oracle ID that provided the AI decision
    pub oracle_id: String,
    /// Request ID for tracing AI service calls
    pub request_id: String,
    /// Transaction metadata for compliance
    pub transaction_metadata: TransactionMetadata,
    /// Compliance status and notes
    pub compliance_status: ComplianceStatus,
    /// Data retention information
    pub retention_info: RetentionInfo,
}

/// Transaction metadata extracted for compliance reporting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionMetadata {
    /// Transaction type (Transfer, Deploy, etc.)
    pub transaction_type: String,
    /// From address
    pub from_address: Option<Address>,
    /// To address
    pub to_address: Option<Address>,
    /// Transaction amount
    pub amount: Option<u128>,
    /// Transaction fee
    pub fee: Option<u128>,
    /// Transaction timestamp
    pub transaction_timestamp: u64,
    /// Additional contextual data
    pub additional_data: HashMap<String, String>,
}

/// Compliance status tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ComplianceStatus {
    /// Pending compliance review
    Pending,
    /// Passed automated compliance checks
    AutoApproved,
    /// Requires manual compliance review
    ManualReviewRequired,
    /// Approved after manual review
    ManualApproved { officer_id: String, notes: String },
    /// Failed compliance checks
    Failed { reason: String },
    /// Flagged for investigation
    Flagged {
        reason: String,
        investigator: Option<String>,
    },
}

/// Data retention policy information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionInfo {
    /// Data classification level
    pub classification: DataClassification,
    /// Retention period in days
    pub retention_days: u32,
    /// Archive date when data should be archived
    pub archive_date: DateTime<Utc>,
    /// Deletion date when data should be purged
    pub deletion_date: DateTime<Utc>,
    /// Whether this entry is subject to legal hold
    pub legal_hold: bool,
}

/// Data classification levels for retention policies
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DataClassification {
    /// Standard business data (7 years retention)
    Standard,
    /// Financial compliance data (10 years retention)
    Financial,
    /// High-risk/suspicious activity (indefinite retention)
    HighRisk,
    /// Legal investigation data (indefinite retention until resolved)
    Legal,
}

/// Compliance report filtering and query parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceQuery {
    /// Date range for the report
    pub date_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    /// Filter by compliance status
    pub status_filter: Option<Vec<ComplianceStatus>>,
    /// Filter by risk priority
    pub priority_filter: Option<Vec<ReviewPriority>>,
    /// Filter by transaction type
    pub transaction_type_filter: Option<Vec<String>>,
    /// Filter by oracle ID
    pub oracle_filter: Option<Vec<String>>,
    /// Filter by address (from/to)
    pub address_filter: Option<Vec<Address>>,
    /// Minimum amount threshold
    pub min_amount: Option<u128>,
    /// Maximum amount threshold
    pub max_amount: Option<u128>,
    /// Include deleted/archived entries
    pub include_archived: bool,
    /// Pagination offset
    pub offset: usize,
    /// Pagination limit
    pub limit: usize,
}

/// Compliance report summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceReportSummary {
    /// Total number of entries in query period
    pub total_entries: usize,
    /// Breakdown by compliance status
    pub status_breakdown: HashMap<String, usize>,
    /// Breakdown by risk priority
    pub priority_breakdown: HashMap<ReviewPriority, usize>,
    /// Breakdown by transaction type
    pub transaction_type_breakdown: HashMap<String, usize>,
    /// Average risk score
    pub average_risk_score: f64,
    /// Total transaction volume
    pub total_volume: u128,
    /// Number of manual reviews required
    pub manual_reviews_required: usize,
    /// Number of flagged transactions
    pub flagged_transactions: usize,
}

/// Configuration for audit trail system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Enable audit trail logging
    pub enabled: bool,
    /// Maximum number of audit entries to keep in memory
    pub max_memory_entries: usize,
    /// Batch size for database writes
    pub batch_write_size: usize,
    /// How often to flush audit entries to storage (seconds)
    pub flush_interval_seconds: u64,
    /// Default retention policy
    pub default_retention: RetentionInfo,
    /// Auto-archive old entries
    pub auto_archive: bool,
    /// Compression for archived data
    pub compression_enabled: bool,
    /// Encryption for sensitive audit data
    pub encryption_enabled: bool,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_memory_entries: 10000,
            batch_write_size: 100,
            flush_interval_seconds: 60,
            default_retention: RetentionInfo {
                classification: DataClassification::Standard,
                retention_days: 2557, // 7 years
                archive_date: Utc::now() + Duration::days(2557),
                deletion_date: Utc::now() + Duration::days(2557 + 30), // 30 day grace period
                legal_hold: false,
            },
            auto_archive: true,
            compression_enabled: true,
            encryption_enabled: true,
        }
    }
}

/// Arguments for recording an AI decision
pub struct RecordAiDecisionArgs<'a> {
    pub transaction: &'a Transaction,
    pub transaction_hash: TxHash,
    pub ai_result: AIVerificationResult,
    pub risk_decision: RiskProcessingDecision,
    pub risk_priority: ReviewPriority,
    pub oracle_id: String,
    pub request_id: String,
    pub block_number: Option<u64>,
}

/// Main audit trail manager
#[derive(Debug)]
pub struct AuditTrailManager {
    config: AuditConfig,
    /// In-memory audit entries waiting to be persisted
    pending_entries: Arc<RwLock<Vec<AuditEntry>>>,
    /// Audit entries indexed by transaction hash
    audit_index: Arc<RwLock<HashMap<TxHash, Vec<Uuid>>>>,
    /// All audit entries (would be persisted to database in production)
    audit_storage: Arc<RwLock<HashMap<Uuid, AuditEntry>>>,
    /// Statistics for monitoring
    stats: Arc<RwLock<AuditStatistics>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditStatistics {
    pub total_entries: usize,
    pub entries_today: usize,
    pub pending_entries: usize,
    pub archived_entries: usize,
    pub last_flush_time: DateTime<Utc>,
    pub flush_failures: usize,
    pub query_count: usize,
    pub last_query_time: Option<DateTime<Utc>>,
}

impl AuditTrailManager {
    /// Create a new audit trail manager
    pub fn new(config: AuditConfig) -> Self {
        Self {
            config,
            pending_entries: Arc::new(RwLock::new(Vec::new())),
            audit_index: Arc::new(RwLock::new(HashMap::new())),
            audit_storage: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(AuditStatistics {
                total_entries: 0,
                entries_today: 0,
                pending_entries: 0,
                archived_entries: 0,
                last_flush_time: Utc::now(),
                flush_failures: 0,
                query_count: 0,
                last_query_time: None,
            })),
        }
    }

    /// Record an AI decision in the audit trail
    pub async fn record_ai_decision(&self, args: RecordAiDecisionArgs<'_>) -> Result<Uuid> {
        if !self.config.enabled {
            return Ok(Uuid::new_v4()); // Return dummy ID if disabled
        }

        let audit_id = Uuid::new_v4();
        let now = Utc::now();

        // Extract transaction metadata
        let transaction_metadata = self.extract_transaction_metadata(args.transaction);

        // Determine compliance status based on AI result and risk decision
        let compliance_status =
            self.determine_compliance_status(&args.ai_result, &args.risk_decision);

        // Determine retention policy based on transaction characteristics
        let retention_info =
            self.determine_retention_policy(&transaction_metadata, &compliance_status);

        let audit_entry = AuditEntry {
            audit_id,
            transaction_hash: args.transaction_hash.clone(),
            block_number: args.block_number,
            timestamp: now,
            ai_result: args.ai_result,
            risk_decision: args.risk_decision,
            risk_priority: args.risk_priority,
            oracle_id: args.oracle_id,
            request_id: args.request_id,
            transaction_metadata,
            compliance_status,
            retention_info,
        };

        // Add to pending entries
        {
            let mut pending = self.pending_entries.write().await;
            pending.push(audit_entry.clone());
        }

        // Update index
        {
            let mut index = self.audit_index.write().await;
            index
                .entry(args.transaction_hash.clone())
                .or_insert_with(Vec::new)
                .push(audit_id);
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.pending_entries += 1;
        }

        info!(
            "Recorded audit entry {} for transaction {}",
            audit_id,
            hex::encode(&args.transaction_hash)
        );

        // Check if we need to flush
        if self.should_flush().await {
            self.flush_pending_entries().await?;
        }

        Ok(audit_id)
    }

    /// Flush pending audit entries to persistent storage
    pub async fn flush_pending_entries(&self) -> Result<usize> {
        let entries_to_flush = {
            let mut pending = self.pending_entries.write().await;
            let entries = pending.clone();
            pending.clear();
            entries
        };

        let flush_count = entries_to_flush.len();
        if flush_count == 0 {
            return Ok(0);
        }

        // In production, this would write to a database
        // For now, we'll store in memory
        {
            let mut storage = self.audit_storage.write().await;
            for entry in entries_to_flush {
                storage.insert(entry.audit_id, entry);
            }
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.total_entries += flush_count;
            stats.pending_entries = 0;
            stats.last_flush_time = Utc::now();
            stats.entries_today += flush_count; // Simplified - would need date tracking
        }

        info!("Flushed {flush_count} audit entries to storage");
        Ok(flush_count)
    }

    /// Query audit entries with filtering and pagination
    pub async fn query_audit_entries(
        &self,
        query: ComplianceQuery,
    ) -> Result<(Vec<AuditEntry>, ComplianceReportSummary)> {
        // Update query statistics
        {
            let mut stats = self.stats.write().await;
            stats.query_count += 1;
            stats.last_query_time = Some(Utc::now());
        }

        let storage = self.audit_storage.read().await;
        let mut matching_entries = Vec::new();
        let mut summary_stats = HashMap::new();
        let mut total_volume = 0u128;
        let mut risk_scores = Vec::new();

        for entry in storage.values() {
            if self.entry_matches_query(entry, &query) {
                matching_entries.push(entry.clone());

                // Update summary statistics
                let status_key = format!("{:?}", entry.compliance_status);
                *summary_stats.entry(status_key).or_insert(0) += 1;

                if let Some(amount) = entry.transaction_metadata.amount {
                    total_volume += amount;
                }

                // Extract risk score for average calculation
                if let AIVerificationResult::Verified {
                    risk_score: Some(score),
                    ..
                } = &entry.ai_result
                {
                    risk_scores.push(*score);
                }
            }
        }

        // Sort by timestamp (newest first) and apply pagination
        matching_entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        let total_count = matching_entries.len();
        let paginated_entries = matching_entries
            .into_iter()
            .skip(query.offset)
            .take(query.limit)
            .collect();

        // Calculate summary statistics
        let average_risk_score = if risk_scores.is_empty() {
            0.0
        } else {
            risk_scores.iter().sum::<f64>() / risk_scores.len() as f64
        };

        let summary = ComplianceReportSummary {
            total_entries: total_count,
            status_breakdown: summary_stats,
            priority_breakdown: HashMap::new(), // Would be calculated similarly
            transaction_type_breakdown: HashMap::new(), // Would be calculated similarly
            average_risk_score,
            total_volume,
            manual_reviews_required: 0, // Would be calculated from entries
            flagged_transactions: 0,    // Would be calculated from entries
        };

        Ok((paginated_entries, summary))
    }

    /// Export audit data for regulatory compliance
    pub async fn export_compliance_data(
        &self,
        query: ComplianceQuery,
        format: ExportFormat,
    ) -> Result<Vec<u8>> {
        let (entries, _summary) = self.query_audit_entries(query).await?;

        match format {
            ExportFormat::Json => {
                let json_data = serde_json::to_vec_pretty(&entries)?;
                Ok(json_data)
            }
            ExportFormat::Csv => {
                // Create CSV format for regulatory reporting
                let mut csv_data = Vec::new();
                csv_data.extend_from_slice(b"audit_id,transaction_hash,timestamp,oracle_id,risk_score,compliance_status,amount,from_address,to_address\n");

                for entry in entries {
                    let risk_score = match &entry.ai_result {
                        AIVerificationResult::Verified {
                            risk_score: Some(score),
                            ..
                        } => score.to_string(),
                        _ => "N/A".to_string(),
                    };

                    let line = format!(
                        "{},{},{},{},{},{:?},{},{},{}\n",
                        entry.audit_id,
                        hex::encode(&entry.transaction_hash),
                        entry.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
                        entry.oracle_id,
                        risk_score,
                        entry.compliance_status,
                        entry.transaction_metadata.amount.unwrap_or(0),
                        entry
                            .transaction_metadata
                            .from_address
                            .as_deref()
                            .unwrap_or("N/A"),
                        entry
                            .transaction_metadata
                            .to_address
                            .as_deref()
                            .unwrap_or("N/A")
                    );
                    csv_data.extend_from_slice(line.as_bytes());
                }

                Ok(csv_data)
            }
        }
    }

    /// Get audit entries for a specific transaction
    pub async fn get_transaction_audit_trail(&self, transaction_hash: &TxHash) -> Vec<AuditEntry> {
        let index = self.audit_index.read().await;
        let storage = self.audit_storage.read().await;

        if let Some(audit_ids) = index.get(transaction_hash) {
            audit_ids
                .iter()
                .filter_map(|id| storage.get(id))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get current audit statistics
    pub async fn get_statistics(&self) -> AuditStatistics {
        let stats = self.stats.read().await;
        stats.clone()
    }

    /// Archive old audit entries based on retention policies
    pub async fn archive_old_entries(&self) -> Result<usize> {
        if !self.config.auto_archive {
            return Ok(0);
        }

        let now = Utc::now();
        let mut archived_count = 0;

        // In production, this would move entries to cold storage
        let storage = self.audit_storage.read().await;
        for entry in storage.values() {
            if entry.retention_info.archive_date <= now && !entry.retention_info.legal_hold {
                // Would archive to cold storage here
                archived_count += 1;
            }
        }

        if archived_count > 0 {
            info!("Archived {archived_count} old audit entries");

            let mut stats = self.stats.write().await;
            stats.archived_entries += archived_count;
        }

        Ok(archived_count)
    }

    /// Update compliance status for an audit entry
    pub async fn update_compliance_status(
        &self,
        audit_id: Uuid,
        new_status: ComplianceStatus,
    ) -> Result<()> {
        let mut storage = self.audit_storage.write().await;

        if let Some(entry) = storage.get_mut(&audit_id) {
            entry.compliance_status = new_status;
            info!("Updated compliance status for audit entry {audit_id}");
            Ok(())
        } else {
            Err(anyhow!("Audit entry not found: {}", audit_id))
        }
    }

    // Helper methods

    async fn should_flush(&self) -> bool {
        let pending = self.pending_entries.read().await;
        pending.len() >= self.config.batch_write_size
    }

    fn extract_transaction_metadata(&self, transaction: &Transaction) -> TransactionMetadata {
        match transaction {
            Transaction::Transfer(tx) => TransactionMetadata {
                transaction_type: "Transfer".to_string(),
                from_address: Some(tx.from.clone()),
                to_address: Some(tx.to.clone()),
                amount: Some(tx.amount),
                fee: Some(tx.fee),
                transaction_timestamp: tx.timestamp,
                additional_data: HashMap::new(),
            },
            Transaction::Deploy(_) => TransactionMetadata {
                transaction_type: "Deploy".to_string(),
                from_address: None,
                to_address: None,
                amount: None,
                fee: None,
                transaction_timestamp: Utc::now().timestamp() as u64,
                additional_data: HashMap::new(),
            },
            Transaction::Call(_) => TransactionMetadata {
                transaction_type: "Call".to_string(),
                from_address: None,
                to_address: None,
                amount: None,
                fee: None,
                transaction_timestamp: Utc::now().timestamp() as u64,
                additional_data: HashMap::new(),
            },
            Transaction::Stake(_) => TransactionMetadata {
                transaction_type: "Stake".to_string(),
                from_address: None,
                to_address: None,
                amount: None,
                fee: None,
                transaction_timestamp: Utc::now().timestamp() as u64,
                additional_data: HashMap::new(),
            },
            Transaction::AIRequest(_) => TransactionMetadata {
                transaction_type: "AIRequest".to_string(),
                from_address: None,
                to_address: None,
                amount: None,
                fee: None,
                transaction_timestamp: Utc::now().timestamp() as u64,
                additional_data: HashMap::new(),
            },
        }
    }

    fn determine_compliance_status(
        &self,
        _ai_result: &AIVerificationResult,
        risk_decision: &RiskProcessingDecision,
    ) -> ComplianceStatus {
        match risk_decision {
            RiskProcessingDecision::AutoApprove => ComplianceStatus::AutoApproved,
            RiskProcessingDecision::RequireReview { .. } => ComplianceStatus::ManualReviewRequired,
            RiskProcessingDecision::AutoReject { reason } => ComplianceStatus::Failed {
                reason: reason.clone(),
            },
        }
    }

    fn determine_retention_policy(
        &self,
        metadata: &TransactionMetadata,
        status: &ComplianceStatus,
    ) -> RetentionInfo {
        let classification = match status {
            ComplianceStatus::Failed { .. } | ComplianceStatus::Flagged { .. } => {
                DataClassification::HighRisk
            }
            _ => {
                if metadata.amount.unwrap_or(0) > 10000 {
                    DataClassification::Financial
                } else {
                    DataClassification::Standard
                }
            }
        };

        let retention_days = match classification {
            DataClassification::Standard => 2557,  // 7 years
            DataClassification::Financial => 3653, // 10 years
            DataClassification::HighRisk => 7305,  // 20 years
            DataClassification::Legal => 36525,    // 100 years (indefinite)
        };

        RetentionInfo {
            classification: classification.clone(),
            retention_days,
            archive_date: Utc::now() + Duration::days(retention_days as i64),
            deletion_date: Utc::now() + Duration::days(retention_days as i64 + 30),
            legal_hold: matches!(classification, DataClassification::Legal),
        }
    }

    fn entry_matches_query(&self, entry: &AuditEntry, query: &ComplianceQuery) -> bool {
        // Date range filter
        if let Some((start, end)) = &query.date_range {
            if entry.timestamp < *start || entry.timestamp > *end {
                return false;
            }
        }

        // Status filter
        if let Some(statuses) = &query.status_filter {
            if !statuses.contains(&entry.compliance_status) {
                return false;
            }
        }

        // Priority filter
        if let Some(priorities) = &query.priority_filter {
            if !priorities.contains(&entry.risk_priority) {
                return false;
            }
        }

        // Transaction type filter
        if let Some(types) = &query.transaction_type_filter {
            if !types.contains(&entry.transaction_metadata.transaction_type) {
                return false;
            }
        }

        // Oracle filter
        if let Some(oracles) = &query.oracle_filter {
            if !oracles.contains(&entry.oracle_id) {
                return false;
            }
        }

        // Amount filters
        if let Some(amount) = entry.transaction_metadata.amount {
            if let Some(min_amount) = query.min_amount {
                if amount < min_amount {
                    return false;
                }
            }
            if let Some(max_amount) = query.max_amount {
                if amount > max_amount {
                    return false;
                }
            }
        }

        true
    }
}

/// Export format options for compliance data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExportFormat {
    Json,
    Csv,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::TransferTransaction;

    fn create_test_transaction() -> Transaction {
        Transaction::Transfer(TransferTransaction {
            hash: "test_tx_123".to_string(),
            from: "alice".to_string(),
            to: "bob".to_string(),
            amount: 1000,
            fee: 10,
            nonce: 1,
            timestamp: 1234567890,
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

    fn create_test_ai_result() -> AIVerificationResult {
        AIVerificationResult::Verified {
            risk_score: Some(0.75),
            confidence: Some(0.95),
            oracle_id: "test-oracle".to_string(),
            response_id: "test-response".to_string(),
            fraud_probability: Some(0.6),
            processing_decision: RiskProcessingDecision::RequireReview {
                reason: "Medium risk score".to_string(),
            },
        }
    }

    #[tokio::test]
    async fn test_audit_entry_recording() {
        let config = AuditConfig::default();
        let audit_manager = AuditTrailManager::new(config);

        let transaction = create_test_transaction();
        let tx_hash = "test_tx_hash".to_string();
        let ai_result = create_test_ai_result();
        let risk_decision = RiskProcessingDecision::RequireReview {
            reason: "Test review".to_string(),
        };

        let audit_id = audit_manager
            .record_ai_decision(RecordAiDecisionArgs {
                transaction: &transaction,
                transaction_hash: tx_hash.clone(),
                ai_result,
                risk_decision,
                risk_priority: ReviewPriority::Medium,
                oracle_id: "test-oracle".to_string(),
                request_id: "test-request".to_string(),
                block_number: Some(12345),
            })
            .await
            .unwrap();

        assert!(!audit_id.is_nil());

        // Verify the entry was recorded
        let trail = audit_manager.get_transaction_audit_trail(&tx_hash).await;
        assert_eq!(trail.len(), 1);
        assert_eq!(trail[0].audit_id, audit_id);
    }

    #[tokio::test]
    async fn test_compliance_query() {
        let config = AuditConfig::default();
        let audit_manager = AuditTrailManager::new(config);

        // Record multiple audit entries
        for i in 0..5 {
            let transaction = create_test_transaction();
            let tx_hash = format!("test_tx_{}", i);
            let ai_result = create_test_ai_result();
            let risk_decision = RiskProcessingDecision::RequireReview {
                reason: format!("Test review {}", i),
            };

            audit_manager
                .record_ai_decision(RecordAiDecisionArgs {
                    transaction: &transaction,
                    transaction_hash: tx_hash,
                    ai_result,
                    risk_decision,
                    risk_priority: ReviewPriority::Medium,
                    oracle_id: "test-oracle".to_string(),
                    request_id: format!("test-request-{}", i),
                    block_number: Some(12345 + i),
                })
                .await
                .unwrap();
        }

        // Flush entries to storage
        audit_manager.flush_pending_entries().await.unwrap();

        // Query all entries
        let query = ComplianceQuery {
            date_range: None,
            status_filter: None,
            priority_filter: None,
            transaction_type_filter: None,
            oracle_filter: None,
            address_filter: None,
            min_amount: None,
            max_amount: None,
            include_archived: false,
            offset: 0,
            limit: 10,
        };

        let (entries, summary) = audit_manager.query_audit_entries(query).await.unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(summary.total_entries, 5);
    }

    #[tokio::test]
    async fn test_compliance_data_export() {
        let config = AuditConfig::default();
        let audit_manager = AuditTrailManager::new(config);

        // Record an audit entry
        let transaction = create_test_transaction();
        let tx_hash = "export_test_tx".to_string();
        let ai_result = create_test_ai_result();
        let risk_decision = RiskProcessingDecision::RequireReview {
            reason: "Export test".to_string(),
        };

        audit_manager
            .record_ai_decision(RecordAiDecisionArgs {
                transaction: &transaction,
                transaction_hash: tx_hash,
                ai_result,
                risk_decision,
                risk_priority: ReviewPriority::Medium,
                oracle_id: "test-oracle".to_string(),
                request_id: "export-request".to_string(),
                block_number: Some(12345),
            })
            .await
            .unwrap();

        audit_manager.flush_pending_entries().await.unwrap();

        // Test JSON export
        let query = ComplianceQuery {
            date_range: None,
            status_filter: None,
            priority_filter: None,
            transaction_type_filter: None,
            oracle_filter: None,
            address_filter: None,
            min_amount: None,
            max_amount: None,
            include_archived: false,
            offset: 0,
            limit: 10,
        };

        let json_data = audit_manager
            .export_compliance_data(query.clone(), ExportFormat::Json)
            .await
            .unwrap();
        assert!(!json_data.is_empty());

        // Test CSV export
        let csv_data = audit_manager
            .export_compliance_data(query, ExportFormat::Csv)
            .await
            .unwrap();
        assert!(!csv_data.is_empty());

        // Verify CSV header
        let csv_string = String::from_utf8(csv_data).unwrap();
        assert!(csv_string.starts_with("audit_id,transaction_hash,timestamp"));
    }
}
