/*
Deterministic execution module for dytallix consensus.

Implements upfront fee charging, full-revert semantics, and deterministic gas accounting
to ensure all nodes reach identical post-state and receipts.
Includes fee burning mechanism for dual-token economics.
*/

use crate::gas::{intrinsic_gas, Gas, GasError, GasMeter, GasSchedule, TxKind};
use crate::runtime::fee_burn::FeeBurnEngine;
use crate::state::State;
use crate::storage::receipts::{TxReceipt, TxStatus, RECEIPT_FORMAT_VERSION};
use crate::storage::tx::Transaction;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("Insufficient funds: required {required}, available {available}")]
    InsufficientFunds { required: u128, available: u128 },

    #[error("InvalidNonce: expected {expected}, got {actual}")]
    InvalidNonce { expected: u64, actual: u64 },

    #[error("Gas error: {0}")]
    Gas(#[from] GasError),

    #[error("Overflow in fee calculation")]
    FeeOverflow,

    #[error("State error: {0}")]
    State(String),
}

/// Execution context for a single transaction
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub gas_limit: Gas,
    pub gas_price: u64,
    pub gas_meter: GasMeter,
    pub state_changes: Vec<StateChange>,
    pub events: Vec<String>, // Simplified events for now
}

/// Represents a state change that can be reverted
#[derive(Debug, Clone)]
pub struct StateChange {
    pub address: String,
    pub denom: String,
    pub old_balance: u128,
    pub new_balance: u128,
}

/// Result of transaction execution
#[derive(Debug)]
pub struct ExecutionResult {
    pub receipt: TxReceipt,
    pub state_changes: Vec<StateChange>,
    pub gas_used: Gas,
    pub success: bool,
}

impl ExecutionContext {
    pub fn new(gas_limit: Gas, gas_price: u64) -> Self {
        Self {
            gas_limit,
            gas_price,
            gas_meter: GasMeter::new(gas_limit),
            state_changes: Vec::new(),
            events: Vec::new(),
        }
    }

    /// Calculate upfront fee with overflow protection
    pub fn calculate_upfront_fee(&self) -> Result<u128, ExecutionError> {
        let gas_limit = self.gas_limit as u128;
        let gas_price = self.gas_price as u128;

        gas_limit
            .checked_mul(gas_price)
            .ok_or(ExecutionError::FeeOverflow)
    }

    /// Record a state change for potential revert
    pub fn record_state_change(
        &mut self,
        address: String,
        denom: String,
        old_balance: u128,
        new_balance: u128,
    ) {
        self.state_changes.push(StateChange {
            address,
            denom,
            old_balance,
            new_balance,
        });
    }

    /// Consume gas and check if we're out of gas
    pub fn consume_gas(&mut self, amount: Gas, operation: &str) -> Result<(), GasError> {
        self.gas_meter.consume(amount, operation)
    }

    /// Get gas used so far
    pub fn gas_used(&self) -> Gas {
        self.gas_meter.gas_used()
    }

    /// Revert all state changes recorded in this context
    pub fn revert_state_changes(&self, state: &mut State) {
        // Revert changes in reverse order
        for change in self.state_changes.iter().rev() {
            // Restore the old balance
            state.set_balance(&change.address, &change.denom, change.old_balance);
        }
    }
}

/// Execute a single transaction with deterministic gas accounting and fee burning
pub fn execute_transaction(
    tx: &Transaction,
    state: &mut State,
    block_height: u64,
    tx_index: u32,
    gas_schedule: &GasSchedule,
    fee_burn_engine: Option<&mut FeeBurnEngine>,
) -> ExecutionResult {
    // If transaction has multiple messages, process them all
    if let Some(messages) = &tx.messages {
        return execute_multi_message_transaction(
            tx,
            messages,
            state,
            block_height,
            tx_index,
            gas_schedule,
            fee_burn_engine,
        );
    }

    // Otherwise, fall back to single-message execution (legacy path)
    execute_single_message_transaction(tx, state, block_height, tx_index, gas_schedule, fee_burn_engine)
}

