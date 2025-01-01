use borsh::{BorshSerialize, BorshDeserialize};
use thiserror::Error;

mod types;
pub use types::*;

/// Represents the type of TEE environment
#[derive(Debug, Clone, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum EnclaveType {
    IntelSGX,
    AMDSEV,
}

/// Result of a TEE execution including proof
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionResult {
    /// Hash of the computation result
    pub result_hash: [u8; 32],
    /// Raw result data
    pub result: Vec<u8>,
    /// Attestation from the TEE
    pub attestation: AttestationReport,
}

/// Attestation information from a TEE
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct AttestationReport {
    /// Type of TEE that generated this attestation
    pub enclave_type: EnclaveType,
    /// Measurement of the code/data in the TEE
    pub measurement: [u8; 32],
    /// When this attestation was generated
    pub timestamp: u64,
    /// Platform-specific attestation data
    pub platform_data: Vec<u8>,
}

/// Verification result when comparing TEE executions
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct VerificationResult {
    /// Whether the executions match
    pub verified: bool,
    /// Hash of the matching result
    pub result_hash: [u8; 32],
    /// Attestations from both TEEs
    pub sgx_attestation: AttestationReport,
    pub sev_attestation: AttestationReport,
}

/// Errors that can occur during TEE operations
#[derive(Error, Debug)]
pub enum TeeError {
    #[error("TEE initialization failed: {0}")]
    InitializationError(String),

    #[error("Execution failed: {0}")]
    ExecutionError(String),

    #[error("Attestation failed: {0}")]
    AttestationError(String),

    #[error("Verification failed: {0}")]
    VerificationError(String),

    #[error("Results don't match between TEEs")]
    ResultMismatch,

    #[error("Invalid attestation: {0}")]
    InvalidAttestation(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] std::io::Error),
}

/// Trait for TEE verification logic
pub trait TeeVerification {
    /// Verify execution results match between TEEs
    fn verify_execution(
        sgx_result: &ExecutionResult,
        sev_result: &ExecutionResult,
    ) -> Result<VerificationResult, TeeError>;

    /// Verify attestation is valid
    fn verify_attestation(attestation: &AttestationReport) -> Result<bool, TeeError>;
}

// Re-export commonly used items
pub mod prelude {
    pub use super::{
        EnclaveType,
        ExecutionResult,
        AttestationReport,
        VerificationResult,
        TeeError,
        TeeVerification,
    };
    pub use borsh::{BorshSerialize, BorshDeserialize};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_result_serialization() {
        let result = ExecutionResult {
            result_hash: [0u8; 32],
            result: vec![1, 2, 3],
            attestation: AttestationReport {
                enclave_type: EnclaveType::IntelSGX,
                measurement: [0u8; 32],
                timestamp: 12345,
                platform_data: vec![],
            },
        };

        // Test Borsh serialization
        let serialized = borsh::to_vec(&result).unwrap();
        let deserialized: ExecutionResult = borsh::from_slice(&serialized).unwrap();
        assert_eq!(result.result_hash, deserialized.result_hash);
        assert_eq!(result.result, deserialized.result);
    }

    #[test]
    fn test_attestation_serialization() {
        let attestation = AttestationReport {
            enclave_type: EnclaveType::IntelSGX,
            measurement: [0u8; 32],
            timestamp: 12345,
            platform_data: vec![1, 2, 3],
        };

        let serialized = borsh::to_vec(&attestation).unwrap();
        let deserialized: AttestationReport = borsh::from_slice(&serialized).unwrap();
        assert_eq!(attestation.measurement, deserialized.measurement);
        assert_eq!(attestation.platform_data, deserialized.platform_data);
    }
}