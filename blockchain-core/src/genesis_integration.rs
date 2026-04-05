/*
Genesis Block Integration for Consensus Engine

Integrates genesis configuration with the consensus engine to initialize
the blockchain state with proper token allocations, validators, and parameters.
*/

use crate::genesis::GenesisConfig;
use crate::types::{
    AccountState, Address, Amount, Block, BlockHeader, Hash, Transaction, ValidatorInfo,
};
use chrono::Utc;
use sha3::{Digest, Sha3_256};
use std::collections::HashMap; // retained for future extensions

/// Genesis block creator for Dytallix blockchain
pub struct GenesisBlockCreator {
    config: GenesisConfig,
}

impl GenesisBlockCreator {
    /// Create a new genesis block creator with the given configuration
    pub fn new(config: GenesisConfig) -> Self {
        Self { config }
    }

    /// Create the genesis block
    pub fn create_genesis_block(&self) -> Result<Block, String> {
        // Validate the configuration first
        self.config._validate()?;

        // Create genesis transactions for DGT allocations
        let genesis_transactions = self.create_genesis_transactions()?;

        // Calculate the merkle root of transactions
        let transactions_root = BlockHeader::calculate_transactions_root(&genesis_transactions);

        // Create the genesis state root (initially empty state + allocations)
        let state_root = self.calculate_genesis_state_root()?;

        // Genesis timestamp
        let genesis_timestamp = self.config.network.genesis_time.timestamp() as u64;

        // Create genesis block header
        let header = BlockHeader {
            number: 0,                   // Genesis block is block 0
            parent_hash: "0".repeat(64), // No parent for genesis block
            transactions_root,
            state_root,
            timestamp: genesis_timestamp,
            validator: "genesis".to_string(), // Special genesis validator
            signature: crate::types::PQCBlockSignature {
                signature: dytallix_pqc::Signature {
                    data: vec![0u8; 32], // Genesis signature placeholder
                    algorithm: dytallix_pqc::SignatureAlgorithm::Dilithium5,
                },
                public_key: vec![0u8; 32], // Genesis public key placeholder
            },
            nonce: 0,
        };

        let genesis_block = Block {
            header,
            transactions: genesis_transactions,
        };

        Ok(genesis_block)
    }

    /// Create genesis transactions for initial DGT allocations
    fn create_genesis_transactions(&self) -> Result<Vec<Transaction>, String> {
        let mut transactions = Vec::new();

        // Create mint transactions for each DGT allocation
        for (index, allocation) in self.config.dgt_allocations.iter().enumerate() {
            let tx = Transaction::Transfer(crate::types::TransferTransaction {
                hash: format!("genesis_mint_{index}"),
                from: "genesis_mint".to_string(), // Special genesis minter address
                to: allocation.address.clone(),
                amount: allocation.amount,
                fee: 0, // No fees for genesis transactions
                nonce: index as u64,
                timestamp: self.config.network.genesis_time.timestamp() as u64,
                signature: crate::types::PQCTransactionSignature {
                    signature: dytallix_pqc::Signature {
                        data: vec![0u8; 32], // Genesis signature placeholder
                        algorithm: dytallix_pqc::SignatureAlgorithm::Dilithium5,
                    },
                    public_key: vec![0u8; 32], // Genesis public key placeholder
                },
                ai_risk_score: Some(0.0), // Genesis transactions are safe
            });
            transactions.push(tx);
        }

        Ok(transactions)
    }

    /// Calculate the genesis state root after applying all allocations
    fn calculate_genesis_state_root(&self) -> Result<Hash, String> {
        let mut state_data = String::new();

        // Add all DGT allocations to the state calculation
        for allocation in &self.config.dgt_allocations {
            state_data.push_str(&format!("{}:{}", allocation.address, allocation.amount));
        }

        // Add genesis configuration hash
        state_data.push_str(&format!("network:{}", self.config.network.chain_id));
        state_data.push_str(&format!("validators:{}", self.config.validators.len()));

        // Calculate SHA3-256 hash
        let mut hasher = Sha3_256::new();
        hasher.update(state_data.as_bytes());
        let hash = hasher.finalize();

        Ok(hex::encode(hash))
    }

