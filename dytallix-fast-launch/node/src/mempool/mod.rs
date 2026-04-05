use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::crypto::{canonical_json, sha3_256, PQCAlgorithm, PQCVerifyError};
use crate::gas::{validate_gas_limit, GasSchedule, TxKind};
use crate::state::State;
use crate::storage::tx::{Transaction, TxMessage};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use dytallix_node::policy::signature_policy::{PolicyError, PolicyManager};

#[cfg(test)]
mod gas_tests;

#[cfg(test)]
mod pqc_tests;

/// Configuration constants - can be overridden by environment variables
pub const DEFAULT_MAX_TX_BYTES: usize = 1024 * 1024; // 1MB
pub const DEFAULT_MIN_GAS_PRICE: u64 = 1000; // 1000 wei
pub const DEFAULT_MEMPOOL_MAX_TXS: usize = 10000;
pub const DEFAULT_MEMPOOL_MAX_BYTES: usize = 100 * 1024 * 1024; // 100MB

/// Error code constants for external API responses
pub const TX_INVALID_SIG: &str = "TX_INVALID_SIG";

/// Rejection reasons for transactions
#[derive(Debug, Clone, PartialEq)]
pub enum RejectionReason {
    InvalidSignature,
    NonceGap { expected: u64, got: u64 },
    InsufficientFunds { denom: String, required: u128, available: u128 },
    UnderpricedGas { min: u64, got: u64 },
    OversizedTx { max: usize, got: usize },
    Duplicate(String),
    PolicyViolation(String),
    InternalError(String),
}

impl RejectionReason {
    /// Convert to metric label
    pub fn to_metric_label(&self) -> &'static str {
        match self {
            RejectionReason::InvalidSignature => "invalid_signature",
            RejectionReason::NonceGap { .. } => "nonce_gap",
            RejectionReason::InsufficientFunds { .. } => "insufficient_funds",
            RejectionReason::UnderpricedGas { .. } => "underpriced_gas",
            RejectionReason::OversizedTx { .. } => "oversized_tx",
            RejectionReason::Duplicate(_) => "duplicate",
            RejectionReason::PolicyViolation(_) => "policy_violation",
            RejectionReason::InternalError(_) => "internal_error",
        }
    }
}

impl std::fmt::Display for RejectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RejectionReason::Duplicate(hash) => write!(f, "duplicate tx {hash}"),
            RejectionReason::InvalidSignature => write!(f, "invalid signature"),
            RejectionReason::NonceGap { expected, got } => {
                write!(f, "nonce gap: expected {expected}, got {got}")
            }
            RejectionReason::InsufficientFunds { denom, required, available } => {
                write!(
                    f,
                    "insufficient funds for {}: required {}, available {}",
                    denom, required, available
                )
            }
            RejectionReason::UnderpricedGas { min, got } => {
                write!(f, "underpriced gas: min {min}, got {got}")
            }
            RejectionReason::OversizedTx { max, got } => {
                write!(f, "oversized transaction: max {max}, got {got}")
            }
            RejectionReason::PolicyViolation(msg) => write!(f, "policy violation: {msg}"),
            RejectionReason::InternalError(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

/// Configuration for mempool
#[derive(Debug, Clone)]
pub struct MempoolConfig {
    pub max_tx_bytes: usize,
    pub min_gas_price: u64,
    pub max_txs: usize,
    pub max_bytes: usize,
}

impl Default for MempoolConfig {
    fn default() -> Self {
        Self {
            max_tx_bytes: std::env::var("DYT_MAX_TX_BYTES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_MAX_TX_BYTES),
            min_gas_price: std::env::var("DYT_MIN_GAS_PRICE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_MIN_GAS_PRICE),
            max_txs: std::env::var("DYT_MEMPOOL_MAX_TXS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_MEMPOOL_MAX_TXS),
            max_bytes: std::env::var("DYT_MEMPOOL_MAX_BYTES")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_MEMPOOL_MAX_BYTES),
        }
    }
}

/// Transaction with priority ordering
#[derive(Debug, Clone)]
pub struct PendingTx {
    pub tx: Transaction,
    pub received_at: u64,
    pub serialized_size: usize,
}

/// Priority key for ordering transactions
/// Primary: gas_price desc, Secondary: nonce asc, Tertiary: tx_hash asc
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxPriorityKey {
    // made public so priority_key method returning it is valid
    gas_price_neg: i64, // negative for descending order
    nonce: u64,
    hash: String,
}

impl Ord for TxPriorityKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.gas_price_neg
            .cmp(&other.gas_price_neg)
            .then_with(|| self.nonce.cmp(&other.nonce))
            .then_with(|| self.hash.cmp(&other.hash))
    }
}

impl PartialOrd for TxPriorityKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PendingTx {
    pub fn new(tx: Transaction) -> Self {
        let serialized_size = estimate_tx_size(&tx);
        let received_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            tx,
            received_at,
            serialized_size,
        }
    }

    pub fn priority_key(&self) -> TxPriorityKey {
        TxPriorityKey {
            gas_price_neg: -(self.tx.gas_price as i64), // negative for descending order
            nonce: self.tx.nonce,
            hash: self.tx.hash.clone(),
        }
    }
}