/// Execute a transaction with multiple messages
fn execute_multi_message_transaction(
    tx: &Transaction,
    messages: &[crate::storage::tx::TxMessage],
    state: &mut State,
    block_height: u64,
    tx_index: u32,
    gas_schedule: &GasSchedule,
    fee_burn_engine: Option<&mut FeeBurnEngine>,
) -> ExecutionResult {
    // Step 1: Validate basic transaction fields
    if let Err(error) = validate_transaction(tx, state) {
        return ExecutionResult {
            receipt: create_failed_receipt(tx, 0, 0, 0, error.to_string(), block_height, tx_index),
            state_changes: Vec::new(),
            gas_used: 0,
            success: false,
        };
    }

    // Step 2: Determine gas parameters
    let (gas_limit, gas_price) = if tx.gas_limit > 0 && tx.gas_price > 0 {
        (tx.gas_limit, tx.gas_price)
    } else {
        (tx.fee as u64, 1u64)
    };

    // Step 3: Create execution context
    let mut ctx = ExecutionContext::new(gas_limit, gas_price);

    // Step 4: Calculate and deduct upfront fee
    let upfront_fee = match ctx.calculate_upfront_fee() {
        Ok(fee) => fee,
        Err(error) => {
            return ExecutionResult {
                receipt: create_failed_receipt(tx, 0, gas_limit, gas_price, error.to_string(), block_height, tx_index),
                state_changes: Vec::new(),
                gas_used: 0,
                success: false,
            };
        }
    };

    let sender_balance = state.balance_of(&tx.from, "udgt");
    if sender_balance < upfront_fee {
        let err_msg = format!("InsufficientFunds: required {upfront_fee}, available {sender_balance}");
        return ExecutionResult {
            receipt: create_failed_receipt(tx, 0, gas_limit, gas_price, err_msg, block_height, tx_index),
            state_changes: Vec::new(),
            gas_used: 0,
            success: false,
        };
    }

    // Step 5: Deduct upfront fee and consume nonce
    let new_sender_balance = sender_balance - upfront_fee;
    state.set_balance(&tx.from, "udgt", new_sender_balance);
    state.increment_nonce(&tx.from);

    // Step 6: Consume minimal overhead gas
    if let Err(_) = ctx.consume_gas(1, "tx_overhead") {
        return ExecutionResult {
            receipt: create_failed_receipt(tx, ctx.gas_used(), gas_limit, gas_price, "OutOfGas".to_string(), block_height, tx_index),
            state_changes: Vec::new(),
            gas_used: ctx.gas_used(),
            success: false,
        };
    }

    // Step 7: Calculate intrinsic gas
    let tx_size = estimate_transaction_size(tx);
    let intrinsic_gas = match intrinsic_gas(&TxKind::Transfer, tx_size, messages.len(), gas_schedule) {
        Ok(gas) => gas,
        Err(error) => {
            return ExecutionResult {
                receipt: create_failed_receipt(tx, ctx.gas_used(), gas_limit, gas_price, error.to_string(), block_height, tx_index),
                state_changes: Vec::new(),
                gas_used: ctx.gas_used(),
                success: false,
            };
        }
    };

    if let Err(_) = ctx.consume_gas(intrinsic_gas, "intrinsic") {
        ctx.revert_state_changes(state);
        return ExecutionResult {
            receipt: create_failed_receipt(tx, ctx.gas_used(), gas_limit, gas_price, "OutOfGas".to_string(), block_height, tx_index),
            state_changes: Vec::new(),
            gas_used: ctx.gas_used(),
            success: false,
        };
    }

    // Step 8: Execute each message
    for (idx, msg) in messages.iter().enumerate() {
        if let Err(e) = execute_message(msg, state, &mut ctx, block_height) {
            // Out of gas during execution - revert state but keep fee
            ctx.revert_state_changes(state);
            return ExecutionResult {
                receipt: create_failed_receipt(tx, ctx.gas_used(), gas_limit, gas_price, format!("Execution failed at message {}: {}", idx, e), block_height, tx_index),
                state_changes: Vec::new(),
                gas_used: ctx.gas_used(),
                success: false,
            };
        }
    }

    // Step 9: Process fee burning if enabled
    if let Some(engine) = fee_burn_engine {
        let _ = engine.process_fee_burn(tx.hash.clone(), block_height, upfront_fee, state);
    }

    // Step 10: Commit state changes and create success receipt
    let state_changes = ctx.state_changes.clone();
    ExecutionResult {
        success: true,
        state_changes,
        gas_used: ctx.gas_used(),
        receipt: create_success_receipt(tx, ctx.gas_used(), gas_limit, gas_price, block_height, tx_index),
    }
}

