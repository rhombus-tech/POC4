// accumulator/src/lib.rs

use wasmlanche::{public, Address, Context};
use tee_interface::prelude::*;
use borsh::{BorshSerialize, BorshDeserialize};

// Re-export the accumulator and verification modules
mod accumulator;
mod verification;

pub use accumulator::{
    AccumulatorParams,
    AccumulatorWitness,
    AccumulatorElement,
    AccumulatorState,
    AttestationRecord,
};

pub use verification::{
    VerificationResult,
    VerificationError,
};

// Contract functions that other contracts can call
#[public]
pub fn init(context: &mut Context, params: AccumulatorParams) -> Result<(), TeeError> {
    accumulator::init(context, params)
}

#[public]
pub fn register_attestation(
    context: &mut Context,
    attestation: AttestationReport,
) -> Result<(), TeeError> {
    accumulator::register_attestation(context, attestation)
}

#[public]
pub fn verify_attestation(
    context: &mut Context,
    sgx_attestation: AttestationReport,
    sev_attestation: AttestationReport,
) -> Result<bool, TeeError> {
    accumulator::verify_attestation(context, sgx_attestation, sev_attestation)
}

#[public]
pub fn verify_execution(
    context: &mut Context,
    sgx_result: &ExecutionResult,
    sev_result: &ExecutionResult,
) -> Result<VerificationResult, TeeError> {
    verification::verify_execution(context, sgx_result, sev_result)
}

// Re-export commonly used items in a prelude module
pub mod prelude {
    pub use super::{
        AccumulatorParams,
        AccumulatorWitness,
        AccumulatorElement,
        AccumulatorState,
        AttestationRecord,
        VerificationResult,
        VerificationError,
        init,
        register_attestation,
        verify_attestation,
        verify_execution,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmlanche::testing::TestContext;

    fn setup() -> (TestContext, AttestationReport, AttestationReport) {
        let mut context = TestContext::new();
        
        // Initialize with default params
        let params = AccumulatorParams {
            max_size: 1000,
            max_witness_age: 7 * 24 * 60 * 60, // 1 week
            min_attestations: 2,
        };
        init(&mut context, params).unwrap();

        // Create test attestations
        let sgx_attestation = AttestationReport {
            enclave_type: EnclaveType::IntelSGX,
            measurement: [1; 32],
            timestamp: context.timestamp(),
            platform_data: vec![],
        };

        let sev_attestation = AttestationReport {
            enclave_type: EnclaveType::AMDSEV,
            measurement: [2; 32],
            timestamp: context.timestamp(),
            platform_data: vec![],
        };

        (context, sgx_attestation, sev_attestation)
    }

    #[test]
    fn test_attestation_flow() {
        let (mut context, sgx_att, sev_att) = setup();

        // Register attestations
        register_attestation(&mut context, sgx_att.clone()).unwrap();
        register_attestation(&mut context, sev_att.clone()).unwrap();

        // Verify attestations
        let result = verify_attestation(&mut context, sgx_att, sev_att).unwrap();
        assert!(result);
    }

    #[test]
    fn test_execution_verification() {
        let (mut context, sgx_att, sev_att) = setup();

        // Register attestations first
        register_attestation(&mut context, sgx_att.clone()).unwrap();
        register_attestation(&mut context, sev_att.clone()).unwrap();

        // Create test execution results
        let sgx_result = ExecutionResult {
            result_hash: [0; 32],
            result: vec![1, 2, 3],
            attestation: sgx_att,
        };

        let sev_result = ExecutionResult {
            result_hash: [0; 32],
            result: vec![1, 2, 3],
            attestation: sev_att,
        };

        // Verify execution
        let verification = verify_execution(&mut context, &sgx_result, &sev_result).unwrap();
        assert!(verification.verified);
    }
}