/// Production-grade mempool with admission rules, ordering, and bounded capacity
pub struct Mempool {
    config: MempoolConfig,
    /// Policy manager for signature algorithm enforcement
    policy_manager: PolicyManager,
    /// Priority-ordered eligible transactions (BTreeSet for deterministic ordering)
    ordered_txs: BTreeSet<TxPriorityKey>,
    /// Hash to eligible transaction mapping for O(1) lookup
    tx_lookup: HashMap<String, PendingTx>,
    /// Deferred (future-nonce) transactions: global priority index for eviction
    deferred_index: BTreeSet<TxPriorityKey>,
    /// Hash to deferred transaction mapping for O(1) lookup
    deferred_lookup: HashMap<String, PendingTx>,
    /// Per-sender deferred map to promote next-ready nonces quickly
    deferred_by_sender: HashMap<String, BTreeMap<u64, String>>, // sender -> nonce -> hash
    /// Hash set for O(1) duplicate detection across eligible+deferred
    tx_hashes: HashSet<String>,
    /// Total size in bytes of all transactions (eligible + deferred)
    total_bytes: usize,
    /// Per-sender count of eligible (ready) transactions, used to compute expected nonce fast
    eligible_by_sender: HashMap<String, usize>,
    /// Per-sender reserved balances per denomination (amount + fee + gas_cost) across eligible + deferred
    reserved_by_sender: HashMap<String, HashMap<String, u128>>,
}

impl Mempool {
    pub fn new() -> Self {
        Self::with_config(MempoolConfig::default())
    }

    pub fn with_config(config: MempoolConfig) -> Self {
        // Use a policy that allows all PQC algorithms for testing/development
        use dytallix_node::policy::signature_policy::SignaturePolicy;
        let policy_manager = PolicyManager::new(SignaturePolicy::allow_all_pqc());
        
        Self {
            config,
            ordered_txs: BTreeSet::new(),
            tx_lookup: HashMap::new(),
            deferred_index: BTreeSet::new(),
            deferred_lookup: HashMap::new(),
            deferred_by_sender: HashMap::new(),
            tx_hashes: HashSet::new(),
            total_bytes: 0,
            policy_manager,
            eligible_by_sender: HashMap::new(),
            reserved_by_sender: HashMap::new(),
        }
    }

    /// Compute how many pending (eligible) transactions exist for a given sender.
    /// This is used to derive the next expected nonce for that sender as
    /// state_nonce + pending_count, ensuring sequential, gap-free promotion.
    fn pending_count_for_sender(&self, sender: &str) -> usize {
        *self.eligible_by_sender.get(sender).unwrap_or(&0)
    }