    /// Initialize account states from genesis configuration
    pub fn create_genesis_accounts(&self) -> HashMap<Address, AccountState> {
        let mut accounts = HashMap::new();

        // Create accounts for DGT allocations
        for allocation in &self.config.dgt_allocations {
            let account_state = AccountState {
                balance: allocation.amount,
                nonce: 0,
                code: None,
                storage: HashMap::new(),
                reputation_score: 1000, // Max reputation for genesis accounts
                last_ai_analysis: Some(self.config.network.genesis_time.timestamp() as u64),
            };
            accounts.insert(allocation.address.clone(), account_state);
        }

        // Create accounts for validators
        for validator in &self.config.validators {
            // Skip if validator account already exists (might be in DGT allocations)
            if !accounts.contains_key(&validator.address) {
                let account_state = AccountState {
                    balance: 0, // Validators might not have initial balance beyond stake
                    nonce: 0,
                    code: None,
                    storage: HashMap::new(),
                    reputation_score: 1000, // Max reputation for genesis validators
                    last_ai_analysis: Some(self.config.network.genesis_time.timestamp() as u64),
                };
                accounts.insert(validator.address.clone(), account_state);
            }
        }

        accounts
    }

    /// Get the genesis configuration
    pub fn config(&self) -> &GenesisConfig {
        &self.config
    }

    /// Get vested amount for an address at current time
    pub fn get_current_vested_amount(&self, address: &Address) -> Amount {
        let current_time = Utc::now().timestamp() as u64;
        self.config._get_vested_amount(address, current_time)
    }

    /// Get locked amount for an address at current time
    pub fn get_current_locked_amount(&self, address: &Address) -> Amount {
        let current_time = Utc::now().timestamp() as u64;
        self.config._get_locked_amount(address, current_time)
    }

    /// Check if an address can transfer a certain amount considering vesting
    pub fn can_transfer(&self, address: &Address, amount: Amount) -> bool {
        let vested = self.get_current_vested_amount(address);
        vested >= amount
    }
}

/// Genesis initialization helper for the consensus engine
pub struct GenesisInitializer;

/// Result of blockchain initialization
pub type GenesisInitializationResult =
    Result<(Block, HashMap<Address, AccountState>, Vec<ValidatorInfo>), String>;

impl GenesisInitializer {
    /// Initialize the blockchain with genesis configuration
    pub fn initialize_blockchain(config: GenesisConfig) -> GenesisInitializationResult {
        // Create genesis block
        let creator = GenesisBlockCreator::new(config.clone());
        let genesis_block = creator.create_genesis_block()?;

        // Create initial account states
        let genesis_accounts = creator.create_genesis_accounts();

        // Get initial validators
        let validators = config.validators.clone();

        Ok((genesis_block, genesis_accounts, validators))
    }

