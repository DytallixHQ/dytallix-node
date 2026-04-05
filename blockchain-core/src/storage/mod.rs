use crate::types::Amount as Tokens;
use crate::types::{
    AccountState, Address, Amount, Block, BlockNumber, Timestamp, Transaction,
    Transaction as TxEnum, TxReceipt,
};
use rocksdb::{Options, DB};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Transaction receipt persisted for lookup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionReceipt {
    pub hash: String,
    pub status: String, // success | failed
    pub block_number: BlockNumber,
    pub index: u32,
    pub fee: Amount,
    pub from: Address,
    pub to: Option<Address>,
    pub amount: Option<Amount>,
    pub nonce: u64,
    pub error: Option<String>,
}

/// Smart Contract State Storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractState {
    pub code: Vec<u8>,
    pub storage: HashMap<Vec<u8>, Vec<u8>>, // removed stray backslash
    pub balance: Amount,
    pub metadata: ContractMetadata,
}

/// Smart Contract Metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetadata {
    pub deployer: Address,
    pub deployment_block: BlockNumber,
    pub last_modified: Timestamp,
    pub call_count: u64,
}

impl ContractState {
    pub fn _new(
        code: Vec<u8>,
        deployer: Address,
        deployment_block: BlockNumber,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            code,
            storage: HashMap::new(),
            balance: 0,
            metadata: ContractMetadata {
                deployer,
                deployment_block,
                last_modified: timestamp,
                call_count: 0,
            },
        }
    }
    pub fn _set_storage(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.storage.insert(key, value);
    }
    pub fn _get_storage(&self, key: &[u8]) -> Option<&Vec<u8>> {
        self.storage.get(key)
    }
    pub fn _increment_calls(&mut self) {
        self.metadata.call_count += 1;
    }
    pub fn _update_timestamp(&mut self, timestamp: Timestamp) {
        self.metadata.last_modified = timestamp;
    }
}

/// Persistent storage manager (RocksDB)
#[derive(Debug)]
pub struct StorageManager {
    db: Arc<DB>,
    // lightweight in-memory cache for hot account states (optional)
    _account_cache: Arc<RwLock<HashMap<Address, AccountState>>>, // underscore
}

const META_CHAIN_ID: &str = "meta:chain_id";
const META_HEIGHT: &str = "meta:height";
const META_BEST_HASH: &str = "meta:best_hash";

pub type _KVSnapshotResult = Result<Vec<(Vec<u8>, Vec<u8>)>, Box<dyn std::error::Error>>;