    /// Helper to compute a transaction's reserved contribution per denomination.
    /// Always accounts for fees/gas in `udgt` and message amounts in their respective denoms.
    fn reserved_amounts_for_tx(tx: &Transaction) -> HashMap<String, u128> {
        let mut required: HashMap<String, u128> = HashMap::new();

        let mut add = |denom: &str, amount: u128| {
            if amount == 0 {
                return;
            }
            let denom_key = denom.to_ascii_lowercase();
            let entry = required.entry(denom_key).or_insert(0);
            *entry = (*entry).saturating_add(amount);
        };

        // Fees and gas are always denominated in udgt for now.
        let gas_cost = (tx.gas_limit as u128) * (tx.gas_price as u128);
        add("udgt", tx.fee.saturating_add(gas_cost));

        if let Some(messages) = &tx.messages {
            for message in messages {
                match message {
                    TxMessage::Send { denom, amount, .. } => add(denom, *amount),
                    TxMessage::Data { .. } => {
                        // Data messages don't require token reserves, only fee payment
                        // which is already handled above
                    }
                    TxMessage::DmsRegister { .. } | TxMessage::DmsPing { .. } | TxMessage::DmsClaim { .. } => {
                        // DMS messages don't require token reserves from sender, only fee
                    }
                }
            }
        } else {
            // Legacy fallback: use top-level amount/denom fields
            add(&tx.denom, tx.amount);
        }

        required
    }

    fn add_reserved_amounts(
        reserved: &mut HashMap<String, HashMap<String, u128>>,
        sender: &str,
        delta: &HashMap<String, u128>,
    ) {
        if delta.is_empty() {
            return;
        }
        let entry = reserved.entry(sender.to_string()).or_insert_with(HashMap::new);
        for (denom, amount) in delta {
            if *amount == 0 {
                continue;
            }
            let slot = entry.entry(denom.clone()).or_insert(0);
            *slot = (*slot).saturating_add(*amount);
        }
    }

    fn subtract_reserved_amounts(
        reserved: &mut HashMap<String, HashMap<String, u128>>,
        sender: &str,
        delta: &HashMap<String, u128>,
    ) {
        if let Some(entry) = reserved.get_mut(sender) {
            let mut remove_keys: Vec<String> = Vec::new();
            for (denom, amount) in delta {
                if let Some(current) = entry.get_mut(denom) {
                    if *amount >= *current {
                        remove_keys.push(denom.clone());
                    } else {
                        *current = current.saturating_sub(*amount);
                    }
                }
            }
            for denom in remove_keys {
                entry.remove(&denom);
            }
            if entry.is_empty() {
                reserved.remove(sender);
            }
        }
    }

    /// Total count of transactions (eligible + deferred)
    fn total_count(&self) -> usize {
        self.tx_lookup.len() + self.deferred_lookup.len()
    }

    /// Add transaction to mempool with full validation
    pub fn add_transaction(
        &mut self,
        state: &State,
        tx: Transaction,
    ) -> Result<(), RejectionReason> {
        self.add_transaction_internal(state, tx, false)
    }

    /// Add a transaction when the caller has already performed signature validation.
    pub fn add_transaction_trusted(
        &mut self,
        state: &State,
        tx: Transaction,
    ) -> Result<(), RejectionReason> {
        self.add_transaction_internal(state, tx, true)
    }