/// Execute a single message
fn execute_message(
    msg: &crate::storage::tx::TxMessage,
    state: &mut State,
    ctx: &mut ExecutionContext,
    block_height: u64,
) -> Result<(), GasError> {
    use crate::storage::tx::TxMessage;
    
    match msg {
        TxMessage::Send { from, to, denom, amount } => {
            // Charge gas for KV operations
            ctx.consume_gas(40, "kv_read_from")?;
            ctx.consume_gas(40, "kv_read_to")?;
            ctx.consume_gas(120, "kv_write_from")?;
            ctx.consume_gas(120, "kv_write_to")?;

            // Record the transfer state changes
            let sender_old_balance = state.balance_of(from, denom);
            let recipient_old_balance = state.balance_of(to, denom);

            // Check sufficient funds
            if sender_old_balance < *amount {
                // Note: This shouldn't happen as validation should catch it, but being defensive
                return Err(GasError::OutOfGas {
                    required: *amount as u64,
                    available: sender_old_balance as u64,
                });
            }

            let sender_new_balance = sender_old_balance - amount;
            let recipient_new_balance = recipient_old_balance + amount;

            ctx.record_state_change(from.clone(), denom.clone(), sender_old_balance, sender_new_balance);
            ctx.record_state_change(to.clone(), denom.clone(), recipient_old_balance, recipient_new_balance);

            // Apply the transfer
            state.set_balance(from, denom, sender_new_balance);
            state.set_balance(to, denom, recipient_new_balance);

            Ok(())
        }
        TxMessage::Data { data, .. } => {
            // Charge gas proportional to data size (1 gas per byte)
            let data_size = data.len() as u64;
            ctx.consume_gas(data_size, "data_storage")?;
            
            // Data messages don't modify state - they're just anchored on-chain
            // The data is already stored in the transaction itself
            Ok(())
        }
        TxMessage::DmsRegister { from, beneficiary, period } => {
            ctx.consume_gas(1000, "dms_register")?;
            let dms = crate::runtime::dead_man_switch::DeadManSwitchModule::new(state.storage.clone());
            dms.register(from, beneficiary, *period as u64, block_height)
                .map_err(|e| GasError::Custom(e))?;
            Ok(())
        }
        TxMessage::DmsPing { from } => {
            ctx.consume_gas(500, "dms_ping")?;
            let dms = crate::runtime::dead_man_switch::DeadManSwitchModule::new(state.storage.clone());
            dms.ping(from, block_height)
                .map_err(|e| GasError::Custom(e))?;
            Ok(())
        }
        TxMessage::DmsClaim { from, owner } => {
            ctx.consume_gas(2000, "dms_claim")?;
            let dms = crate::runtime::dead_man_switch::DeadManSwitchModule::new(state.storage.clone());
            let beneficiary = dms.validate_claim(owner, from, block_height)
                .map_err(|e| GasError::Custom(e))?;
            
            // Transfer all funds
            let balances = state.balances_of(owner);
            for (denom, amount) in balances {
                if amount > 0 {
                    let owner_old = amount;
                    let beneficiary_old = state.balance_of(&beneficiary, &denom);
                    
                    let owner_new = 0;
                    let beneficiary_new = beneficiary_old + amount;
                    
                    ctx.record_state_change(owner.clone(), denom.clone(), owner_old, owner_new);
                    ctx.record_state_change(beneficiary.clone(), denom.clone(), beneficiary_old, beneficiary_new);
                    
                    state.set_balance(owner, &denom, owner_new);
                    state.set_balance(&beneficiary, &denom, beneficiary_new);
                }
            }
            Ok(())
        }
    }
}

