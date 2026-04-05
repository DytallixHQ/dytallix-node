/*
Genesis Block Configuration for Dytallix Mainnet
Implements the mainnet genesis configuration with dual-token system
*/

use crate::types::{Address, Amount, BlockNumber, Hash, Timestamp, ValidatorInfo};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Vesting schedule for token allocations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VestingSchedule {
    /// Total amount to be vested
    pub total_amount: Amount,
    /// Cliff period in seconds (tokens locked completely)
    pub cliff_duration: u64,
    /// Total vesting duration in seconds
    pub vesting_duration: u64,
    /// Start timestamp for vesting
    pub start_time: Timestamp,
}

impl VestingSchedule {
    /// Calculate vested amount at given timestamp
    pub fn _vested_amount(&self, current_time: Timestamp) -> Amount {
        if current_time < self.start_time + self.cliff_duration {
            return 0; // Still in cliff period
        }

        if current_time >= self.start_time + self.vesting_duration {
            return self.total_amount; // Fully vested
        }

        // Linear vesting after cliff
        let elapsed_since_cliff =
            u128::from(current_time - (self.start_time + self.cliff_duration));
        let vesting_period_after_cliff = u128::from(self.vesting_duration - self.cliff_duration);

        (self.total_amount * elapsed_since_cliff) / vesting_period_after_cliff
    }

    /// Calculate unvested (locked) amount at given timestamp
    pub fn _locked_amount(&self, current_time: Timestamp) -> Amount {
        self.total_amount - self._vested_amount(current_time)
    }
}

/// DGT (Governance Token) allocation with vesting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DGTAllocation {
    /// Recipient address
    pub address: Address,
    /// Allocation amount
    pub amount: Amount,
    /// Vesting schedule (None = unlocked immediately)
    pub vesting: Option<VestingSchedule>,
}

/// DRT emission configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DRTEmissionConfig {
    /// Annual inflation rate (5% = 500 basis points)
    pub annual_inflation_rate: u16,
    /// Initial supply (0 for DRT)
    pub initial_supply: Amount,
    /// Emission breakdown percentages (must sum to 100)
    pub emission_breakdown: EmissionBreakdown,
}

/// DRT emission distribution breakdown
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmissionBreakdown {
    /// Block rewards percentage (60%)
    pub block_rewards: u8,
    /// Staking rewards percentage (25%)
    pub staking_rewards: u8,
    /// AI module incentives percentage (10%)
    pub ai_module_incentives: u8,
    /// Bridge operations percentage (5%)
    pub bridge_operations: u8,
}

impl EmissionBreakdown {
    /// Validate that percentages sum to 100
    pub fn _is_valid(&self) -> bool {
        self.block_rewards
            + self.staking_rewards
            + self.ai_module_incentives
            + self.bridge_operations
            == 100
    }
}

/// Burn rules configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnRulesConfig {
    /// Percentage of transaction fees to burn (100% = all fees burned)
    pub transaction_fee_burn_rate: u8,
    /// Percentage of AI service fees to burn (50%)
    pub ai_service_fee_burn_rate: u8,
    /// Percentage of bridge fees to burn (75%)
    pub bridge_fee_burn_rate: u8,
}

/// Governance parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// Minimum DGT tokens required to create a proposal
    pub proposal_threshold: Amount,
    /// Voting period in blocks
    pub voting_period: BlockNumber,
    /// Minimum quorum for proposal to pass (basis points)
    pub quorum_threshold: u16,
    /// Percentage required for proposal to pass (basis points)
    pub pass_threshold: u16,
}

/// Staking parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StakingConfig {
    /// Minimum stake required to become a validator
    pub minimum_validator_stake: Amount,
    /// Maximum number of validators
    pub max_validators: u32,
    /// Slash percentage for double signing (basis points)
    pub double_sign_slash_rate: u16,
    /// Slash percentage for downtime (basis points)
    pub downtime_slash_rate: u16,
    /// Blocks to consider validator offline
    pub offline_threshold: BlockNumber,
    /// Emission rate per block (in uDRT)
    pub emission_per_block: u128,
}