    fn add_transaction_internal(
        &mut self,
        state: &State,
        tx: Transaction,
        skip_signature: bool,
    ) -> Result<(), RejectionReason> {
        // 1. Signature verification (unless caller opted out after verifying upstream)
        if !skip_signature && !verify_envelope(&tx) {
            return Err(RejectionReason::InvalidSignature);
        }

        // 1.5. Policy enforcement - validate signature algorithm if policy is configured
        if let Err(policy_error) = self.validate_signature_policy(&tx) {
            return Err(RejectionReason::PolicyViolation(format!("{policy_error}")));
        }

        // 2. Duplicate check (across eligible+deferred)
        if self.tx_hashes.contains(&tx.hash) {
            return Err(RejectionReason::Duplicate(tx.hash));
        }

        // 3. Size check
        let tx_size = estimate_tx_size(&tx);
        if tx_size > self.config.max_tx_bytes {
            return Err(RejectionReason::OversizedTx {
                max: self.config.max_tx_bytes,
                got: tx_size,
            });
        }

        // 4. Gas price check
        if tx.gas_price < self.config.min_gas_price {
            return Err(RejectionReason::UnderpricedGas {
                min: self.config.min_gas_price,
                got: tx.gas_price,
            });
        }

        // 5. Nonce and balance validation (aware of pending state)
        // Note: we allow future nonces; they are deferred until gap-free
        // but balance must still cover all reserved amounts including deferred.
        self.validate_tx_funds_only(state, &tx)?;

        let pending_tx = PendingTx::new(tx.clone());

        // Determine expected nonce from on-chain state + eligible pending count
        let state_nonce = state.snapshot_nonce(&tx.from);
        let expected = state_nonce + self.pending_count_for_sender(&tx.from) as u64;

        if tx.nonce < expected {
            return Err(RejectionReason::NonceGap {
                expected,
                got: tx.nonce,
            });
        }

        // Ensure capacity before inserting (counts all)
        self.ensure_capacity_for(&pending_tx)?;

        let reserved_delta = Self::reserved_amounts_for_tx(&tx);
        if tx.nonce == expected {
            // Eligible now: track reserved and eligible count
            Self::add_reserved_amounts(&mut self.reserved_by_sender, &tx.from, &reserved_delta);
            self.insert_eligible(pending_tx);
            // Try to promote any deferred txs for this sender
            self.try_promote_deferred(&tx.from, state);
        } else {
            // Future nonce -> defer (tracks reserved internally)
            self.insert_deferred(pending_tx);
        }

        Ok(())
    }

    /// Validate only funds and gas (nonce handled separately to allow deferral)
    fn validate_tx_funds_only(
        &self,
        state: &State,
        tx: &Transaction,
    ) -> Result<(), RejectionReason> {
        let account = state.snapshot_account(&tx.from);
        let existing_reserved = self.reserved_by_sender.get(&tx.from);
        let required = Self::reserved_amounts_for_tx(tx);

        for (denom, needed_now) in required.iter() {
            let already_reserved = existing_reserved
                .and_then(|m| m.get(denom))
                .copied()
                .unwrap_or(0);
            let total_needed = already_reserved.saturating_add(*needed_now);
            let available = account.balance_of(denom);

            if available < total_needed {
                eprintln!(
                    "WARN  [Mempool] Rejecting tx from {} (insufficient {} balance: {} < {})",
                    &tx.from[..12],
                    denom,
                    available,
                    total_needed
                );
                return Err(RejectionReason::InsufficientFunds {
                    denom: denom.clone(),
                    required: total_needed,
                    available,
                });
            }
        }

        // Gas validation
        if tx.gas_limit > 0 || tx.gas_price > 0 {
            validate_gas(tx).map_err(RejectionReason::InternalError)?;
        }

        Ok(())
    }

    /// Insert an eligible transaction into main structures
    fn insert_eligible(&mut self, pending_tx: PendingTx) {
        let key = pending_tx.priority_key();
        self.total_bytes += pending_tx.serialized_size;
        self.tx_hashes.insert(pending_tx.tx.hash.clone());
        self.ordered_txs.insert(key);
        *self
            .eligible_by_sender
            .entry(pending_tx.tx.from.clone())
            .or_default() += 1;
        self.tx_lookup
            .insert(pending_tx.tx.hash.clone(), pending_tx);
    }

    /// Insert a deferred (future-nonce) transaction into deferred structures
    fn insert_deferred(&mut self, pending_tx: PendingTx) {
        let key = pending_tx.priority_key();
        self.total_bytes += pending_tx.serialized_size;
        self.tx_hashes.insert(pending_tx.tx.hash.clone());
        self.deferred_index.insert(key.clone());
        self.deferred_by_sender
            .entry(pending_tx.tx.from.clone())
            .or_default()
            .entry(pending_tx.tx.nonce)
            .or_default();
        // Track reserved value for deferred txs as well
        let delta = Self::reserved_amounts_for_tx(&pending_tx.tx);
        Self::add_reserved_amounts(&mut self.reserved_by_sender, &pending_tx.tx.from, &delta);
        self.deferred_lookup
            .insert(pending_tx.tx.hash.clone(), pending_tx);
    }

