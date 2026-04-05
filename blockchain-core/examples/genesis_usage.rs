/*
Example: Using the Genesis Configuration

This example demonstrates how to use the genesis configuration
to initialize a Dytallix blockchain node.
*/

use chrono::Utc;
use dytallix_node::{
    genesis::{GenesisConfig, VestingSchedule},
    genesis_integration::{GenesisBlockCreator, GenesisInitializer},
    types::{AccountState, Address, Block, ValidatorInfo},
};
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Dytallix Genesis Configuration Example");
    println!("{}", "=".repeat(50));

    // 1. Load mainnet genesis configuration
    let genesis_config = GenesisConfig::mainnet();

    // 2. Validate the configuration
    genesis_config.validate()?;
    println!("✅ Genesis configuration validated");

    // 3. Create genesis block creator
    let creator = GenesisBlockCreator::new(genesis_config.clone());

    // 4. Generate the genesis block
    let genesis_block = creator.create_genesis_block()?;
    println!(
        "✅ Genesis block created: Block #{}",
        genesis_block.header.number
    );

    // 5. Initialize blockchain state
    let (block, accounts, validators) =
        GenesisInitializer::initialize_blockchain(genesis_config.clone())?;
    println!(
        "✅ Blockchain initialized with {} accounts and {} validators",
        accounts.len(),
        validators.len()
    );

    // 6. Display genesis information
    display_genesis_summary(&genesis_config);

    // 7. Demonstrate vesting calculations
    demonstrate_vesting(&creator);

    // 8. Show burn rule implementation
    demonstrate_burn_rules(&genesis_config);

    println!("\n🎉 Genesis configuration ready for mainnet!");
    Ok(())
}

fn display_genesis_summary(config: &GenesisConfig) {
    println!("\n📊 Genesis Summary:");
    println!("Network: {}", config.network.name);
    println!("Chain ID: {}", config.network.chain_id);
    println!("Genesis Time: {}", config.network.genesis_time);

    println!("\n💰 DGT Allocations:");
    let total_supply = config.total_dgt_supply();
    for allocation in &config.dgt_allocations {
        let amount_tokens = allocation.amount as f64 / 1e18;
        let percentage = (allocation.amount as f64 / total_supply as f64) * 100.0;

        match &allocation.vesting {
            Some(vesting) => {
                let cliff_years = vesting.cliff_duration as f64 / (365.25 * 24.0 * 60.0 * 60.0);
                let total_years = vesting.vesting_duration as f64 / (365.25 * 24.0 * 60.0 * 60.0);
                println!(
                    "  {} - {:.0}M DGT ({:.1}%) - {:.1}y cliff, {:.1}y vesting",
                    allocation.address,
                    amount_tokens / 1e6,
                    percentage,
                    cliff_years,
                    total_years
                );
            }
            None => {
                println!(
                    "  {} - {:.0}M DGT ({:.1}%) - Unlocked",
                    allocation.address,
                    amount_tokens / 1e6,
                    percentage
                );
            }
        }
    }

    println!("\n⚡ DRT Emission:");
    println!(
        "  Annual Inflation: {}%",
        config.drt_emission.annual_inflation_rate as f64 / 100.0
    );
    println!(
        "  Block Rewards: {}%",
        config.drt_emission.emission_breakdown.block_rewards
    );
    println!(
        "  Staking Rewards: {}%",
        config.drt_emission.emission_breakdown.staking_rewards
    );
    println!(
        "  AI Incentives: {}%",
        config.drt_emission.emission_breakdown.ai_module_incentives
    );
    println!(
        "  Bridge Operations: {}%",
        config.drt_emission.emission_breakdown.bridge_operations
    );
}

fn demonstrate_vesting(creator: &GenesisBlockCreator) {
    println!("\n⏰ Vesting Demonstration (Current Time):");

    let addresses = vec![
        "0xCommunityTreasury",
        "0xStakingRewards",
        "0xDevTeam",
        "0xValidators",
        "0xEcosystemFund",
    ];

    for address in addresses {
        let address = address.to_string();
        let vested = creator.get_current_vested_amount(&address);
        let locked = creator.get_current_locked_amount(&address);
        let total = vested + locked;

        if total > 0 {
            let vested_tokens = vested as f64 / 1e18;
            let locked_tokens = locked as f64 / 1e18;
            let total_tokens = total as f64 / 1e18;
            let vested_pct = if total > 0 {
                (vested as f64 / total as f64) * 100.0
            } else {
                0.0
            };

            println!(
                "  {}: {:.0}M vested ({:.1}%), {:.0}M locked, {:.0}M total",
                address,
                vested_tokens / 1e6,
                vested_pct,
                locked_tokens / 1e6,
                total_tokens / 1e6
            );
        }
    }
}

