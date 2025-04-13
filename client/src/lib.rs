/*!
 * Aristo Client Library
 * 
 * Type-safe client for Aristo's cross-regional TEE architecture, supporting:
 * - Cross-regional communication between TEE pairs
 * - WebAssembly contract parameter handling
 * - External API integration (including NASDAQ Capital Access Platform)
 * - High-performance ITCH market data integration 
 * - Attestation verification for secure communication
 */

use thiserror::Error;
use std::sync::Arc;

pub mod adapters;
pub mod protocol;
pub mod error;
pub mod nasdaq;

// Feature-gated modules
#[cfg(feature = "market-data")]
pub mod itch;

#[cfg(any(feature = "sgx", feature = "sev"))]
pub mod attestation;

pub mod client;
pub mod tee;

pub use client::AristoClient;
pub use protocol::ParameterFormat;

/// Re-export TEE-specific types as needed
#[cfg(feature = "sev")]
pub use sev_snp_types;

#[cfg(feature = "sgx")]
pub use sgx_types;

/// Configuration for the AristoClient
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Endpoints for regional TEE coordinators
    pub regions: std::collections::HashMap<String, String>,
    /// Attestation verification configuration
    #[cfg(any(feature = "sgx", feature = "sev"))]
    pub attestation: Option<attestation::AttestationConfig>,
    #[cfg(not(any(feature = "sgx", feature = "sev")))]
    pub attestation: Option<()>, // Placeholder when attestation is not enabled
    /// Optional external API configurations
    pub external_apis: std::collections::HashMap<String, adapters::ApiConfig>,
}

// AristoClient implementation is now in client.rs

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_client_creation() {
        let config = ClientConfig {
            regions: [
                ("us-west".to_string(), "https://tee-us-west.example.com".to_string()),
                ("eu-central".to_string(), "https://tee-eu-central.example.com".to_string()),
            ].into(),
            attestation: None,
            external_apis: Default::default(),
        };
        
        let client = AristoClient::new(config);
        assert_eq!(client.config.regions.len(), 2);
    }
}
