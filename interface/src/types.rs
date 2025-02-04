use borsh::{BorshSerialize, BorshDeserialize};

/// Input payload for TEE execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionPayload {
    /// Unique identifier for this execution
    pub execution_id: u64,
    /// Input data to be processed
    pub input: Vec<u8>,
    /// Any additional parameters needed for execution
    pub params: ExecutionParams,
}

/// Parameters for execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionParams {
    /// Expected result hash if doing verification
    pub expected_hash: Option<[u8; 32]>,
    /// Whether to include detailed proof
    pub detailed_proof: bool,
}

impl Default for ExecutionParams {
    fn default() -> Self {
        Self {
            expected_hash: None,
            detailed_proof: false,
        }
    }
}

/// State of the TEE execution
#[derive(Debug, Clone, Copy, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum ExecutionState {
    /// Not yet started
    Pending,
    /// Currently executing
    Running,
    /// Successfully completed
    Completed,
    /// Failed to execute
    Failed,
}

/// Platform-specific measurements
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub enum PlatformMeasurement {
    /// Intel SGX measurements
    Sgx {
        /// MRENCLAVE measurement
        mrenclave: [u8; 32],
        /// MRSIGNER measurement
        mrsigner: [u8; 32],
        /// MISCSELECT value
        miscselect: u32,
        /// SGX attributes
        attributes: [u8; 16],
    },
    /// AMD SEV measurements
    Sev {
        /// Platform measurement
        measurement: [u8; 32],
        /// Platform information
        platform_info: [u8; 32],
        /// Launch digest
        launch_digest: [u8; 32],
    },
}

/// Proof of execution from TEE
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionProof {
    /// Hash of the computation result
    pub result_hash: [u8; 32],
    /// Attestation from the TEE
    pub attestation: TeeAttestation,
    /// Platform-specific measurements
    pub platform_measurement: PlatformMeasurement,
}

/// Configuration for TEE execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct TeeConfig {
    /// Type of TEE to use
    pub tee_type: TeeType,
    /// Memory size in bytes
    pub memory_size: usize,
    /// Number of CPU cores
    pub num_cores: u32,
}

impl Default for TeeConfig {
    fn default() -> Self {
        Self {
            tee_type: TeeType::Sgx,
            memory_size: 1024 * 1024 * 1024, // 1GB
            num_cores: 1,
        }
    }
}

/// Statistics about TEE execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionStats {
    /// Time taken for execution in milliseconds
    pub execution_time_ms: u64,
    /// Memory usage in bytes
    pub memory_usage: usize,
}

/// Constants for execution
pub mod constants {
    /// Maximum input size (10MB)
    pub const MAX_INPUT_SIZE: usize = 10 * 1024 * 1024;
    /// Maximum output size (10MB)
    pub const MAX_OUTPUT_SIZE: usize = 10 * 1024 * 1024;
    /// Default memory size (8MB)
    pub const DEFAULT_MEMORY_SIZE: usize = 8 * 1024 * 1024;
    /// Default stack size (8MB)
    pub const DEFAULT_STACK_SIZE: usize = 8 * 1024 * 1024;
}

/// Result of TEE execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionResult {
    /// Transaction ID
    pub tx_id: Vec<u8>,
    /// Hash of final state
    pub state_hash: [u8; 32],
    /// Output data
    pub output: Vec<u8>,
    /// Attestations from TEEs
    pub attestations: [TeeAttestation; 2],
    /// Timestamp of execution
    pub timestamp: u64,
    /// Region ID where executed
    pub region_id: String,
}

/// Verification result when comparing TEE executions
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct VerificationResult {
    /// Whether verification passed
    pub verified: bool,
    /// Hash of the verified result
    pub result_hash: [u8; 32],
    /// Attestations from both TEEs
    pub attestations: Vec<TeeAttestation>,
    /// Timestamp of verification
    pub timestamp: u64,
}

/// Attestation from a TEE
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct TeeAttestation {
    /// TEE type (SGX/SEV)
    pub tee_type: TeeType,
    /// Platform-specific measurement data
    pub measurement: PlatformMeasurement,
    /// Signature over measurement
    pub signature: Vec<u8>,
}

/// Type of TEE environment
#[derive(Debug, Clone, Copy, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum TeeType {
    /// Intel SGX
    Sgx,
    /// AMD SEV
    Sev,
}

/// Input for TEE execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionInput {
    /// WASM module bytes
    pub wasm_bytes: Vec<u8>,
    /// Function to call
    pub function: String,
    /// Arguments to pass
    pub args: Vec<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use borsh::BorshSerialize;

    #[test]
    fn test_payload_serialization() {
        let payload = ExecutionPayload {
            execution_id: 1,
            input: vec![1, 2, 3],
            params: ExecutionParams {
                expected_hash: Some([0u8; 32]),
                detailed_proof: true,
            },
        };

        let serialized = payload.try_to_vec().unwrap();
        let deserialized: ExecutionPayload = borsh::BorshDeserialize::try_from_slice(&serialized).unwrap();

        assert_eq!(payload.execution_id, deserialized.execution_id);
        assert_eq!(payload.input, deserialized.input);
        assert_eq!(payload.params.detailed_proof, deserialized.params.detailed_proof);
    }

    #[test]
    fn test_platform_measurement() {
        let measurement = PlatformMeasurement::Sgx {
            mrenclave: [0u8; 32],
            mrsigner: [1u8; 32],
            miscselect: 0,
            attributes: [0u8; 16],
        };

        let serialized = measurement.try_to_vec().unwrap();
        let deserialized: PlatformMeasurement = borsh::BorshDeserialize::try_from_slice(&serialized).unwrap();

        match (measurement, deserialized) {
            (
                PlatformMeasurement::Sgx {
                    mrenclave: e1,
                    mrsigner: s1,
                    miscselect: m1,
                    attributes: a1,
                },
                PlatformMeasurement::Sgx {
                    mrenclave: e2,
                    mrsigner: s2,
                    miscselect: m2,
                    attributes: a2,
                },
            ) => {
                assert_eq!(e1, e2);
                assert_eq!(s1, s2);
                assert_eq!(m1, m2);
                assert_eq!(a1, a2);
            }
            _ => panic!("Unexpected variant"),
        }
    }
}