use thiserror::Error;
use async_trait::async_trait;
use borsh::{BorshSerialize, BorshDeserialize};

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

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error("Region error: {0}")]
    RegionError(String),
}

/// Result type for TEE operations
pub type Result<T> = std::result::Result<T, TeeError>;

/// Trait for TEE verification logic
#[async_trait]
pub trait TeeVerification {
    /// Verify execution results from two different TEEs match
    async fn verify_execution(
        &self,
        sgx_result: &ExecutionResult,
        sev_result: &ExecutionResult,
    ) -> Result<VerificationResult>;
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ContractState {
    pub state: Vec<u8>,
    pub timestamp: u64,
}

pub trait StateAccess {
    fn get_state(&self) -> &ContractState;
    fn get_state_mut(&mut self) -> &mut ContractState;
}

/// Main controller for TEE execution
#[async_trait]
pub trait TeeController: Send + Sync {
    /// Execute in TEE with region awareness
    async fn execute(
        &self,
        region_id: String,
        input: Vec<u8>,
        attestation_required: bool,
    ) -> Result<ExecutionResult>;
    
    /// Get TEE health status
    async fn health_check(&self) -> Result<bool>;

    /// Get contract state
    async fn get_state(&self, region_id: String, contract_id: [u8; 32]) -> Result<ContractState>;
}

/// Controller interface for TEE execution
#[async_trait]
pub trait NewTeeController: Send + Sync {
    /// Execute in TEE with region awareness
    async fn execute(
        &self,
        region_id: String,
        input: Vec<u8>,
        attestation_required: bool,
    ) -> Result<ExecutionResult>;
    
    /// Get TEE health status
    async fn health_check(&self) -> Result<bool>;
}

/// Prelude module containing commonly used types and traits
pub mod prelude {
    pub use super::{
        TeeError,
        TeeVerification,
        TeeController,
        NewTeeController,
        Result,
        ContractState,
        StateAccess,
    };
    pub use super::types::*;
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;

    fn create_test_attestation(tee_type: TeeType) -> TeeAttestation {
        let measurement = match tee_type {
            TeeType::Sgx => PlatformMeasurement::Sgx {
                mrenclave: [0u8; 32],
                mrsigner: [0u8; 32],
                miscselect: 0,
                attributes: [0u8; 16],
            },
            TeeType::Sev => PlatformMeasurement::Sev {
                measurement: [0u8; 32],
                platform_info: [0u8; 32],
                launch_digest: [0u8; 32],
            },
        };

        TeeAttestation {
            tee_type,
            measurement,
            signature: vec![0u8; 64],
        }
    }

    #[test]
    fn test_execution_result_serialization() {
        let sgx_attestation = create_test_attestation(TeeType::Sgx);
        let sev_attestation = create_test_attestation(TeeType::Sev);

        let result = ExecutionResult {
            tx_id: vec![1, 2, 3],
            state_hash: [0u8; 32],
            output: vec![4, 5, 6],
            attestations: [sgx_attestation, sev_attestation],
            timestamp: 12345,
            region_id: String::from("test"),
        };

        let serialized = result.try_to_vec().unwrap();
        let deserialized: ExecutionResult = borsh::BorshDeserialize::try_from_slice(&serialized).unwrap();

        assert_eq!(result.tx_id, deserialized.tx_id);
        assert_eq!(result.state_hash, deserialized.state_hash);
        assert_eq!(result.output, deserialized.output);
        assert_eq!(result.attestations.len(), 2);
        assert_eq!(result.timestamp, deserialized.timestamp);
        assert_eq!(result.region_id, deserialized.region_id);

        // Verify SGX attestation
        match deserialized.attestations[0].measurement {
            PlatformMeasurement::Sgx { mrenclave, mrsigner, .. } => {
                assert_eq!(mrenclave, [0u8; 32]);
                assert_eq!(mrsigner, [0u8; 32]);
            }
            _ => panic!("Expected SGX measurement"),
        }

        // Verify SEV attestation
        match deserialized.attestations[1].measurement {
            PlatformMeasurement::Sev { measurement, platform_info, launch_digest } => {
                assert_eq!(measurement, [0u8; 32]);
                assert_eq!(platform_info, [0u8; 32]);
                assert_eq!(launch_digest, [0u8; 32]);
            }
            _ => panic!("Expected SEV measurement"),
        }
    }

    #[test]
    fn test_attestation_serialization() {
        let attestation = create_test_attestation(TeeType::Sgx);

        let serialized = attestation.try_to_vec().unwrap();
        let deserialized: TeeAttestation = borsh::BorshDeserialize::try_from_slice(&serialized).unwrap();

        assert_eq!(attestation.tee_type, deserialized.tee_type);
        assert_eq!(attestation.signature, deserialized.signature);
        
        match (attestation.measurement, deserialized.measurement) {
            (PlatformMeasurement::Sgx { mrenclave: m1, mrsigner: s1, .. },
             PlatformMeasurement::Sgx { mrenclave: m2, mrsigner: s2, .. }) => {
                assert_eq!(m1, m2);
                assert_eq!(s1, s2);
            }
            _ => panic!("Expected SGX measurements"),
        }
    }

    #[test]
    fn test_sev_attestation_serialization() {
        let attestation = create_test_attestation(TeeType::Sev);

        let serialized = attestation.try_to_vec().unwrap();
        let deserialized: TeeAttestation = borsh::BorshDeserialize::try_from_slice(&serialized).unwrap();

        assert_eq!(attestation.tee_type, deserialized.tee_type);
        assert_eq!(attestation.signature, deserialized.signature);
        
        match (attestation.measurement, deserialized.measurement) {
            (PlatformMeasurement::Sev { measurement: m1, platform_info: p1, launch_digest: l1 },
             PlatformMeasurement::Sev { measurement: m2, platform_info: p2, launch_digest: l2 }) => {
                assert_eq!(m1, m2);
                assert_eq!(p1, p2);
                assert_eq!(l1, l2);
            }
            _ => panic!("Expected SEV measurements"),
        }
    }
}