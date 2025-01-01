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
    Sgx {
        mrenclave: [u8; 32],
        mrsigner: [u8; 32],
        miscselect: u32,
        attributes: u64,
    },
    Sev {
        measurement: [u8; 32],
        policy: u32,
        signature: Vec<u8>,
    },
}

/// Proof of execution from TEE
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionProof {
    /// Final state after execution
    pub state_hash: [u8; 32],
    /// Platform-specific measurements
    pub platform_measurement: PlatformMeasurement,
    /// Signatures or other cryptographic proofs
    pub signatures: Vec<u8>,
    /// Additional platform-specific data
    pub platform_data: Vec<u8>,
}

/// Configuration for TEE execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct TeeConfig {
    /// Type of TEE to use
    pub enclave_type: super::EnclaveType,
    /// Heap size for TEE
    pub heap_size: usize,
    /// Stack size for TEE
    pub stack_size: usize,
    /// Debug mode enabled
    pub debug: bool,
}

/// Statistics about TEE execution
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutionStats {
    /// Time taken for execution (in milliseconds)
    pub execution_time: u64,
    /// Memory used during execution
    pub memory_used: usize,
    /// Number of system calls made
    pub syscall_count: u64,
}

/// Constants for execution
pub mod constants {
    /// Maximum input size (10MB)
    pub const MAX_INPUT_SIZE: usize = 10 * 1024 * 1024;
    
    /// Maximum proof size (1MB)
    pub const MAX_PROOF_SIZE: usize = 1024 * 1024;
    
    /// Default heap size (64MB)
    pub const DEFAULT_HEAP_SIZE: usize = 64 * 1024 * 1024;
    
    /// Default stack size (8MB)
    pub const DEFAULT_STACK_SIZE: usize = 8 * 1024 * 1024;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payload_serialization() {
        let payload = ExecutionPayload {
            execution_id: 1,
            input: vec![1, 2, 3],
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: false,
            },
        };

        let serialized = borsh::to_vec(&payload).unwrap();
        let deserialized: ExecutionPayload = borsh::from_slice(&serialized).unwrap();

        assert_eq!(payload.execution_id, deserialized.execution_id);
        assert_eq!(payload.input, deserialized.input);
    }

    #[test]
    fn test_platform_measurement() {
        let sgx_measurement = PlatformMeasurement::Sgx {
            mrenclave: [0; 32],
            mrsigner: [0; 32],
            miscselect: 0,
            attributes: 0,
        };

        let serialized = borsh::to_vec(&sgx_measurement).unwrap();
        let deserialized: PlatformMeasurement = borsh::from_slice(&serialized).unwrap();

        matches!(deserialized, PlatformMeasurement::Sgx { .. });
    }
}