/// Execute a single-message transaction (legacy path for backward compatibility)
fn execute_single_message_transaction(
    tx: &Transaction,
    state: &mut State,
    block_height: u64,
    tx_index: u32,
    gas_schedule: &GasSchedule,
    fee_burn_engine: Option<&mut FeeBurnEngine>,
) -> ExecutionResult {
    // Step 1: Validate basic transaction fields
    if let Err(error) = validate_transaction(tx, state) {
        return ExecutionResult {
            receipt: create_failed_receipt(tx, 0, 0, 0, error.to_string(), block_height, tx_index),
            state_changes: Vec::new(),
            gas_used: 0,
            success: false,
        };
    }

    // Step 2: Determine gas parameters (backward compatibility)
    let (gas_limit, gas_price) = if tx.gas_limit > 0 && tx.gas_price > 0 {
        // New gas-aware transaction
        (tx.gas_limit, tx.gas_price)
    } else {
        // Legacy transaction: treat fee as gas_limit with gas_price=1
        (tx.fee as u64, 1u64)
    };

    // Step 3: Create execution context
    let mut ctx = ExecutionContext::new(gas_limit, gas_price);

    // Step 4: Calculate and deduct upfront fee (only fee is required upfront)
    let upfront_fee = match ctx.calculate_upfront_fee() {
        Ok(fee) => fee,
        Err(error) => {
            return ExecutionResult {
                receipt: create_failed_receipt(
                    tx,
                    0,
                    gas_limit,
                    gas_price,
                    error.to_string(),
                    block_height,
                    tx_index,
                ),
                state_changes: Vec::new(),
                gas_used: 0,
                success: false,
            };
        }
    };

    let sender_balance = state.balance_of(&tx.from, "udgt");

    if sender_balance < upfront_fee {
        let err_msg =
            format!("InsufficientFunds: required {upfront_fee}, available {sender_balance}");
        return ExecutionResult {
            receipt: create_failed_receipt(
                tx,
                0,
                gas_limit,
                gas_price,
                err_msg,
                block_height,
                tx_index,
            ),
            state_changes: Vec::new(),
            gas_used: 0,
            success: false,
        };
    }

    // Step 5: Deduct upfront fee immediately (non-revertible on failure)
    let new_sender_balance = sender_balance - upfront_fee;
    state.set_balance(&tx.from, "udgt", new_sender_balance);

    // Consume the nonce once the transaction has passed validation and paid the upfront fee.
    // This matches common blockchain semantics (e.g., Ethereum): nonce is consumed even if execution fails.
    state.increment_nonce(&tx.from);

    // Step 5.1: Consume minimal overhead gas to ensure non-zero gas_used in OOG scenarios
    if let Err(_gas_error) = ctx.consume_gas(1, "tx_overhead") {
        // Out of gas - nothing to revert (fee is non-revertible), return failure
        return ExecutionResult {
            receipt: create_failed_receipt(
                tx,
                ctx.gas_used(),
                gas_limit,
                gas_price,
                "OutOfGas".to_string(),
                block_height,
                tx_index,
            ),
            state_changes: Vec::new(),
            gas_used: ctx.gas_used(),
            success: false,
        };
    }

    // Step 6: Calculate intrinsic gas and charge it
    let tx_size = estimate_transaction_size(tx);
    let intrinsic_gas = match intrinsic_gas(&TxKind::Transfer, tx_size, 1, gas_schedule) {
        Ok(gas) => gas,
        Err(error) => {
            // Fail but keep upfront fee
            return ExecutionResult {
                receipt: create_failed_receipt(
                    tx,
                    ctx.gas_used(),
                    gas_limit,
                    gas_price,
                    error.to_string(),
                    block_height,
                    tx_index,
                ),
                state_changes: Vec::new(),
                gas_used: ctx.gas_used(),
                success: false,
            };
        }
    };

    if let Err(_gas_error) = ctx.consume_gas(intrinsic_gas, "intrinsic") {
        // Out of gas - revert any execution changes but keep fee
        ctx.revert_state_changes(state);
        return ExecutionResult {
            receipt: create_failed_receipt(
                tx,
                ctx.gas_used(),
                gas_limit,
                gas_price,
                "OutOfGas".to_string(),
                block_height,
                tx_index,
            ),
            state_changes: Vec::new(),
            gas_used: ctx.gas_used(),
            success: false,
        };
    }

    // Step 7: Execute the actual transfer
    if let Err(_gas_error) = execute_transfer(tx, state, &mut ctx) {
        // Out of gas during execution - revert state but keep fee
        ctx.revert_state_changes(state);
        return ExecutionResult {
            receipt: create_failed_receipt(
                tx,
                ctx.gas_used(),
                gas_limit,
                gas_price,
                "OutOfGas".to_string(),
                block_height,
                tx_index,
            ),
            state_changes: Vec::new(),
            gas_used: ctx.gas_used(),
            success: false,
        };
    }

    // Step 8: Success - Process fee burning if enabled
    if let Some(engine) = fee_burn_engine {
        let _ = engine.process_fee_burn(
            tx.hash.clone(),
            block_height,
            upfront_fee,
            state,
        );
        // Note: Fee burning errors are non-fatal and don't affect transaction success
    }

    // Step 9: Commit state changes and create success receipt
    let state_changes = ctx.state_changes.clone();
    ExecutionResult {
        success: true,
        state_changes,
        gas_used: ctx.gas_used(),
        receipt: create_success_receipt(
            tx,
            ctx.gas_used(),
            gas_limit,
            gas_price,
            block_height,
            tx_index,
        ),
    }
}

