use wasmlanche::{public, Context, Address};
use tee_interface::prelude::*;
use borsh::{BorshSerialize, BorshDeserialize};

mod state;
mod executor;
mod types;

use state::*;
use types::*;

#[public]
pub fn init(context: &mut Context) -> Result<(), ContractError> {
    // Ensure not already initialized
    if context.get(SystemInitialized())?.unwrap_or(false) {
        return Err(ContractError::AlreadyInitialized);
    }

    // Initialize contract state
    context.store((
        (SystemInitialized(), true),
        (CurrentPhase(), Phase::Registration),
        (ExecutorCount(), 0),
    ))?;

    Ok(())
}

#[public]
pub fn register_executor(
    context: &mut Context,
    enclave_type: EnclaveType,
    attestation: AttestationReport,
) -> Result<(), ContractError> {
    ensure_initialized(context)?;
    ensure_phase(context, Phase::Registration)?;

    let caller = context.actor();

    // Verify attestation
    executor::verify_attestation(context, &attestation)?;

    // Create executor metadata
    let metadata = ExecutorMetadata {
        address: caller,
        enclave_type,
        status: ExecutorStatus::Active,
        last_attestation: context.timestamp(),
        execution_count: 0,
    };

    // Store executor data
    context.store((
        (Executor(caller), metadata),
        (ExecutorCount(), context.get(ExecutorCount())?.unwrap_or(0) + 1),
    ))?;

    Ok(())
}

#[public]
pub fn submit_result(
    context: &mut Context,
    result: ExecutionResult,
) -> Result<(), ContractError> {
    ensure_initialized(context)?;
    ensure_phase(context, Phase::Active)?;

    let caller = context.actor();

    // Verify caller is registered executor
    let executor = context.get(Executor(caller))?
        .ok_or(ContractError::NotRegistered)?;

    if executor.status != ExecutorStatus::Active {
        return Err(ContractError::NotActive);
    }

    // Store result
    store_execution_result(context, &result)?;

    // Check if we can verify results
    verify_results(context, result.execution_id)?;

    Ok(())
}

#[derive(Debug, BorshSerialize, BorshDeserialize)]
pub enum ContractError {
    NotInitialized,
    AlreadyInitialized,
    InvalidPhase,
    NotRegistered,
    NotActive,
    InvalidAttestation(String),
    InvalidResult(String),
    StorageError(String),
}

impl From<wasmlanche::Error> for ContractError {
    fn from(err: wasmlanche::Error) -> Self {
        ContractError::StorageError(err.to_string())
    }
}

/// Verify that contract is initialized
fn ensure_initialized(context: &Context) -> Result<(), ContractError> {
    if !context.get(SystemInitialized())?.unwrap_or(false) {
        return Err(ContractError::NotInitialized);
    }
    Ok(())
}

/// Verify contract is in expected phase
fn ensure_phase(context: &Context, expected: Phase) -> Result<(), ContractError> {
    let current = context.get(CurrentPhase())?.unwrap_or(Phase::Registration);
    if current != expected {
        return Err(ContractError::InvalidPhase);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialization() {
        let mut context = wasmlanche::testing::TestContext::new();
        
        // Test initialization
        assert!(init(&mut context).is_ok());
        
        // Test double initialization fails
        assert!(matches!(
            init(&mut context),
            Err(ContractError::AlreadyInitialized)
        ));
    }

    #[test]
    fn test_executor_registration() {
        let mut context = wasmlanche::testing::TestContext::new();
        
        // Initialize contract
        init(&mut context).unwrap();
        
        // Create test attestation
        let attestation = AttestationReport {
            enclave_type: EnclaveType::IntelSGX,
            measurement: [1u8; 32],
            timestamp: context.timestamp(),
            platform_data: vec![],
        };
        
        // Test registration
        assert!(register_executor(
            &mut context,
            EnclaveType::IntelSGX,
            attestation
        ).is_ok());
    }
}