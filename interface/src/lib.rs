use thiserror::Error;
use async_trait::async_trait;
use std::error::Error;

pub mod types;
pub use types::*;

/// Errors that can occur during TEE operations
#[derive(Error, Debug)]
pub enum TeeError {
    #[error("TEE initialization failed: {0}")]
    InitializationError(String),
    
    #[error("TEE execution failed: {0}")]
    ExecutionError(String),
    
    #[error("TEE attestation failed: {0}")]
    AttestationError(String),
    
    #[error("TEE verification failed: {0}")]
    VerificationError(String),
    
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

/// Trait for TEE verification logic
#[async_trait]
pub trait TeeVerification {
    /// Verify execution results from two different TEEs match
    async fn verify_execution(
        &self,
        sgx_result: &ExecutionResult,
        sev_result: &ExecutionResult,
    ) -> Result<VerificationResult, TeeError>;
}

/// Main controller for TEE execution
#[async_trait]
pub trait TeeController {
    /// Execute code in the TEE
    async fn execute(
        &self,
        input: ExecutionInput,
    ) -> Result<ExecutionResult, TeeError>;

    /// Health check for the TEE
    async fn health_check(&self) -> Result<bool, Box<dyn Error>>;
}

/// Prelude module containing commonly used types and traits
pub mod prelude {
    pub use super::{
        TeeError,
        TeeVerification,
        TeeController,
    };
    pub use super::types::*;
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;

    #[test]
    fn test_execution_result_serialization() {
        let result = ExecutionResult {
            tx_id: vec![1, 2, 3],
            state_hash: [0u8; 32],
            output: vec![4, 5, 6],
            attestations: vec![],
            timestamp: 12345,
            region_id: String::from("test"),
        };

        let serialized = result.try_to_vec().unwrap();
        let deserialized: ExecutionResult = borsh::BorshDeserialize::try_from_slice(&serialized).unwrap();

        assert_eq!(result.tx_id, deserialized.tx_id);
        assert_eq!(result.state_hash, deserialized.state_hash);
        assert_eq!(result.output, deserialized.output);
        assert!(result.attestations.is_empty() && deserialized.attestations.is_empty());
        assert_eq!(result.timestamp, deserialized.timestamp);
        assert_eq!(result.region_id, deserialized.region_id);
    }

    #[test]
    fn test_attestation_serialization() {
        let attestation = TeeAttestation {
            tee_type: TeeType::Sgx,
            measurement: vec![0u8; 32],
            signature: vec![1, 2, 3],
        };

        let serialized = attestation.try_to_vec().unwrap();
        let deserialized: TeeAttestation = borsh::BorshDeserialize::try_from_slice(&serialized).unwrap();

        assert_eq!(attestation.tee_type, deserialized.tee_type);
        assert_eq!(attestation.measurement, deserialized.measurement);
        assert_eq!(attestation.signature, deserialized.signature);
    }
}