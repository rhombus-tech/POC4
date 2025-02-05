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
    /// Transaction ID this result is for
    pub tx_id: Vec<u8>,
    /// Output data from execution
    pub output: Vec<u8>,
    /// Hash of final state
    pub state_hash: [u8; 32],
    /// TEE attestations
    pub attestations: Vec<TeeAttestation>,
    /// Timestamp of execution
    pub timestamp: u64,
    /// ID of region that executed
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
    /// Type of TEE
    pub tee_type: TeeType,
    /// Platform measurement/quote
    pub measurement: Vec<u8>,
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
    /// ARM TrustZone
    TrustZone,
}

/// Platform measurement types
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub enum PlatformMeasurement {
    /// SGX quote
    SgxQuote(Vec<u8>),
    /// SEV attestation report
    SevReport(Vec<u8>),
    /// TrustZone measurement
    TrustZoneMeasurement(Vec<u8>),
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

    #[test]
    fn test_execution_payload_serialization() {
        let payload = ExecutionPayload {
            execution_id: 1,
            input: b"test".to_vec(),
            params: ExecutionParams::default(),
        };

        let bytes = borsh::to_vec(&payload).unwrap();
        let decoded: ExecutionPayload = borsh::from_slice(&bytes).unwrap();

        assert_eq!(decoded.execution_id, payload.execution_id);
        assert_eq!(decoded.input, payload.input);
    }

    #[test]
    fn test_platform_measurement() {
        let measurement = PlatformMeasurement::SgxQuote(vec![1, 2, 3]);
        let bytes = borsh::to_vec(&measurement).unwrap();
        let decoded: PlatformMeasurement = borsh::from_slice(&bytes).unwrap();

        match decoded {
            PlatformMeasurement::SgxQuote(data) => assert_eq!(data, vec![1, 2, 3]),
            _ => panic!("Wrong variant"),
        }
    }
}