impl StorageManager {
    /// Open or create storage at data_dir. If empty, will initialize genesis (balances)
    pub async fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Fallback to env path if provided
        let data_dir = std::env::var("DYT_DATA_DIR").unwrap_or_else(|_| "./data".to_string());
        let chain_id = std::env::var("DYT_CHAIN_ID").unwrap_or_else(|_| "dyt-local-1".to_string());
        std::fs::create_dir_all(&data_dir)?;
        let mut opts = Options::default();
        opts.create_if_missing(true);
        let db_path = Path::new(&data_dir).join("node.db");
        let db = DB::open(&opts, db_path)?;
        let mgr = Self {
            db: Arc::new(db),
            _account_cache: Arc::new(RwLock::new(HashMap::new())),
        };
        mgr.ensure_chain_id(&chain_id)?;
        // If height not set treat as fresh and init genesis
        if mgr.get_height()? == 0 {
            mgr.init_genesis(&chain_id).await?;
        }
        Ok(mgr)
    }

    fn ensure_chain_id(&self, expected: &str) -> Result<(), Box<dyn std::error::Error>> {
        match self.db.get(META_CHAIN_ID.as_bytes())? {
            Some(stored) => {
                let stored_str = String::from_utf8(stored)?;
                if stored_str != expected {
                    return Err(format!(
                        "Chain ID mismatch: existing {stored_str} expected {expected}"
                    )
                    .into());
                }
            }
            None => {
                self.db.put(META_CHAIN_ID, expected)?;
            }
        }
        Ok(())
    }

    async fn init_genesis(&self, _chain_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        // Load balances from genesis file if provided
        let genesis_path =
            std::env::var("DYT_GENESIS_FILE").unwrap_or_else(|_| "genesisBlock.json".to_string());
        if Path::new(&genesis_path).exists() {
            if let Ok(text) = std::fs::read_to_string(&genesis_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    // Accept either dgt_allocations or allocations arrays; only load dyt1* addresses
                    if let Some(arr) = json.get("dgt_allocations").and_then(|v| v.as_array()) {
                        for entry in arr {
                            if let (Some(addr), Some(amount)) = (
                                entry.get("address").and_then(|v| v.as_str()),
                                entry.get("amount").and_then(|v| v.as_u64()),
                            ) {
                                if addr.starts_with("dyt1") {
                                    let acct = AccountState {
                                        balance: amount as u128,
                                        ..Default::default()
                                    };
                                    self.store_account_state(addr, &acct)?;
                                }
                            }
                        }
                    }
                }
            }
        }
        // Initialize metadata height=0 best_hash=0*64
        self.db.put(META_HEIGHT, 0u64.to_be_bytes())?;
        self.db.put(META_BEST_HASH, vec![b'0'; 64])?;
        Ok(())
    }

    fn _set_height(&self, h: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.db.put(META_HEIGHT, h.to_be_bytes())?;
        Ok(())
    }
    pub fn get_height(&self) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(self
            .db
            .get(META_HEIGHT)?
            .map(|b| {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&b);
                u64::from_be_bytes(arr)
            })
            .unwrap_or(0))
    }
    fn _set_best_hash(&self, hash: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.db.put(META_BEST_HASH, hash.as_bytes())?;
        Ok(())
    }
    pub fn _get_best_hash(&self) -> Result<String, Box<dyn std::error::Error>> {
        Ok(self
            .db
            .get(META_BEST_HASH)?
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_else(|| "0".repeat(64)))
    }

    fn account_key(address: &str) -> String {
        format!("acct:{address}")
    }
    fn block_hash_key(hash: &str) -> String {
        format!("blk_hash:{hash}")
    }
    fn block_num_key(num: u64) -> String {
        format!("blk_num:{num:016x}")
    }
    fn tx_key(hash: &str) -> String {
        format!("tx:{hash}")
    }
    fn rcpt_key(hash: &str) -> String {
        format!("rcpt:{hash}")
    }
    fn _contract_key(address: &str) -> String {
        format!("contract:{address}")
    }
    fn receipt_key(hash: &str) -> String {
        format!("receipt:{hash}")
    }

    pub fn store_account_state(
        &self,
        address: &str,
        state: &AccountState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let enc = bincode::serialize(state)?;
        self.db.put(Self::account_key(address), enc)?;
        Ok(())
    }
    pub fn get_account_state(
        &self,
        address: &str,
    ) -> Result<AccountState, Box<dyn std::error::Error>> {
        if let Some(v) = self.db.get(Self::account_key(address))? {
            Ok(bincode::deserialize(&v)?)
        } else {
            Ok(AccountState::default())
        }
    }

    /// External API: Get address balance
    pub async fn get_address_balance(
        &self,
        address: &str,
    ) -> Result<Tokens, Box<dyn std::error::Error>> {
        Ok(self.get_account_state(address)?.balance)
    }
    /// External API: Set (overwrite) address balance (used only in tests / genesis)
    pub async fn _set_address_balance(
        &self,
        address: &str,
        balance: Tokens,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut st = self.get_account_state(address)?;
        st.balance = balance;
        self.store_account_state(address, &st)?;
        Ok(())
    }
    pub async fn get_address_nonce(
        &self,
        address: &str,
    ) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(self.get_account_state(address)?.nonce)
    }

    /// Apply a transfer inclusion (validates nonce & balances) and mutate state
    pub fn _apply_transfer(&self, tx: &crate::types::TransferTransaction) -> Result<(), String> {
        let mut sender = self
            .get_account_state(&tx.from)
            .map_err(|e| e.to_string())?;
        let mut recipient = self.get_account_state(&tx.to).map_err(|e| e.to_string())?;
        if sender.nonce != tx.nonce {
            return Err("nonce_mismatch".into());
        }
        let total = tx.amount.checked_add(tx.fee).ok_or("overflow")?;
        if sender.balance < total {
            return Err("insufficient_balance".into());
        }
        sender.balance -= total;
        sender.nonce += 1;
        recipient.balance = recipient.balance.saturating_add(tx.amount);
        self.store_account_state(&tx.from, &sender)
            .map_err(|e| e.to_string())?;
        self.store_account_state(&tx.to, &recipient)
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Store block + its transactions + receipts, update metadata
    pub fn _store_block(&self, block: &Block) -> Result<(), Box<dyn std::error::Error>> {
        let hash = block.hash();
        let height = block.header.number;
        // Store each transaction & receipt placeholder (success assumed if already applied)
        for (idx, tx) in block.transactions.iter().enumerate() {
            let tx_hash = tx.hash();
            if self.db.get(Self::tx_key(&tx_hash))?.is_none() {
                self.db
                    .put(Self::tx_key(&tx_hash), bincode::serialize(tx)?)?;
            }
            if self.db.get(Self::receipt_key(&tx_hash))?.is_none() {
                let (from, to, amount, fee, nonce) = match tx {
                    TxEnum::Transfer(t) => (
                        t.from.clone(),
                        Some(t.to.clone()),
                        Some(t.amount),
                        t.fee,
                        t.nonce,
                    ),
                    _ => (tx.from().clone(), None, None, tx.fee(), tx.nonce()),
                };
                let receipt = TransactionReceipt {
                    hash: tx_hash.clone(),
                    status: "success".into(),
                    block_number: height,
                    index: idx as u32,
                    fee,
                    from,
                    to,
                    amount,
                    nonce,
                    error: None,
                };
                self.db
                    .put(Self::receipt_key(&tx_hash), bincode::serialize(&receipt)?)?;
            }
        }
        self.db
            .put(Self::block_hash_key(&hash), bincode::serialize(block)?)?;
        self.db.put(Self::block_num_key(height), hash.as_bytes())?;
        self._set_height(height)?;
        self._set_best_hash(&hash)?;
        Ok(())
    }

    pub async fn get_block_by_height(
        &self,
        height: u64,
    ) -> Result<Option<Block>, Box<dyn std::error::Error>> {
        if let Some(hbytes) = self.db.get(Self::block_num_key(height))? {
            let hash = String::from_utf8(hbytes)?;
            self.get_block_by_hash(&hash).await
        } else {
            Ok(None)
        }
    }
    pub async fn get_block_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<Block>, Box<dyn std::error::Error>> {
        if let Some(raw) = self.db.get(Self::block_hash_key(hash))? {
            let blk: Block = bincode::deserialize(&raw)?;
            Ok(Some(blk))
        } else {
            Ok(None)
        }
    }

    pub async fn list_blocks_desc(
        &self,
        limit: usize,
        from: Option<u64>,
    ) -> Result<Vec<Block>, Box<dyn std::error::Error>> {
        let current = from.unwrap_or(self.get_height()?);
        let mut out = Vec::new();
        let mut h = current;
        loop {
            if out.len() >= limit {
                break;
            }
            if let Some(b) = self.get_block_by_height(h).await? {
                out.push(b);
            }
            if h == 0 {
                break;
            }
            h -= 1; // safe because we break at 0
        }
        Ok(out)
    }

    pub async fn get_transaction_by_hash(
        &self,
        hash: &str,
    ) -> Result<Option<Transaction>, Box<dyn std::error::Error>> {
        if let Some(raw) = self.db.get(Self::tx_key(hash))? {
            Ok(Some(bincode::deserialize(&raw)?))
        } else {
            Ok(None)
        }
    }
    pub async fn store_transaction(
        &self,
        tx: &Transaction,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let key = Self::tx_key(&tx.hash());
        if self.db.get(&key)?.is_none() {
            self.db.put(key, bincode::serialize(tx)?)?;
        }
        Ok(())
    }

    /// Store a transaction receipt under receipt:{hash}
    pub async fn _store_receipt(
        &self,
        receipt: &TxReceipt,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let key = Self::receipt_key(&receipt.tx_hash);
        self.db.put(key, bincode::serialize(receipt)?)?;
        Ok(())
    }

    /// Fetch a transaction receipt by tx hash (tries new prefix, falls back to legacy rcpt:)
    pub async fn get_receipt(
        &self,
        hash: &str,
    ) -> Result<Option<TxReceipt>, Box<dyn std::error::Error>> {
        if let Some(raw) = self.db.get(Self::receipt_key(hash))? {
            return Ok(Some(bincode::deserialize(&raw)?));
        }
        if let Some(raw) = self.db.get(Self::rcpt_key(hash))? {
            return Ok(Some(bincode::deserialize(&raw)?));
        }
        Ok(None)
    }

    /// Store a full contract state (code + storage + metadata)
    pub async fn _store_contract(
        &self,
        address: &str,
        state: &ContractState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let enc = bincode::serialize(state)?;
        self.db.put(Self::_contract_key(address), enc)?;
        Ok(())
    }

    /// Check whether a contract exists
    pub async fn _contract_exists(
        &self,
        address: &str,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(self.db.get(Self::_contract_key(address))?.is_some())
    }

    /// Retrieve a contract state
    pub async fn _get_contract(
        &self,
        address: &str,
    ) -> Result<Option<ContractState>, Box<dyn std::error::Error>> {
        if let Some(raw) = self.db.get(Self::_contract_key(address))? {
            Ok(Some(bincode::deserialize(&raw)?))
        } else {
            Ok(None)
        }
    }

    /// Generic put helper (used by runtime persistence)
    pub async fn _put(&self, key: &[u8], value: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        self.db.put(key, value)?;
        Ok(())
    }
    /// Generic get helper (used by runtime persistence)
    pub async fn _get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
        Ok(self.db.get(key)?)
    }

    /// Legacy compatibility (was in-memory). Now uses RocksDB key prefixes.
    pub async fn _clear(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Not efficient; for tests only.
        self.db.flush()?; // leave data (full deletion would require destroying DB)
        Ok(())
    }

    /// Destroy RocksDB at path for tests/dev only (closes and deletes underlying directory)
    pub fn _destroy_for_tests(path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        if Path::new(path).exists() {
            drop(opts); // ensure no open handles (caller must drop StorageManager before calling)
            rocksdb::DB::destroy(&Options::default(), Path::new(path).join("node.db"))?;
        }
        Ok(())
    }

    /// Snapshot all key/value pairs relevant for state root commitment.
    /// Currently includes all RocksDB entries except meta:* keys.
    pub fn _snapshot_kv(&self) -> _KVSnapshotResult {
        use rocksdb::IteratorMode;
        let mut out = Vec::new();
        let iter = self.db.iterator(IteratorMode::Start);
        for (k, v) in iter.flatten() {
            // Exclude meta & ephemeral prefixes from state commitment
            if k.starts_with(b"meta:") {
                continue;
            }
            if k.starts_with(b"rcpt:") {
                continue;
            } // legacy receipts (non-consensus)
            if k.starts_with(b"receipt:") {
                continue;
            } // volatile per-tx receipts
            if k.starts_with(b"tx:") {
                continue;
            } // mempool / tx objects
            out.push((k.to_vec(), v.to_vec()));
        }
        // Canonical sort lexicographically by key bytes
        out.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(out)
    }
    fn _receipt_height_index_key(height: u64, index: u32) -> String {
        format!("rcpi:{height:016x}:{index}")
    }
    fn _receipt_tx_lookup_key(tx_hash: &str) -> String {
        format!("rcpx:{tx_hash}")
    }

    /// Store receipts in new indexed form (height/index and tx_hash -> (height,index))
    pub fn _store_receipts_indexed(
        &self,
        height: u64,
        receipts: &[TxReceipt],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (i, r) in receipts.iter().enumerate() {
            let key_hi = Self::_receipt_height_index_key(height, i as u32);
            if self.db.get(&key_hi)?.is_none() {
                self.db.put(key_hi.as_bytes(), bincode::serialize(r)?)?;
            }
            let key_tx = Self::_receipt_tx_lookup_key(&r.tx_hash);
            if self.db.get(&key_tx)?.is_none() {
                self.db
                    .put(key_tx.as_bytes(), bincode::serialize(&(height, i as u32))?)?;
            }
            // Also keep backward compatibility single-key receipt
            let key = Self::receipt_key(&r.tx_hash);
            if self.db.get(&key)?.is_none() {
                self.db.put(key, bincode::serialize(r)?)?;
            }
        }
        Ok(())
    }

    /// Lookup receipt via index map then load
    pub fn _get_receipt_via_index(
        &self,
        tx_hash: &str,
    ) -> Result<Option<TxReceipt>, Box<dyn std::error::Error>> {
        if let Some(raw_idx) = self.db.get(Self::_receipt_tx_lookup_key(tx_hash))? {
            let (h, idx): (u64, u32) = bincode::deserialize(&raw_idx)?;
            let key_hi = Self::_receipt_height_index_key(h, idx);
            if let Some(raw_r) = self.db.get(key_hi)? {
                return Ok(Some(bincode::deserialize(&raw_r)?));
            }
        }
        // fallback to legacy direct storage (synchronous)
        if let Some(raw) = self.db.get(Self::receipt_key(tx_hash))? {
            return Ok(Some(bincode::deserialize(&raw)?));
        }
        if let Some(raw) = self.db.get(Self::rcpt_key(tx_hash))? {
            return Ok(Some(bincode::deserialize(&raw)?));
        }
        Ok(None)
    }
}