    /// Attempt to promote deferred transactions for a sender if sequentially ready
    fn try_promote_deferred(&mut self, sender: &str, state: &State) {
        loop {
            let expected = {
                let state_nonce = state.snapshot_nonce(sender);
                state_nonce + self.pending_count_for_sender(sender) as u64
            };

            let next_hash_opt = self
                .deferred_by_sender
                .get_mut(sender)
                .and_then(|m| m.remove(&expected));

            let Some(next_hash) = next_hash_opt else {
                break;
            };

            if let Some(pending) = self.deferred_lookup.remove(&next_hash) {
                // Remove from deferred index
                let key = pending.priority_key();
                self.deferred_index.remove(&key);
                // Adjust bytes to avoid double counting when moving from deferred -> eligible
                self.total_bytes = self.total_bytes.saturating_sub(pending.serialized_size);
                // Insert into eligible, ensuring capacity (may evict others)
                self.ensure_capacity_for(&pending).ok();
                self.insert_eligible(pending);
            }
        }

        // Clean up empty sender map
        if let Some(map) = self.deferred_by_sender.get(sender) {
            if map.is_empty() {
                self.deferred_by_sender.remove(sender);
            }
        }
    }

    /// Internal helper: promote deferred txs for `sender` starting from `expected` nonce
    /// without requiring State. This is used after transactions are dropped due to inclusion
    /// in a block, where chain state nonce has advanced accordingly.
    fn promote_deferred_from_expected(&mut self, sender: &str, mut expected: u64) {
        loop {
            let next_hash_opt = self
                .deferred_by_sender
                .get_mut(sender)
                .and_then(|m| m.remove(&expected));

            let Some(next_hash) = next_hash_opt else {
                break;
            };

            if let Some(pending) = self.deferred_lookup.remove(&next_hash) {
                // Remove from deferred index and adjust bytes (move semantics)
                let key = pending.priority_key();
                self.deferred_index.remove(&key);
                self.total_bytes = self.total_bytes.saturating_sub(pending.serialized_size);
                // Insert into eligible (may evict others)
                self.ensure_capacity_for(&pending).ok();
                self.insert_eligible(pending);
                expected = expected.saturating_add(1);
            }
        }

        // Clean up empty sender map
        if let Some(map) = self.deferred_by_sender.get(sender) {
            if map.is_empty() {
                self.deferred_by_sender.remove(sender);
            }
        }
    }

    /// Ensure capacity by evicting lowest priority transactions (eligible or deferred) if needed
    fn ensure_capacity_for(&mut self, new_tx: &PendingTx) -> Result<(), RejectionReason> {
        // If the incoming tx itself exceeds the byte budget, reject early
        if new_tx.serialized_size > self.config.max_bytes {
            return Err(RejectionReason::OversizedTx {
                max: self.config.max_bytes,
                got: new_tx.serialized_size,
            });
        }

        // Evict by count
        while self.total_count() >= self.config.max_txs {
            self.evict_lowest_priority()?;
            if self.total_count() == 0 {
                break;
            }
        }

        // Evict by bytes
        while self.total_bytes + new_tx.serialized_size > self.config.max_bytes {
            self.evict_lowest_priority()?;
            if self.total_count() == 0 {
                return Err(RejectionReason::OversizedTx {
                    max: self.config.max_bytes,
                    got: self.total_bytes + new_tx.serialized_size,
                });
            }
        }

        Ok(())
    }

