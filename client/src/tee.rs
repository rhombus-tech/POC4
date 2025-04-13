/*!
 * TEE Integration Module
 * 
 * This module provides common functionality for TEE integration.
 */
 
#[cfg(feature = "sgx")]
pub mod sgx;

#[cfg(feature = "sev")]
pub mod sev;

/// TEE Attestation Type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttestationType {
    /// Intel SGX Attestation
    #[cfg(feature = "sgx")]
    SGX,
    
    /// AMD SEV Attestation 
    #[cfg(feature = "sev")]
    SEV,
    
    /// Simulation mode (no actual attestation)
    Simulation,
}

/// TEE Configuration
#[derive(Debug, Clone)]
pub struct TeeConfig {
    /// Type of attestation to use
    pub attestation_type: AttestationType,
    
    /// Remote attestation URL (if applicable)
    pub attestation_url: Option<String>,
}

impl Default for TeeConfig {
    fn default() -> Self {
        Self {
            attestation_type: AttestationType::Simulation,
            attestation_url: None,
        }
    }
}
