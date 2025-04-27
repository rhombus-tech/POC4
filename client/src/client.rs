/*!
 * AristoClient Implementation
 * 
 * Core client implementation for the Aristo TEE mesh.
 */

use std::sync::Arc;
use crate::adapters;
use crate::protocol;
#[cfg(feature = "market-data")]
use crate::itch;

/// Core client for interacting with the Aristo TEE mesh network
#[derive(Clone)]
pub struct AristoClient {
    pub config: Arc<crate::ClientConfig>,
}

impl AristoClient {
    /// Create a new client with the provided configuration
    pub fn new(config: crate::ClientConfig) -> Self {
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
    pub fn nasdaq(&self) -> Option<crate::nasdaq::NasdaqClient> {
        self.config.external_apis.get("nasdaq").map(|config| {
            let adapter = adapters::ApiAdapter::new("nasdaq".to_string(), config.clone());
            crate::nasdaq::NasdaqClient::new(adapter)
        })
    }
    
    /// Get the NASDAQ ITCH client for high-performance market data
    #[cfg(feature = "market-data")]
    pub fn itch(&self) -> itch::client::ITCHClient {
        // ITCH client doesn't require external API configuration
        // since it uses direct connection to ITCH feeds or files
        itch::client::ITCHClient::new()
    }
}