    /// Evict the lowest priority transaction across eligible and deferred
    fn evict_lowest_priority(&mut self) -> Result<(), RejectionReason> {
        // Get lowest from eligible
        let eligible_low = self.ordered_txs.iter().last().cloned();
        // Get lowest from deferred
        let deferred_low = self.deferred_index.iter().last().cloned();

        // Choose the lower priority among the two (max key)
        let choice = match (eligible_low, deferred_low) {
            (Some(e), Some(d)) => {
                if e >= d {
                    Some((e, true))
                } else {
                    Some((d, false))
                }
            }
            (Some(e), None) => Some((e, true)),
            (None, Some(d)) => Some((d, false)),
            (None, None) => None,
        };

        if let Some((key, is_eligible)) = choice {
            if is_eligible {
                self.ordered_txs.remove(&key);
                if let Some(tx) = self.tx_lookup.remove(&key.hash) {
                    self.tx_hashes.remove(&key.hash);
                    self.total_bytes = self.total_bytes.saturating_sub(tx.serialized_size);
                    // Update per-sender trackers
                    if let Some(count) = self.eligible_by_sender.get_mut(&tx.tx.from) {
                        *count = count.saturating_sub(1);
                        if *count == 0 {
                            self.eligible_by_sender.remove(&tx.tx.from);
                        }
                    }
                    let delta = Self::reserved_amounts_for_tx(&tx.tx);
                    Self::subtract_reserved_amounts(
                        &mut self.reserved_by_sender,
                        &tx.tx.from,
                        &delta,
                    );
                    log::info!("Evicted transaction {} due to capacity", key.hash);
                }
            } else {
                self.deferred_index.remove(&key);
                if let Some(tx) = self.deferred_lookup.remove(&key.hash) {
                    // Remove from per-sender map
                    if let Some(map) = self.deferred_by_sender.get_mut(&tx.tx.from) {
                        map.remove(&tx.tx.nonce);
                        if map.is_empty() {
                            self.deferred_by_sender.remove(&tx.tx.from);
                        }
                    }
                    self.tx_hashes.remove(&key.hash);
                    self.total_bytes = self.total_bytes.saturating_sub(tx.serialized_size);
                    let delta = Self::reserved_amounts_for_tx(&tx.tx);
                    Self::subtract_reserved_amounts(
                        &mut self.reserved_by_sender,
                        &tx.tx.from,
                        &delta,
                    );
                    log::info!("Evicted deferred transaction {} due to capacity", key.hash);
                }
            }
        }
        Ok(())
    }

    /// Get up to n highest priority transactions for block creation
    pub fn take_snapshot(&self, n: usize) -> Vec<Transaction> {
        self.ordered_txs
            .iter()
            .take(n)
            .filter_map(|key| self.tx_lookup.get(&key.hash))
            .map(|pending| pending.tx.clone())
            .collect()
    }

    /// Remove transactions by hash (after inclusion in block)
    pub fn drop_hashes(&mut self, hashes: &[String]) {
        use std::cmp::max;
        // Track the highest included nonce per sender for correct promotion
        let mut removed_max_nonce: HashMap<String, u64> = HashMap::new();

        for hash in hashes {
            if let Some(pending_tx) = self.tx_lookup.remove(hash) {
                let priority_key = pending_tx.priority_key();
                self.ordered_txs.remove(&priority_key);
                self.tx_hashes.remove(hash);
                self.total_bytes = self.total_bytes.saturating_sub(pending_tx.serialized_size);

                // Update trackers
                if let Some(count) = self.eligible_by_sender.get_mut(&pending_tx.tx.from) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        self.eligible_by_sender.remove(&pending_tx.tx.from);
                    }
                }
                let delta = Self::reserved_amounts_for_tx(&pending_tx.tx);
                Self::subtract_reserved_amounts(
                    &mut self.reserved_by_sender,
                    &pending_tx.tx.from,
                    &delta,
                );

                // Record removed nonce per sender (eligible inclusion)
                let sender = pending_tx.tx.from.clone();
                let nonce = pending_tx.tx.nonce;
                removed_max_nonce
                    .entry(sender)
                    .and_modify(|m| *m = max(*m, nonce))
                    .or_insert(nonce);
            } else if let Some(deferred_tx) = self.deferred_lookup.remove(hash) {
                // Remove from deferred structures
                let key = deferred_tx.priority_key();
                self.deferred_index.remove(&key);
                if let Some(map) = self.deferred_by_sender.get_mut(&deferred_tx.tx.from) {
                    map.remove(&deferred_tx.tx.nonce);
                    if map.is_empty() {
                        self.deferred_by_sender.remove(&deferred_tx.tx.from);
                    }
                }
                self.tx_hashes.remove(hash);
                self.total_bytes = self.total_bytes.saturating_sub(deferred_tx.serialized_size);
                // Update reserved totals
                let delta = Self::reserved_amounts_for_tx(&deferred_tx.tx);
                Self::subtract_reserved_amounts(
                    &mut self.reserved_by_sender,
                    &deferred_tx.tx.from,
                    &delta,
                );
            }
        }

        // After removals, attempt to promote deferred txs for affected senders using
        // an estimated expected nonce based on remaining eligible txs and removed max nonce.
        for (sender, removed_max) in removed_max_nonce.into_iter() {
            let current_max_eligible = self
                .tx_lookup
                .values()
                .filter(|p| p.tx.from == sender)
                .map(|p| p.tx.nonce)
                .max();

            let expected = match current_max_eligible {
                Some(max_nonce) => max(max_nonce, removed_max).saturating_add(1),
                None => removed_max.saturating_add(1),
            };

            self.promote_deferred_from_expected(&sender, expected);
        }
    }

    /// Check if transaction exists in mempool
    pub fn contains(&self, hash: &str) -> bool {
        self.tx_hashes.contains(hash)
    }

    /// Get current mempool statistics (eligible only)
    pub fn len(&self) -> usize {
        self.tx_lookup.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn is_full(&self) -> bool {
        self.total_count() >= self.config.max_txs || self.total_bytes >= self.config.max_bytes
    }

    /// Get current minimum gas price in the pool (eligible only)
    pub fn current_min_gas_price(&self) -> u64 {
        // Find the lowest gas price in the pool (last eligible transaction)
        self.ordered_txs
            .iter()
            .last()
            .and_then(|key| self.tx_lookup.get(&key.hash))
            .map(|tx| tx.tx.gas_price)
            .unwrap_or(self.config.min_gas_price)
    }

    /// Get pool configuration
    pub fn config(&self) -> &MempoolConfig {
        &self.config
    }

    /// Push a transaction into the mempool (RPC method)
    pub fn push(&mut self, tx: Transaction) -> Result<(), RejectionReason> {
        self.add_transaction(&State::default(), tx)
    }
}