/// Validate basic transaction fields
fn validate_transaction(tx: &Transaction, state: &mut State) -> Result<(), ExecutionError> {
    let current = state.nonce_of(&tx.from);
    if current != tx.nonce {
        return Err(ExecutionError::InvalidNonce {
            expected: current,
            actual: tx.nonce,
        });
    }
    Ok(())
}

/// Execute the transfer operation with gas metering
fn execute_transfer(
    tx: &Transaction,
    state: &mut State,
    ctx: &mut ExecutionContext,
) -> Result<(), GasError> {
    // Charge gas for KV operations
    ctx.consume_gas(40, "kv_read_from")?; // Read sender balance
    ctx.consume_gas(40, "kv_read_to")?; // Read recipient balance
    ctx.consume_gas(120, "kv_write_from")?; // Write sender balance
    ctx.consume_gas(120, "kv_write_to")?; // Write recipient balance

    // Use the denom from the transaction (defaults to "udgt" for backward compatibility)
    let denom = &tx.denom;

    // Record the transfer state changes
    let sender_old_balance = state.balance_of(&tx.from, denom);
    let recipient_old_balance = state.balance_of(&tx.to, denom);

    // NOTE: We assume sufficient funds after upfront fee; future work can add explicit checks
    let sender_new_balance = sender_old_balance - tx.amount;
    let recipient_new_balance = recipient_old_balance + tx.amount;

    ctx.record_state_change(
        tx.from.clone(),
        denom.to_string(),
        sender_old_balance,
        sender_new_balance,
    );
    ctx.record_state_change(
        tx.to.clone(),
        denom.to_string(),
        recipient_old_balance,
        recipient_new_balance,
    );

    // Apply the transfer
    state.set_balance(&tx.from, denom, sender_new_balance);
    state.set_balance(&tx.to, denom, recipient_new_balance);

    Ok(())
}

/// Estimate transaction size for gas calculation
fn estimate_transaction_size(tx: &Transaction) -> usize {
    // Simplified estimation - in practice would serialize the transaction
    tx.hash.len() + tx.from.len() + tx.to.len() + 64 // rough estimate
}