    /// Validate genesis block against configuration
    pub fn validate_genesis_block(block: &Block, config: &GenesisConfig) -> Result<(), String> {
        // Check if it's actually block 0
        if block.header.number != 0 {
            return Err("Genesis block must have number 0".to_string());
        }

        // Check timestamp matches configuration
        let expected_timestamp = config.network.genesis_time.timestamp() as u64;
        if block.header.timestamp != expected_timestamp {
            return Err(format!(
                "Genesis timestamp mismatch: expected {}, got {}",
                expected_timestamp, block.header.timestamp
            ));
        }

        // Check that we have the correct number of genesis transactions
        let expected_tx_count = config.dgt_allocations.len();
        if block.transactions.len() != expected_tx_count {
            return Err(format!(
                "Expected {} genesis transactions, got {}",
                expected_tx_count,
                block.transactions.len()
            ));
        }

        // Validate each transaction corresponds to a DGT allocation
        for (index, transaction) in block.transactions.iter().enumerate() {
            if let Transaction::Transfer(tx) = transaction {
                let allocation = &config.dgt_allocations[index];
                if tx.to != allocation.address || tx.amount != allocation.amount {
                    return Err(format!(
                        "Genesis transaction {index} does not match allocation"
                    ));
                }
            } else {
                return Err(format!("Genesis transaction {index} is not a transfer"));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_block_creation() {
        let config = GenesisConfig::mainnet();
        let creator = GenesisBlockCreator::new(config.clone());

        let genesis_block = creator.create_genesis_block().unwrap();

        // Verify genesis block properties
        assert_eq!(genesis_block.header.number, 0);
        assert_eq!(genesis_block.header.parent_hash, "0".repeat(64));
        assert_eq!(
            genesis_block.transactions.len(),
            config.dgt_allocations.len()
        );

        // Verify all allocations are present as transactions
        for (index, allocation) in config.dgt_allocations.iter().enumerate() {
            if let Transaction::Transfer(tx) = &genesis_block.transactions[index] {
                assert_eq!(tx.to, allocation.address);
                assert_eq!(tx.amount, allocation.amount);
                assert_eq!(tx.fee, 0);
            } else {
                panic!("Expected transfer transaction");
            }
        }
    }

    #[test]
    fn test_genesis_accounts_creation() {
        let config = GenesisConfig::mainnet();
        let creator = GenesisBlockCreator::new(config.clone());

        let accounts = creator.create_genesis_accounts();

        // Should have accounts for all allocations and validators
        assert!(accounts.len() >= config.dgt_allocations.len());

        // Check each allocation has correct balance
        for allocation in &config.dgt_allocations {
            let account = accounts.get(&allocation.address).unwrap();
            assert_eq!(account.balance, allocation.amount);
            assert_eq!(account.nonce, 0);
            assert_eq!(account.reputation_score, 1000);
        }
    }

    #[test]
    fn test_vesting_calculations() {
        let config = GenesisConfig::mainnet();
        let creator = GenesisBlockCreator::new(config);

        // Test community treasury (should be fully unlocked)
        let community_vested =
            creator.get_current_vested_amount(&"0xCommunityTreasury".to_string());
        assert_eq!(community_vested, 400_000_000_000_000_000_000_000_000);

        // Test dev team (should be locked due to cliff)
        let dev_vested = creator.get_current_vested_amount(&"0xDevTeam".to_string());
        assert_eq!(dev_vested, 0); // Should be 0 due to 1-year cliff

        let dev_locked = creator.get_current_locked_amount(&"0xDevTeam".to_string());
        assert_eq!(dev_locked, 150_000_000_000_000_000_000_000_000);
    }

    #[test]
    fn test_transfer_permissions() {
        let config = GenesisConfig::mainnet();
        let creator = GenesisBlockCreator::new(config);

        // Community treasury should be able to transfer full amount
        assert!(creator.can_transfer(
            &"0xCommunityTreasury".to_string(),
            400_000_000_000_000_000_000_000_000
        ));

        // Dev team should not be able to transfer anything due to cliff
        assert!(!creator.can_transfer(&"0xDevTeam".to_string(), 1));

        // Ecosystem fund should be able to transfer some amount (linear vesting, no cliff)
        assert!(creator.can_transfer(&"0xEcosystemFund".to_string(), 1_000_000_000_000_000_000));
        // 1 token
    }

    #[test]
    fn test_genesis_validation() {
        let config = GenesisConfig::mainnet();
        let creator = GenesisBlockCreator::new(config.clone());
        let genesis_block = creator.create_genesis_block().unwrap();

        // Should validate successfully
        GenesisInitializer::validate_genesis_block(&genesis_block, &config).unwrap();

        // Test with wrong block number
        let mut invalid_block = genesis_block.clone();
        invalid_block.header.number = 1;
        assert!(GenesisInitializer::validate_genesis_block(&invalid_block, &config).is_err());
    }

    #[test]
    fn test_blockchain_initialization() {
        let config = GenesisConfig::mainnet();

        let (genesis_block, accounts, validators) =
            GenesisInitializer::initialize_blockchain(config.clone()).unwrap();

        // Verify results
        assert_eq!(genesis_block.header.number, 0);
        assert!(!accounts.is_empty());
        assert_eq!(validators.len(), config.validators.len());

        // Verify total DGT balance across all accounts
        let total_balance: Amount = accounts.values().map(|acc| acc.balance).sum();
        assert_eq!(total_balance, 1_000_000_000_000_000_000_000_000_000); // 1 billion DGT
    }
}
