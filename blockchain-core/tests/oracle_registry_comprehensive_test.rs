use anyhow::Result;
use dytallix_node::consensus::oracle_registry::{
    OracleRegistry, OracleRegistryConfig, OracleStatus, RegisterOracleArgs,
};

fn register_args(id: u32, stake_amount: u128) -> RegisterOracleArgs {
    RegisterOracleArgs {
        oracle_address: format!("dyt1oracle{}", id),
        oracle_name: format!("Test Oracle {}", id),
        description: format!("Test oracle {} for comprehensive testing", id),
        public_key: vec![id as u8; 32],
        stake_amount,
        oracle_version: "1.0.0".to_string(),
        supported_services: vec!["risk_scoring".to_string(), "fraud_detection".to_string()],
        contact_info: Some(format!("oracle{}@example.com", id)),
    }
}

#[tokio::test]
async fn test_oracle_registration_complete_flow() -> Result<()> {
    let registry = OracleRegistry::new(OracleRegistryConfig::default())?;
    let args = register_args(1, 2_001_000_000);

    registry.register_oracle(args.clone()).await?;

    let oracle = registry
        .get_oracle(&args.oracle_address)
        .await
        .expect("oracle should exist after registration");
    assert_eq!(oracle.oracle_name, args.oracle_name);
    assert_eq!(oracle.status, OracleStatus::Pending);
    assert_eq!(oracle.stake.total_amount, args.stake_amount);
    assert_eq!(oracle.reputation.current_score, 1.0);

    registry.activate_oracle(&args.oracle_address).await?;
    let activated = registry.get_oracle(&args.oracle_address).await.unwrap();
    assert_eq!(activated.status, OracleStatus::Active);

    assert!(registry.register_oracle(args).await.is_err());

    Ok(())
}

#[tokio::test]
async fn test_stake_requirements() -> Result<()> {
    let registry = OracleRegistry::new(OracleRegistryConfig {
        min_stake_amount: 5_000_000_000,
        ..OracleRegistryConfig::default()
    })?;

    assert!(registry
        .register_oracle(register_args(2, 1_000_000_000))
        .await
        .is_err());

    assert!(registry
        .register_oracle(register_args(2, 6_000_000_000))
        .await
        .is_ok());

    Ok(())
}

#[tokio::test]
async fn test_reputation_scoring_system() -> Result<()> {
    let registry = OracleRegistry::new(OracleRegistryConfig::default())?;
    let args = register_args(3, 2_003_000_000);

    registry.register_oracle(args.clone()).await?;
    registry.activate_oracle(&args.oracle_address).await?;

    registry
        .update_reputation(&args.oracle_address, 1000, true, true)
        .await?;
    let after_good = registry.get_oracle(&args.oracle_address).await.unwrap();
    assert!(after_good.reputation.current_score > 0.9);
    assert_eq!(after_good.reputation.accurate_responses, 1);

    registry
        .update_reputation(&args.oracle_address, 2000, false, true)
        .await?;
    let after_bad = registry.get_oracle(&args.oracle_address).await.unwrap();
    assert!(after_bad.reputation.current_score < after_good.reputation.current_score);
    assert_eq!(after_bad.reputation.inaccurate_responses, 1);

    registry
        .update_reputation(&args.oracle_address, 1500, true, false)
        .await?;
    let after_invalid_sig = registry.get_oracle(&args.oracle_address).await.unwrap();
    assert_eq!(after_invalid_sig.reputation.invalid_signature_responses, 1);

    Ok(())
}