impl StakingConfig {
    /// Convert to staking module parameters
    pub fn to_staking_params(&self) -> crate::staking::StakingParams {
        // Use defaults for fields not represented in StakingConfig
        let defaults = crate::staking::StakingParams::default();
        crate::staking::StakingParams {
            max_validators: self.max_validators,
            min_self_stake: self.minimum_validator_stake,
            slash_double_sign: self.double_sign_slash_rate,
            slash_downtime: self.downtime_slash_rate,
            emission_per_block: self.emission_per_block,
            // Newly required fields
            downtime_threshold: self.offline_threshold,
            signed_blocks_window: defaults.signed_blocks_window,
            min_signed_per_window: defaults.min_signed_per_window,
        }
    }
}

/// Network metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    /// Network name
    pub name: String,
    /// Chain ID
    pub chain_id: String,
    /// Genesis timestamp
    pub genesis_time: DateTime<Utc>,
}

/// Complete genesis configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisConfig {
    /// Network configuration
    pub network: NetworkConfig,
    /// DGT token allocations
    pub dgt_allocations: Vec<DGTAllocation>,
    /// DRT emission configuration
    pub drt_emission: DRTEmissionConfig,
    /// Burn rules
    pub burn_rules: BurnRulesConfig,
    /// Initial validator set
    pub validators: Vec<ValidatorInfo>,
    /// Governance parameters
    pub governance: GovernanceConfig,
    /// Staking parameters
    pub staking: StakingConfig,
    /// Genesis block hash (calculated)
    pub genesis_hash: Option<Hash>,
}

impl GenesisConfig {
    /// Create the mainnet genesis configuration
    pub fn mainnet() -> Self {
        Self::_mainnet()
    }