// --- Helper: build a simple block from transactions (used by background producer) ---
// Refactored: caller must supply precomputed state_root (from injected snapshot) and timestamp.
pub fn _build_block_with_state(
    parent_hash: String,
    number: u64,
    txs: Vec<Transaction>,
    validator: Address,
    state_root: String,
    timestamp: u64,
) -> Block {
    let transactions_root = crate::types::BlockHeader::calculate_transactions_root(&txs);
    let signature = crate::types::PQCBlockSignature {
        signature: dytallix_pqc::Signature {
            data: vec![],
            algorithm: dytallix_pqc::SignatureAlgorithm::Dilithium5,
        },
        public_key: vec![],
    };
    let header = crate::types::BlockHeader {
        number,
        parent_hash,
        transactions_root,
        state_root,
        timestamp,
        validator,
        signature,
        nonce: 0,
    };
    Block {
        header,
        transactions: txs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TxReceipt, TxStatus};

    #[tokio::test]
    async fn test_store_get_receipt_roundtrip() {
        let mgr = StorageManager::new().await.unwrap();
        let rcpt = TxReceipt {
            tx_hash: "0xdeadbeef".into(),
            block_number: 1,
            status: TxStatus::Success,
            gas_used: 1,
            fee_paid: 1,
            timestamp: 123,
            index: 0,
            error: None,
            contract_address: None,
            logs: vec![],
            return_data: None,
        };
        mgr._store_receipt(&rcpt).await.unwrap();
        let fetched = mgr.get_receipt(&rcpt.tx_hash).await.unwrap().unwrap();
        assert_eq!(fetched, rcpt);
    }
}