/// Create a success receipt
fn create_success_receipt(
    tx: &Transaction,
    gas_used: Gas,
    gas_limit: Gas,
    gas_price: u64,
    block_height: u64,
    index: u32,
) -> TxReceipt {
    TxReceipt {
        receipt_version: RECEIPT_FORMAT_VERSION,
        tx_hash: tx.hash.clone(),
        status: TxStatus::Success,
        block_height: Some(block_height),
        index: Some(index),
        from: tx.from.clone(),
        to: tx.to.clone(),
        amount: tx.amount,
        fee: tx.fee,
        nonce: tx.nonce,
        error: None,
        gas_used,
        gas_limit,
        gas_price,
        gas_refund: 0, // Always 0 as per spec
        success: true,
    }
}

/// Create a failed receipt
fn create_failed_receipt(
    tx: &Transaction,
    gas_used: Gas,
    gas_limit: Gas,
    gas_price: u64,
    error: String,
    block_height: u64,
    index: u32,
) -> TxReceipt {
    TxReceipt {
        receipt_version: RECEIPT_FORMAT_VERSION,
        tx_hash: tx.hash.clone(),
        status: TxStatus::Failed,
        block_height: Some(block_height),
        index: Some(index),
        from: tx.from.clone(),
        to: tx.to.clone(),
        amount: tx.amount,
        fee: tx.fee,
        nonce: tx.nonce,
        error: Some(error),
        gas_used,
        gas_limit,
        gas_price,
        gas_refund: 0, // Always 0 as per spec
        success: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::state::Storage;
    use std::sync::Arc;

    fn create_test_state() -> State {
        // Use a unique temporary directory per test to avoid cross-test contamination
        let dir = tempfile::tempdir().expect("tempdir");
        let storage = Arc::new(Storage::open(dir.path().join("state.db")).unwrap());
        State::new(storage)
    }

    #[test]
    fn test_upfront_fee_calculation() {
        let ctx = ExecutionContext::new(25000, 1500);
        let fee = ctx.calculate_upfront_fee().unwrap();
        assert_eq!(fee, 37_500_000); // 25000 * 1500
    }

    #[test]
    fn test_upfront_fee_overflow() {
        let ctx = ExecutionContext::new(u64::MAX, u64::MAX);
        // Multiplying two u64 values fits into u128 without overflow
        let fee = ctx
            .calculate_upfront_fee()
            .expect("no overflow for u64::MAX * u64::MAX");
        let expected = (u128::from(u64::MAX)) * (u128::from(u64::MAX));
        assert_eq!(fee, expected);
    }

    #[test]
    fn test_successful_execution() {
        let mut state = create_test_state();
        let gas_schedule = GasSchedule::default();

        // Setup initial state
        state.set_balance("alice", "udgt", 100_000);
        state.set_balance("bob", "udgt", 50_000);

        let tx = Transaction::new(
            "test_hash".to_string(),
            "alice".to_string(),
            "bob".to_string(),
            1_000,
            10_000,
            0,
            Some("sig".to_string()),
        )
        .with_gas(25_000, 1);

        let result = execute_transaction(&tx, &mut state, 100, 0, &gas_schedule, None);

        assert!(result.success);
        assert_eq!(result.receipt.status, TxStatus::Success);
        assert!(result.gas_used > 0);
        assert_eq!(result.receipt.gas_limit, 25_000);
        assert_eq!(result.receipt.gas_price, 1);
    }

    #[test]
    fn test_insufficient_funds() {
        let mut state = create_test_state();
        let gas_schedule = GasSchedule::default();

        // Setup insufficient balance
        state.set_balance("alice", "udgt", 1_000); // Not enough for amount + gas

        let tx = Transaction::new(
            "test_hash".to_string(),
            "alice".to_string(),
            "bob".to_string(),
            1_000,
            10_000,
            0,
            Some("sig".to_string()),
        )
        .with_gas(25_000, 1_000); // High gas price

        let result = execute_transaction(&tx, &mut state, 100, 0, &gas_schedule, None);

        assert!(!result.success);
        assert_eq!(result.receipt.status, TxStatus::Failed);
        assert!(result
            .receipt
            .error
            .as_ref()
            .unwrap()
            .contains("InsufficientFunds"));
    }
}
