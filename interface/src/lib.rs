use async_trait::async_trait;
use borsh::{BorshSerialize, BorshDeserialize};
use thiserror::Error;

pub mod types;
pub use types::*;

#[derive(Debug, Error, BorshSerialize, BorshDeserialize)]
pub enum TeeError {
    #[error("Attestation verification failed: {0}")]
    AttestationError(String),
    #[error("Execution failed: {0}")]
    ExecutionError(String),
    #[error("Invalid state: {0}")]
    StateError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
    #[error("Verification failed: {0}")]
    VerificationError(String),
}

/// Trait for TEE verification logic
#[async_trait]
pub trait TeeVerification: Send + Sync {
    /// Verify a TEE attestation
    async fn verify_attestation(&self, attestation: &TeeAttestation) -> Result<bool, TeeError>;

    /// Verify execution result matches attestation
    async fn verify_result(&self, result: &ExecutionResult) -> Result<bool, TeeError>;
}

/// Main controller for TEE execution
#[async_trait]
pub trait TeeController: Send + Sync {
    /// Initialize TEE environment
    async fn init(&mut self) -> Result<(), TeeError>;

    /// Execute code in TEE
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError>;

    /// Get current TEE configuration
    async fn get_config(&self) -> Result<TeeConfig, TeeError>;

    /// Update TEE configuration
    async fn update_config(&mut self, config: TeeConfig) -> Result<(), TeeError>;
}

/// Configuration for TEE operations
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub struct TeeConfig {
    /// Minimum number of attestations required
    pub min_attestations: u32,
    /// Whether to verify measurements
    pub verify_measurements: bool,
}

impl Default for TeeConfig {
    fn default() -> Self {
        Self {
            min_attestations: 1,
            verify_measurements: true,
        }
    }
}

/// Prelude module containing commonly used types and traits
pub mod prelude {
    pub use super::{
        TeeVerification,
        TeeController,
        TeeConfig,
        TeeError,
    };
    pub use super::types::*;
    pub use borsh::{BorshSerialize, BorshDeserialize};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = TeeConfig::default();
        let bytes = borsh::to_vec(&config).unwrap();
        let decoded: TeeConfig = borsh::from_slice(&bytes).unwrap();
        assert_eq!(decoded.min_attestations, config.min_attestations);
        assert_eq!(decoded.verify_measurements, config.verify_measurements);
    }
}