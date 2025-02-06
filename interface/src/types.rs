use borsh::{BorshSerialize, BorshDeserialize};
use serde::{Serialize, Deserialize};

/// Input payload for TEE execution
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecutionPayload {
    /// Unique identifier for this execution
    pub execution_id: u64,
    /// Input data to be processed
    pub input: Vec<u8>,
    /// Any additional parameters needed for execution
    pub params: ExecutionParams,
}

impl Default for ExecutionPayload {
    fn default() -> Self {
        Self {
            execution_id: 0,
            input: Vec::new(),
            params: ExecutionParams::default(),
        }
    }
}

/// Parameters for execution
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ExecutionParams {
    /// Expected result hash if doing verification
    pub expected_hash: Option<[u8; 32]>,
    /// Whether to include detailed proof
    pub detailed_proof: bool,
    /// Function to call in the WASM module
    pub function_call: String,
}

impl Default for ExecutionParams {
    fn default() -> Self {
        Self {
            expected_hash: None,
            detailed_proof: false,
            function_call: "execute".to_string(),
        }
    }
}

/// State of the TEE execution
#[derive(Debug, Clone, Copy, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum ExecutionState {
    /// Initial state
    Initial,
    /// Executing in TEE
    Running,
    /// Execution completed successfully
    Completed,
    /// Execution failed
    Failed,
}

/// Proof of execution from TEE
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionProof {
    /// Hash of input data
    pub input_hash: [u8; 32],
    /// Hash of output data
    pub output_hash: [u8; 32],
    /// Detailed execution trace if requested
    pub trace: Option<Vec<u8>>,
}

/// Configuration for TEE execution
#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TeeConfig {
    /// Minimum number of attestations required
    pub min_attestations: u32,
    /// Whether to verify measurements
    pub verify_measurements: bool,
    /// Maximum input size
    pub max_input_size: usize,
    /// Maximum memory size
    pub max_memory_size: usize,
    /// Maximum execution time in milliseconds
    pub max_execution_time: u64,
    /// Maximum gas limit
    pub max_gas: u64,
}

impl Default for TeeConfig {
    fn default() -> Self {
        Self {
            min_attestations: 1,
            verify_measurements: true,
            max_input_size: 1024 * 1024, // 1MB
            max_memory_size: 1024 * 1024 * 1024, // 1GB
            max_execution_time: 60 * 1000, // 1 minute
            max_gas: 1000000,
        }
    }
}

/// Statistics about TEE execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionStats {
    /// Execution time in microseconds
    pub execution_time: u64,
    /// Memory used in bytes
    pub memory_used: u64,
    /// Number of syscalls made
    pub syscall_count: u64,
}

/// Information about a TEE region
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct RegionInfo {
    /// Region identifier
    pub id: String,
    /// List of worker IDs in this region
    pub worker_ids: Vec<String>,
    /// Maximum number of concurrent tasks
    pub max_tasks: u32,
}

/// Constants for execution
pub mod constants {
    /// Maximum input size (10MB)
    pub const MAX_INPUT_SIZE: usize = 10 * 1024 * 1024;
    /// Maximum execution time (5 minutes)
    pub const MAX_EXECUTION_TIME: u64 = 5 * 60;
    /// Maximum memory usage (1GB)
    pub const MAX_MEMORY_USAGE: u64 = 1024 * 1024 * 1024;
    /// Maximum syscalls per execution
    pub const MAX_SYSCALLS: u64 = 1000;
}

/// Result of TEE execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionResult {
    /// Output data from execution
    pub result: Vec<u8>,
    /// Attestation from TEE
    pub attestation: TeeAttestation,
    /// Hash of final state
    pub state_hash: Vec<u8>,
    /// Execution statistics
    pub stats: ExecutionStats,
}

/// Verification result when comparing TEE executions
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct VerificationResult {
    /// Whether executions match
    pub matches: bool,
    /// Hash mismatch if any
    pub hash_mismatch: Option<String>,
    /// Detailed comparison if requested
    pub details: Option<Vec<u8>>,
}

/// Attestation from a TEE
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct TeeAttestation {
    /// Unique identifier for the enclave
    pub enclave_id: [u8; 32],
    /// Platform measurement
    pub measurement: Vec<u8>,
    /// Attested data
    pub data: Vec<u8>,
    /// Signature over data
    pub signature: Vec<u8>,
    /// Optional proof of region membership
    pub region_proof: Option<Vec<u8>>,
    /// Timestamp of attestation
    pub timestamp: u64,
    /// Type of enclave
    pub enclave_type: TeeType,
}

/// Type of TEE environment
#[derive(Debug, Clone, Copy, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum TeeType {
    /// Intel SGX
    SGX,
    /// AMD SEV
    SEV,
}

/// Input for TEE execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionInput {
    /// Input data
    pub data: Vec<u8>,
    /// Expected output hash if verifying
    pub expected_hash: Option<[u8; 32]>,
    /// Whether to include detailed proof
    pub detailed_proof: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execution_payload_serialization() {
        let payload = ExecutionPayload {
            execution_id: 1,
            input: b"test".to_vec(),
            params: ExecutionParams {
                expected_hash: Some([0; 32]),
                detailed_proof: true,
                function_call: "test".to_string(),
            },
        };

        let serialized = borsh::to_vec(&payload).unwrap();
        let deserialized: ExecutionPayload = borsh::from_slice(&serialized).unwrap();

        assert_eq!(deserialized.execution_id, payload.execution_id);
        assert_eq!(deserialized.input, payload.input);
        assert_eq!(deserialized.params.expected_hash, payload.params.expected_hash);
        assert_eq!(deserialized.params.detailed_proof, payload.params.detailed_proof);
        assert_eq!(deserialized.params.function_call, payload.params.function_call);
    }

    #[test]
    fn test_platform_measurement() {
        let attestation = TeeAttestation {
            enclave_id: [0; 32],
            measurement: vec![1, 2, 3],
            data: vec![4, 5, 6],
            signature: vec![7, 8, 9],
            region_proof: Some(vec![10, 11, 12]),
            timestamp: 123,
            enclave_type: TeeType::SGX,
        };

        let serialized = borsh::to_vec(&attestation).unwrap();
        let deserialized: TeeAttestation = borsh::from_slice(&serialized).unwrap();

        assert_eq!(deserialized.enclave_id, attestation.enclave_id);
        assert_eq!(deserialized.measurement, attestation.measurement);
        assert_eq!(deserialized.data, attestation.data);
        assert_eq!(deserialized.signature, attestation.signature);
        assert_eq!(deserialized.region_proof, attestation.region_proof);
        assert_eq!(deserialized.timestamp, attestation.timestamp);
        assert_eq!(deserialized.enclave_type, attestation.enclave_type);
    }
}