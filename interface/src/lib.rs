use thiserror::Error;

#[cfg(feature = "async")]
use async_trait::async_trait;

pub mod types;
pub use types::*;

#[derive(Error, Debug)]
pub enum TeeError {
    #[error("Failed to initialize TEE: {0}")]
    InitializationError(String),
    #[error("Failed to execute in TEE: {0}")]
    ExecutionError(String),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Invalid attestation: {0}")]
    InvalidAttestation(String),
    #[error("Verification failed: {0}")]
    VerificationError(String),
}

/// Trait for TEE verification logic
#[cfg_attr(feature = "async", async_trait)]
pub trait TeeVerification: Send + Sync {
    /// Verify a TEE attestation
    #[cfg(feature = "async")]
    async fn verify_attestation(&self, attestation: &TeeAttestation) -> Result<bool, TeeError>;
    
    #[cfg(not(feature = "async"))]
    fn verify_attestation(&self, attestation: &TeeAttestation) -> Result<bool, TeeError>;

    /// Verify multiple TEE attestations
    #[cfg(feature = "async")]
    async fn verify_attestations(&self, attestations: &[TeeAttestation]) -> Result<bool, TeeError>;

    #[cfg(not(feature = "async"))]
    fn verify_attestations(&self, attestations: &[TeeAttestation]) -> Result<bool, TeeError>;
}

/// Main controller for TEE execution
#[cfg_attr(feature = "async", async_trait)]
pub trait TeeController: Send + Sync {
    /// Initialize the TEE controller
    #[cfg(feature = "async")]
    async fn init(&mut self) -> Result<(), TeeError>;
    
    #[cfg(not(feature = "async"))]
    fn init(&mut self) -> Result<(), TeeError>;

    /// Execute a payload in the TEE
    #[cfg(feature = "async")]
    async fn execute(&mut self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError>;
    
    #[cfg(not(feature = "async"))]
    fn execute(&mut self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError>;

    /// Get the current configuration
    #[cfg(feature = "async")]
    async fn get_config(&self) -> Result<TeeConfig, TeeError>;
    
    #[cfg(not(feature = "async"))]
    fn get_config(&self) -> Result<TeeConfig, TeeError>;

    /// Update the configuration
    #[cfg(feature = "async")]
    async fn update_config(&mut self, new_config: TeeConfig) -> Result<(), TeeError>;
    
    #[cfg(not(feature = "async"))]
    fn update_config(&mut self, new_config: TeeConfig) -> Result<(), TeeError>;

    /// Get attestations from the TEE
    #[cfg(feature = "async")]
    async fn get_attestations(&self) -> Result<Vec<TeeAttestation>, TeeError>;

    #[cfg(not(feature = "async"))]
    fn get_attestations(&self) -> Result<Vec<TeeAttestation>, TeeError>;
}

/// Prelude module containing commonly used types and traits
pub mod prelude {
    pub use super::{
        TeeController,
        TeeVerification,
        TeeError,
    };
    pub use crate::types::*;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_serialization() {
        let config = TeeConfig {
            min_attestations: 2,
            verify_measurements: true,
            max_input_size: 2048 * 1024,
            max_memory_size: 32 * 1024 * 1024,
            max_execution_time: 10000,
            max_gas: 2_000_000,
        };

        let encoded = borsh::to_vec(&config).unwrap();
        let decoded: TeeConfig = borsh::from_slice(&encoded).unwrap();

        assert_eq!(decoded.min_attestations, config.min_attestations);
        assert_eq!(decoded.verify_measurements, config.verify_measurements);
        assert_eq!(decoded.max_input_size, config.max_input_size);
        assert_eq!(decoded.max_memory_size, config.max_memory_size);
        assert_eq!(decoded.max_execution_time, config.max_execution_time);
        assert_eq!(decoded.max_gas, config.max_gas);
    }
}