fn demonstrate_burn_rules(config: &GenesisConfig) {
    println!("\n🔥 Burn Rules Implementation:");

    // Example fee amounts (in smallest unit)
    let tx_fee = 1_000_000_000_000_000_000; // 1 DGT
    let ai_fee = 500_000_000_000_000_000; // 0.5 DGT
    let bridge_fee = 2_000_000_000_000_000_000; // 2 DGT

    let tx_burned = (tx_fee * config.burn_rules.transaction_fee_burn_rate as u64) / 100;
    let ai_burned = (ai_fee * config.burn_rules.ai_service_fee_burn_rate as u64) / 100;
    let bridge_burned = (bridge_fee * config.burn_rules.bridge_fee_burn_rate as u64) / 100;

    println!(
        "  Transaction Fee (1 DGT): {:.2} DGT burned ({}%)",
        tx_burned as f64 / 1e18,
        config.burn_rules.transaction_fee_burn_rate
    );
    println!(
        "  AI Service Fee (0.5 DGT): {:.2} DGT burned ({}%)",
        ai_burned as f64 / 1e18,
        config.burn_rules.ai_service_fee_burn_rate
    );
    println!(
        "  Bridge Fee (2 DGT): {:.2} DGT burned ({}%)",
        bridge_burned as f64 / 1e18,
        config.burn_rules.bridge_fee_burn_rate
    );
}

// Helper function for simulating time-based vesting
fn simulate_vesting_over_time(config: &GenesisConfig) {
    println!("\n📅 Vesting Simulation Over Time:");

    let genesis_time = config.network.genesis_time.timestamp() as u64;
    let time_points = vec![
        ("Genesis", genesis_time),
        ("6 Months", genesis_time + 6 * 30 * 24 * 60 * 60),
        ("1 Year", genesis_time + 365 * 24 * 60 * 60),
        ("2 Years", genesis_time + 2 * 365 * 24 * 60 * 60),
        ("3 Years", genesis_time + 3 * 365 * 24 * 60 * 60),
        ("4 Years", genesis_time + 4 * 365 * 24 * 60 * 60),
        ("5 Years", genesis_time + 5 * 365 * 24 * 60 * 60),
    ];

    for (label, timestamp) in time_points {
        println!("\n  📍 {label}:");

        let dev_vested = config.get_vested_amount(&"0xDevTeam".to_string(), timestamp);
        let dev_total = 150_000_000_000_000_000_000_000_000u64;
        let dev_pct = (dev_vested as f64 / dev_total as f64) * 100.0;

        let validator_vested = config.get_vested_amount(&"0xValidators".to_string(), timestamp);
        let validator_total = 100_000_000_000_000_000_000_000_000u64;
        let validator_pct = (validator_vested as f64 / validator_total as f64) * 100.0;

        println!("    Dev Team: {:.1}% vested", dev_pct);
        println!("    Validators: {:.1}% vested", validator_pct);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_example_genesis_creation() {
        let config = GenesisConfig::mainnet();
        assert!(config.validate().is_ok());

        let creator = GenesisBlockCreator::new(config);
        let genesis_block = creator.create_genesis_block().unwrap();

        assert_eq!(genesis_block.header.number, 0);
        assert_eq!(genesis_block.transactions.len(), 5); // 5 DGT allocations
    }

    #[test]
    fn test_vesting_edge_cases() {
        let config = GenesisConfig::mainnet();
        let creator = GenesisBlockCreator::new(config.clone());

        // Community treasury should always be fully vested
        let community_vested =
            creator.get_current_vested_amount(&"0xCommunityTreasury".to_string());
        assert_eq!(community_vested, 400_000_000_000_000_000_000_000_000);

        // Non-existent address should return 0
        let unknown_vested = creator.get_current_vested_amount(&"0xUnknownAddress".to_string());
        assert_eq!(unknown_vested, 0);
    }

    #[test]
    fn test_burn_calculations() {
        let config = GenesisConfig::mainnet();

        // Test transaction fee burn (100%)
        let fee = 1_000_000_000_000_000_000; // 1 DGT
        let burned = (fee * config.burn_rules.transaction_fee_burn_rate as u64) / 100;
        assert_eq!(burned, fee); // Should be 100% burned

        // Test AI service fee burn (50%)
        let ai_fee = 2_000_000_000_000_000_000; // 2 DGT
        let ai_burned = (ai_fee * config.burn_rules.ai_service_fee_burn_rate as u64) / 100;
        assert_eq!(ai_burned, 1_000_000_000_000_000_000); // Should be 1 DGT burned
    }
}