    /// Import from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        Self::_from_json(json)
    }

    /// Create the mainnet genesis configuration
    pub fn _mainnet() -> Self {
        let genesis_time = DateTime::parse_from_rfc3339("2025-08-03T19:00:26.000000000Z")
            .unwrap()
            .with_timezone(&Utc);

        // DGT allocations with vesting schedules
        let dgt_allocations = vec![
            // Community Treasury - 400M DGT, unlocked
            DGTAllocation {
                address: "0xCommunityTreasury".to_string(),
                amount: 400_000_000_000_000_000, // 400M tokens (assuming 18 decimals)
                vesting: None,
            },
            // Staking Rewards - 250M DGT, 4-year linear vesting
            DGTAllocation {
                address: "0xStakingRewards".to_string(),
                amount: 250_000_000_000_000_000, // 250M tokens
                vesting: Some(VestingSchedule {
                    total_amount: 250_000_000_000_000_000,
                    cliff_duration: 0, // No cliff for staking rewards
                    vesting_duration: 4 * 365 * 24 * 60 * 60, // 4 years in seconds
                    start_time: genesis_time.timestamp() as u64,
                }),
            },
            // Dev Team - 150M DGT, 1-year cliff + 3-year linear vesting
            DGTAllocation {
                address: "0xDevTeam".to_string(),
                amount: 150_000_000_000_000_000, // 150M tokens
                vesting: Some(VestingSchedule {
                    total_amount: 150_000_000_000_000_000,
                    cliff_duration: 365 * 24 * 60 * 60, // 1 year cliff
                    vesting_duration: 4 * 365 * 24 * 60 * 60, // Total 4 years (1 cliff + 3 vesting)
                    start_time: genesis_time.timestamp() as u64,
                }),
            },
            // Validators - 100M DGT, 6-month cliff + 2-year linear vesting
            DGTAllocation {
                address: "0xValidators".to_string(),
                amount: 100_000_000_000_000_000, // 100M tokens
                vesting: Some(VestingSchedule {
                    total_amount: 100_000_000_000_000_000,
                    cliff_duration: 6 * 30 * 24 * 60 * 60, // 6 months cliff (approx)
                    vesting_duration: (6 + 24) * 30 * 24 * 60 * 60, // Total 2.5 years (6m cliff + 2y vesting)
                    start_time: genesis_time.timestamp() as u64,
                }),
            },
            // Ecosystem Fund - 100M DGT, 5-year linear vesting
            DGTAllocation {
                address: "0xEcosystemFund".to_string(),
                amount: 100_000_000_000_000_000, // 100M tokens
                vesting: Some(VestingSchedule {
                    total_amount: 100_000_000_000_000_000,
                    cliff_duration: 0, // No cliff for ecosystem fund
                    vesting_duration: 5 * 365 * 24 * 60 * 60, // 5 years in seconds
                    start_time: genesis_time.timestamp() as u64,
                }),
            },
        ];

        // DRT emission configuration (~5% annual inflation)
        let drt_emission = DRTEmissionConfig {
            annual_inflation_rate: 500, // 5% in basis points
            initial_supply: 0,          // DRT starts with 0 supply
            emission_breakdown: EmissionBreakdown {
                block_rewards: 60,
                staking_rewards: 25,
                ai_module_incentives: 10,
                bridge_operations: 5,
            },
        };

        // Burn rules configuration
        let burn_rules = BurnRulesConfig {
            transaction_fee_burn_rate: 100, // 100% of transaction fees burned
            ai_service_fee_burn_rate: 50,   // 50% of AI service fees burned
            bridge_fee_burn_rate: 75,       // 75% of bridge fees burned
        };

        // Initial validator set (placeholder keys for now)
        let validators = vec![
            ValidatorInfo {
                address: "dyt1validator1000000000000000000000000000".to_string(),
                // Using 32 DGT with 6 decimals instead of 18 to fit into u128
                stake: 32_000_000_000_000u128, // 32 * 10^12 (represents 32 DGT if 12 decimals)
                public_key: vec![0u8; 32],     // Placeholder - would be real keys in production
                signature_algorithm: dytallix_pqc::SignatureAlgorithm::Dilithium5,
                active: true,
                commission: 500, // 5% commission
            },
            ValidatorInfo {
                address: "dyt1validator2000000000000000000000000000".to_string(),
                stake: 32_000_000_000_000u128,
                public_key: vec![1u8; 32],
                signature_algorithm: dytallix_pqc::SignatureAlgorithm::Dilithium5,
                active: true,
                commission: 500,
            },
            ValidatorInfo {
                address: "dyt1validator3000000000000000000000000000".to_string(),
                stake: 32_000_000_000_000u128,
                public_key: vec![2u8; 32],
                signature_algorithm: dytallix_pqc::SignatureAlgorithm::Dilithium5,
                active: true,
                commission: 500,
            },
        ];

        // Governance configuration
        let governance = GovernanceConfig {
            proposal_threshold: 1_000_000_000_000_000_000, // 1M DGT to create proposal
            voting_period: 50400,                          // ~7 days assuming 12s block time
            quorum_threshold: 3333,                        // 33.33% quorum required
            pass_threshold: 5000,                          // 50% majority required
        };

        // Staking configuration
        let staking = StakingConfig {
            minimum_validator_stake: 32_000_000_000_000u128,
            max_validators: 100,
            double_sign_slash_rate: 500,   // 5% slash for double signing
            downtime_slash_rate: 100,      // 1% slash for downtime
            offline_threshold: 300,        // 300 blocks (~1 hour) to be considered offline
            emission_per_block: 1_000_000, // 1 DRT per block in uDRT
        };

        Self {
            network: NetworkConfig {
                name: "dytallix-mainnet".to_string(),
                chain_id: "dytallix-mainnet-1".to_string(),
                genesis_time,
            },
            dgt_allocations,
            drt_emission,
            burn_rules,
            validators,
            governance,
            staking,
            genesis_hash: None, // Will be calculated when genesis block is created
        }
    }

    /// Validate the genesis configuration
    pub fn _validate(&self) -> Result<(), String> {
        // Validate DGT total supply is 1 billion
        let total_dgt: Amount = self.dgt_allocations.iter().map(|a| a.amount).sum();
        if total_dgt != 1_000_000_000_000_000_000 {
            return Err(format!(
                "DGT total supply must be 1 billion, got {total_dgt}"
            ));
        }

        // Validate emission breakdown
        if !self.drt_emission.emission_breakdown._is_valid() {
            return Err("DRT emission breakdown percentages must sum to 100".to_string());
        }

        // Validate burn rates are <= 100%
        if self.burn_rules.transaction_fee_burn_rate > 100
            || self.burn_rules.ai_service_fee_burn_rate > 100
            || self.burn_rules.bridge_fee_burn_rate > 100
        {
            return Err("Burn rates cannot exceed 100%".to_string());
        }

        // Validate governance parameters
        if self.governance.quorum_threshold > 10000 || self.governance.pass_threshold > 10000 {
            return Err(
                "Governance thresholds cannot exceed 100% (10000 basis points)".to_string(),
            );
        }

        // Validate staking parameters
        if self.staking.double_sign_slash_rate > 10000 || self.staking.downtime_slash_rate > 10000 {
            return Err("Slash rates cannot exceed 100% (10000 basis points)".to_string());
        }

        // Validate validators have minimum stake
        for validator in &self.validators {
            if validator.stake < self.staking.minimum_validator_stake {
                return Err(format!(
                    "Validator {} has insufficient stake",
                    validator.address
                ));
            }
        }

        Ok(())
    }

    /// Calculate total DGT supply
    pub fn _total_dgt_supply(&self) -> Amount {
        self.dgt_allocations.iter().map(|a| a.amount).sum()
    }

    /// Get vested amount for an address at given timestamp
    pub fn _get_vested_amount(&self, address: &Address, current_time: Timestamp) -> Amount {
        self.dgt_allocations
            .iter()
            .find(|alloc| &alloc.address == address)
            .map(|alloc| {
                match &alloc.vesting {
                    Some(vesting) => vesting._vested_amount(current_time),
                    None => alloc.amount, // Fully unlocked
                }
            })
            .unwrap_or(0)
    }

    /// Get locked amount for an address at given timestamp
    pub fn _get_locked_amount(&self, address: &Address, current_time: Timestamp) -> Amount {
        self.dgt_allocations
            .iter()
            .find(|alloc| &alloc.address == address)
            .map(|alloc| {
                match &alloc.vesting {
                    Some(vesting) => vesting._locked_amount(current_time),
                    None => 0, // Nothing locked
                }
            })
            .unwrap_or(0)
    }

    /// Export to JSON string
    pub fn _to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Import from JSON string
    pub fn _from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_genesis_amounts_serialize_as_strings() {
        let genesis = GenesisConfig::_mainnet();
        let json = genesis._to_json().unwrap();
        // Spot check one large number appears quoted
        assert!(json.contains("\"400000000000000000\""));
    }

    #[test]
    fn test_mainnet_genesis_validation() {
        let genesis = GenesisConfig::_mainnet();
        assert!(genesis._validate().is_ok());
    }

    #[test]
    fn test_dgt_total_supply() {
        let genesis = GenesisConfig::_mainnet();
        assert_eq!(genesis._total_dgt_supply(), 1_000_000_000_000_000_000);
    }

    #[test]
    fn test_emission_breakdown_validation() {
        let breakdown = EmissionBreakdown {
            block_rewards: 60,
            staking_rewards: 25,
            ai_module_incentives: 10,
            bridge_operations: 5,
        };
        assert!(breakdown._is_valid());

        let invalid_breakdown = EmissionBreakdown {
            block_rewards: 60,
            staking_rewards: 25,
            ai_module_incentives: 10,
            bridge_operations: 6, // Sum = 101%
        };
        assert!(!invalid_breakdown._is_valid());
    }

    #[test]
    fn test_vesting_schedule() {
        let start_time = 1722715226; // Genesis timestamp
        let vesting = VestingSchedule {
            total_amount: 1000,
            cliff_duration: 365 * 24 * 60 * 60,       // 1 year
            vesting_duration: 4 * 365 * 24 * 60 * 60, // 4 years total
            start_time,
        };

        // During cliff period
        let cliff_time = start_time + 6 * 30 * 24 * 60 * 60; // 6 months
        assert_eq!(vesting._vested_amount(cliff_time), 0);

        // After cliff, during vesting
        let mid_vesting_time = start_time + 2 * 365 * 24 * 60 * 60; // 2 years
        let vested = vesting._vested_amount(mid_vesting_time);
        assert!(vested > 0 && vested < 1000);

        // After full vesting
        let end_time = start_time + 5 * 365 * 24 * 60 * 60; // 5 years
        assert_eq!(vesting._vested_amount(end_time), 1000);
    }

    #[test]
    fn test_genesis_serialization() {
        let genesis = GenesisConfig::_mainnet();
        let json = genesis._to_json().unwrap();
        let deserialized = GenesisConfig::_from_json(&json).unwrap();

        assert_eq!(genesis.network.name, deserialized.network.name);
        assert_eq!(
            genesis.dgt_allocations.len(),
            deserialized.dgt_allocations.len()
        );
    }

    #[test]
    fn generate_genesis_json() {
        let genesis = GenesisConfig::_mainnet();

        // Validate the configuration
        genesis._validate().unwrap();

        // Convert to JSON
        let json = genesis._to_json().unwrap();

        // Write to genesisBlock.json in the project root
        let output_path = "../../genesisBlock.json";
        std::fs::write(output_path, &json).unwrap();

        println!("✅ Genesis configuration written to {}", output_path);
        println!("📊 Configuration summary:");
        println!("   Network: {}", genesis.network.name);
        println!("   Chain ID: {}", genesis.network.chain_id);
        println!("   Genesis Time: {}", genesis.network.genesis_time);
        println!(
            "   Total DGT Supply: {:.0} tokens",
            genesis._total_dgt_supply() as f64 / 1e18
        );
        println!(
            "   DGT Allocations: {} recipients",
            genesis.dgt_allocations.len()
        );
        println!("   Initial Validators: {}", genesis.validators.len());
        println!(
            "   DRT Annual Inflation: {}%",
            genesis.drt_emission.annual_inflation_rate as f64 / 100.0
        );

        // Print allocation breakdown
        println!("\n💰 DGT Token Allocations:");
        for allocation in &genesis.dgt_allocations {
            let amount_readable = allocation.amount as f64 / 1e18;
            match &allocation.vesting {
                Some(vesting) => {
                    let cliff_years = vesting.cliff_duration as f64 / (365.25 * 24.0 * 60.0 * 60.0);
                    let total_years =
                        vesting.vesting_duration as f64 / (365.25 * 24.0 * 60.0 * 60.0);
                    println!("   {} - {:.0}M DGT ({:.1}% of supply) - {:.1}y cliff, {:.1}y total vesting",
                        allocation.address,
                        amount_readable / 1e6,
                        (allocation.amount as f64 / genesis._total_dgt_supply() as f64) * 100.0,
                        cliff_years,
                        total_years
                    );
                }
                None => {
                    println!(
                        "   {} - {:.0}M DGT ({:.1}% of supply) - Unlocked",
                        allocation.address,
                        amount_readable / 1e6,
                        (allocation.amount as f64 / genesis._total_dgt_supply() as f64) * 100.0
                    );
                }
            }
        }

        println!("\n🔥 Burn Rules:");
        println!(
            "   Transaction fees: {}% burned",
            genesis.burn_rules.transaction_fee_burn_rate
        );
        println!(
            "   AI service fees: {}% burned",
            genesis.burn_rules.ai_service_fee_burn_rate
        );
        println!(
            "   Bridge fees: {}% burned",
            genesis.burn_rules.bridge_fee_burn_rate
        );

        println!("\n⚡ DRT Emission Breakdown:");
        println!(
            "   Block rewards: {}%",
            genesis.drt_emission.emission_breakdown.block_rewards
        );
        println!(
            "   Staking rewards: {}%",
            genesis.drt_emission.emission_breakdown.staking_rewards
        );
        println!(
            "   AI module incentives: {}%",
            genesis.drt_emission.emission_breakdown.ai_module_incentives
        );
        println!(
            "   Bridge operations: {}%",
            genesis.drt_emission.emission_breakdown.bridge_operations
        );

        println!("\n🏛️ Governance Parameters:");
        println!(
            "   Proposal threshold: {:.0}M DGT",
            genesis.governance.proposal_threshold as f64 / 1e24
        );
        println!(
            "   Voting period: {} blocks",
            genesis.governance.voting_period
        );
        println!(
            "   Quorum threshold: {:.1}%",
            genesis.governance.quorum_threshold as f64 / 100.0
        );
        println!(
            "   Pass threshold: {:.1}%",
            genesis.governance.pass_threshold as f64 / 100.0
        );

        println!("\n🔒 Staking Parameters:");
        println!(
            "   Minimum validator stake: {:.0}M DGT",
            genesis.staking.minimum_validator_stake as f64 / 1e24
        );
        println!("   Maximum validators: {}", genesis.staking.max_validators);
        println!(
            "   Double sign slash: {:.1}%",
            genesis.staking.double_sign_slash_rate as f64 / 100.0
        );
        println!(
            "   Downtime slash: {:.1}%",
            genesis.staking.downtime_slash_rate as f64 / 100.0
        );
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub const TEST_GENESIS_FLAG: bool = true;
}
