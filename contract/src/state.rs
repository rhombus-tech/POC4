use wasmlanche::{state_schema, Address};
use tee_interface::prelude::*;
use crate::types::{Phase, ExecutorMetadata};

// System state
state_schema! {
    pub SystemInitialized => bool,
}

state_schema! {
    pub CurrentPhase => Phase,
}

state_schema! {
    pub ExecutorCount => u64,
}

// Executor state
state_schema! {
    pub Executor(pub Address) => ExecutorMetadata,
}

// Execution state - using bytes to avoid Copy/Pod requirements
state_schema! {
    pub ExecutionOutput(pub [u8; 32]) => Vec<u8>,
}

pub fn store_execution_result(context: &mut wasmlanche::Context, result: &ExecutionResult) -> Result<(), crate::ContractError> {
    // Hash tx_id to get fixed size key
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(&result.tx_id);
    let key = hasher.finalize().into();

    // Store result data
    context.store((
        (ExecutionOutput(key), result.output.clone()),
    ))?;
    Ok(())
}

pub fn verify_results(_context: &mut wasmlanche::Context, _tx_id: Vec<u8>) -> Result<(), crate::ContractError> {
    // In a real implementation, we would:
    // 1. Get results from multiple executors
    // 2. Compare their outputs
    // 3. Update state if consensus reached
    Ok(())
}