/// Legacy error type for backward compatibility
#[derive(Debug)]
pub enum MempoolError {
    Duplicate,
    Full,
    Rejection(RejectionReason),
}

impl From<RejectionReason> for MempoolError {
    fn from(reason: RejectionReason) -> Self {
        match reason {
            RejectionReason::Duplicate(_) => MempoolError::Duplicate,
            reason => MempoolError::Rejection(reason),
        }
    }
}

impl std::fmt::Display for MempoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MempoolError::Duplicate => write!(f, "duplicate"),
            MempoolError::Full => write!(f, "full"),
            MempoolError::Rejection(reason) => write!(f, "{reason}"),
        }
    }
}

impl std::error::Error for MempoolError {}

/// Verify transaction envelope (signature validation)
pub(crate) fn verify_envelope(tx: &Transaction) -> bool {
    match (&tx.signature, &tx.public_key) {
        (Some(signature), Some(public_key)) => {
            // Perform real PQC signature verification
            match verify_pqc_signature(tx, signature, public_key) {
                Ok(()) => true,
                Err(_) => false,
            }
        }
        _ => false, // No signature or public key provided
    }
}

/// Verify PQC signature for a transaction
fn verify_pqc_signature(tx: &Transaction, signature: &str, public_key: &str) -> Result<(), String> {
    // 1. Decode base64 signature and public key
    let sig_bytes = B64
        .decode(signature)
        .map_err(|e| format!("invalid signature encoding: {e}"))?;
    let pk_bytes = B64
        .decode(public_key)
        .map_err(|e| format!("invalid public key encoding: {e}"))?;

    tracing::info!(
        "PQC verification: pk_len={}, sig_len={}, from={:?}",
        pk_bytes.len(),
        sig_bytes.len(),
        tx.from
    );

    // 2. Create canonical transaction for signing
    let canonical_tx = tx.canonical_fields();

    // 3. Serialize to canonical JSON
    let tx_bytes = canonical_json(&canonical_tx)
        .map_err(|e| format!("failed to serialize transaction: {e}"))?;

    tracing::debug!("Canonical JSON: {}", String::from_utf8_lossy(&tx_bytes));

    // 4. Hash with SHA3-256
    let tx_hash = sha3_256(&tx_bytes);

    tracing::info!("Transaction hash: {}", hex::encode(&tx_hash));

    // 5. Verify signature using new multi-algorithm verification
    // For mempool transactions, we use the default algorithm (Dilithium5)
    // as the Transaction struct doesn't include algorithm field
    match crate::crypto::pqc_verify::verify(
        &pk_bytes,
        &tx_hash,
        &sig_bytes,
        PQCAlgorithm::default(),
    ) {
        Ok(()) => {
            tracing::info!("✅ Transaction signature verification successful");
            Ok(())
        }
        Err(PQCVerifyError::UnsupportedAlgorithm(alg)) => {
            tracing::error!("Unsupported PQC algorithm: {}", alg);
            Err(format!("unsupported algorithm: {alg}"))
        }
        Err(PQCVerifyError::InvalidPublicKey { algorithm, details }) => {
            tracing::error!("Invalid public key for {}: {}", algorithm, details);
            Err(format!("invalid public key: {details}"))
        }
        Err(PQCVerifyError::InvalidSignature { algorithm, details }) => {
            tracing::error!("Invalid signature for {}: {}", algorithm, details);
            Err(format!("invalid signature: {details}"))
        }
        Err(PQCVerifyError::VerificationFailed { algorithm }) => {
            tracing::warn!("❌ Signature verification failed for algorithm: {}", algorithm);
            Err("signature verification failed".to_string())
        }
        Err(PQCVerifyError::FeatureNotCompiled { feature }) => {
            tracing::error!("PQC feature not compiled: {}", feature);
            Err(format!("feature not available: {feature}"))
        }
    }
}

