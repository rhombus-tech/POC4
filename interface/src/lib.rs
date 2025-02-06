use serde::{Deserialize, Serialize};
use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

pub mod types {
    use super::*;
    use std::fmt;

    #[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
    pub struct ExecutionStats {
        pub execution_time: u64,
        pub memory_used: u64,
        pub syscall_count: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
    pub struct ExecutionParams {
        pub id_to: String,
        pub function_call: String,
        pub detailed_proof: bool,
        pub expected_hash: Vec<u8>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
    pub struct ExecutionPayload {
        pub input: Vec<u8>,
        pub params: ExecutionParams,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
    pub struct ExecutionResult {
        pub result: Vec<u8>,
        pub state_hash: Vec<u8>,
        pub stats: ExecutionStats,
        pub attestations: Vec<TeeAttestation>,
        pub timestamp: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
    pub struct TeeConfig {
        pub region_id: String,
        pub max_memory: usize,
        pub max_execution_time: u64,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
    pub struct Region {
        pub id: String,
        pub worker_ids: Vec<String>,
        pub max_tasks: u32,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
    pub enum TeeType {
        SGX,
        SEV,
    }

    impl fmt::Display for TeeType {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                TeeType::SGX => write!(f, "SGX"),
                TeeType::SEV => write!(f, "SEV"),
            }
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
    pub struct TeeAttestation {
        pub enclave_id: Vec<u8>,
        pub measurement: Vec<u8>,
        pub timestamp: u64,
        pub data: Vec<u8>,
        pub signature: Vec<u8>,
        pub region_proof: Option<Vec<u8>>,
        pub enclave_type: TeeType,
    }
}

pub use types::*;

#[derive(Error, Debug)]
pub enum TeeError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Execution error: {0}")]
    ExecutionError(String),
    #[error("Region error: {0}")]
    Region(String),
    #[error("Attestation error: {0}")]
    Attestation(String),
    #[error("Configuration error: {0}")]
    Config(String),
    #[error("Contract error: {0}")]
    Contract(String),
}

pub mod prelude {
    pub use super::types::*;
    pub use super::TeeError;
}

#[cfg_attr(feature = "async", async_trait::async_trait)]
pub trait TeeController: Send + Sync {
    async fn execute(
        &self,
        payload: &ExecutionPayload,
    ) -> Result<ExecutionResult, TeeError>;

    async fn get_config(&self) -> Result<TeeConfig, TeeError>;
    
    async fn update_config(
        &self,
        new_config: TeeConfig,
    ) -> Result<(), TeeError>;
}

#[cfg_attr(feature = "async", async_trait::async_trait)]
pub trait TeeExecutor: Send + Sync {
    async fn execute(
        &self,
        payload: &ExecutionPayload,
    ) -> Result<ExecutionResult, TeeError>;

    async fn get_regions(&self) -> Result<Vec<Region>, TeeError>;

    async fn get_attestations(
        &self,
        region_id: &str,
    ) -> Result<Vec<TeeAttestation>, TeeError>;

    async fn deploy_contract(
        &self,
        wasm_code: &[u8],
        region_id: &str,
    ) -> Result<String, TeeError>;

    async fn get_state_hash(
        &self,
        contract_address: &str,
    ) -> Result<Vec<u8>, TeeError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tee_type_display() {
        assert_eq!(TeeType::SGX.to_string(), "SGX");
        assert_eq!(TeeType::SEV.to_string(), "SEV");
    }
}