use wasmlanche::{
    public, state_schema, Address, Context,
    borsh::{BorshSerialize, BorshDeserialize}
};
use tee_interface::prelude::*;

state_schema! {
    /// Address of deployed accumulator contract
    AccumulatorContract => Address,
    /// Configuration parameters
    Parameters => VerifierParams,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub struct VerifierParams {
    pub window_size: u64,
    pub min_attestations: u64,
}

#[public]
pub fn init(
    context: &mut Context, 
    accumulator: Address,
    params: VerifierParams,
) -> Result<(), TeeError> {
    if context.get(AccumulatorContract)?.is_some() {
        return Err(TeeError::InitializationError("Already initialized".into()));
    }

    context.store((
        (AccumulatorContract, accumulator),
        (Parameters, params),
    ))?;

    Ok(())
}

#[public]
pub fn verify_execution(
    context: &mut Context,
    sgx_result: &ExecutionResult,
    sev_result: &ExecutionResult,
) -> Result<VerificationResult, TeeError> {
    // Verify results match
    if sgx_result.result_hash != sev_result.result_hash {
        return Err(TeeError::ResultMismatch);
    }

    // Get accumulator contract
    let accumulator = context.get(AccumulatorContract)?
        .ok_or(TeeError::InitializationError("Not initialized".into()))?;

    // Call accumulator for verification
    let witness_valid: bool = context.call_contract(
        accumulator,
        "verify_attestation",
        &(sgx_result.attestation.clone(), sev_result.attestation.clone()),
        context.remaining_fuel(),
        0,
    )?;

    if !witness_valid {
        return Err(TeeError::AttestationError("Invalid accumulator proof".into()));
    }

    Ok(VerificationResult {
        verified: true,
        result_hash: sgx_result.result_hash,
        sgx_attestation: sgx_result.attestation.clone(),
        sev_attestation: sev_result.attestation.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmlanche::simulator::{Simulator, SimpleState};

    fn setup() -> (Simulator, Address, Address) {
        let mut state = SimpleState::new();
        let mut sim = Simulator::new(&mut state);
        
        let executor = Address::new([1; 33]);
        let accumulator = Address::new([2; 33]);
        sim.set_actor(executor);

        let params = VerifierParams {
            window_size: 1000,
            min_attestations: 2,
        };

        let ctx = &mut sim;
        init(ctx, accumulator, params).unwrap();

        (sim, executor, accumulator) 
    }

    #[test]
    fn test_verification_flow() {
        let (mut sim, executor, _) = setup();
        let ctx = &mut sim;

        let sgx_attestation = AttestationReport {
            enclave_type: EnclaveType::IntelSGX,
            measurement: [1; 32],
            timestamp: ctx.timestamp(),
            platform_data: vec![1],
        };

        let sev_attestation = AttestationReport {
            enclave_type: EnclaveType::AMDSEV,
            measurement: [2; 32],
            timestamp: ctx.timestamp(),
            platform_data: vec![2],
        };

        // Create sample results
        let result_hash = [3; 32];
        let sgx_result = ExecutionResult {
            result_hash,
            result: vec![1],
            attestation: sgx_attestation,
        };

        let sev_result = ExecutionResult {
            result_hash,
            result: vec![1],
            attestation: sev_attestation,
        };

        // Mock accumulator verification
        sim.mock_function_call(
            executor, 
            "verify_attestation",
            (sgx_result.attestation.clone(), sev_result.attestation.clone()),
            0,
            true
        );

        let result = verify_execution(ctx, &sgx_result, &sev_result).unwrap();
        assert!(result.verified);
    }
}