/// Enhanced validation including gas validation (legacy function for backward compatibility)
pub fn basic_validate(state: &State, tx: &Transaction) -> Result<(), String> {
    let mempool = Mempool::new();
    match mempool.validate_tx_funds_only(state, tx) {
        Ok(()) => Ok(()),
        Err(reason) => Err(reason.to_string()),
    }
}

/// Gas validation function with enhanced error reporting
fn validate_gas(tx: &Transaction) -> Result<(), String> {
    let schedule = GasSchedule::default();

    // For now, assume all transactions are transfers
    // This will be extended when we have better transaction type detection
    let tx_kind = TxKind::Transfer;

    // Estimate transaction size (approximation for now)
    let tx_size_bytes = estimate_tx_size(tx);
    let additional_signatures = 0; // Single signature for now

    // Validate gas limit against intrinsic requirements
    validate_gas_limit(
        &tx_kind,
        tx_size_bytes,
        additional_signatures,
        tx.gas_limit,
        &schedule,
    )
    .map_err(|e| format!("GasValidationError: {e}"))?;

    // Check gas price is reasonable (non-zero)
    if tx.gas_price == 0 {
        return Err("GasValidationError: gas price cannot be zero".to_string());
    }

    Ok(())
}

/// Estimate transaction size for gas calculation and size limits
fn estimate_tx_size(tx: &Transaction) -> usize {
    // Cap the signature contribution to avoid pathological sizes from different
    // signature schemes (e.g., PQC). This keeps mempool accounting predictable.
    const MAX_SIG_ACCOUNTING: usize = 256;

    let sig_len = tx
        .signature
        .as_ref()
        .map_or(0, |s| s.len().min(MAX_SIG_ACCOUNTING));

    // Rough estimate based on serialized fields
    // This should be more precise in production
    tx.hash.len() +
    tx.from.len() +
    tx.to.len() +
    16 + // amount (u128)
    16 + // fee (u128)
    8 +  // nonce (u64)
    sig_len +
    8 +  // gas_limit (u64)
    8 // gas_price (u64)
}

impl Mempool {
    /// Validate transaction signature algorithm against policy
    fn validate_signature_policy(&self, tx: &Transaction) -> Result<(), PolicyError> {
        if let Some(alg) = tx.signature_algorithm() {
            if self.policy_manager.policy().should_enforce_at_mempool() {
                // All transactions currently use Dilithium5
                // The algorithm is already in dytallix_pqc::SignatureAlgorithm format
                self.policy_manager.validate_transaction_algorithm(&alg)?;
            }
        }
        Ok(())
    }
}

impl Default for Mempool {
    fn default() -> Self {
        Self::new()
    }
}
