/*!
 * Aristo Client Library
 * 
 * Type-safe client for Aristo's cross-regional TEE architecture, supporting:
 * - Cross-regional communication between TEE pairs
 * - WebAssembly contract parameter handling
 * - External API integration (including NASDAQ Capital Access Platform)
 * - Attestation verification for secure communication
 */

pub mod error;
pub mod protocol;
pub mod adapters;
pub mod nasdaq;
pub mod attestation;

use std::sync::Arc;

/// Core client for interacting with the Aristo TEE mesh network
#[derive(Clone)]
pub struct AristoClient {
    config: Arc<ClientConfig>,
}

/// Configuration for the AristoClient
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Endpoints for regional TEE coordinators
    pub regions: std::collections::HashMap<String, String>,
    /// Attestation verification configuration
    pub attestation: Option<attestation::AttestationConfig>,
    /// Optional external API configurations
    pub external_apis: std::collections::HashMap<String, adapters::ApiConfig>,
}

impl AristoClient {
    /// Create a new client with the provided configuration
    pub fn new(config: ClientConfig) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
    
    /// Get a protocol client for cross-regional communication
    pub fn protocol(&self) -> protocol::ProtocolClient {
        protocol::ProtocolClient::new(self.config.clone())
    }
    
    /// Get an adapter for external API integration
    pub fn adapter(&self, name: &str) -> Option<adapters::ApiAdapter> {
        self.config.external_apis.get(name).map(|config| {
            adapters::ApiAdapter::new(name.to_string(), config.clone())
        })
    }
    
    /// Get the NASDAQ API client if configured
    pub fn nasdaq(&self) -> Option<nasdaq::NasdaqClient> {
        self.adapter("nasdaq").map(|adapter| {
            nasdaq::NasdaqClient::new(adapter)
        })
    }
}

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
