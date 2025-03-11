use crate::mesh::{MeshCoordinator, PeerInfo};
use crate::discovery_service::{DiscoveryService, DiscoveryServiceConfig, BatchRegistrationResult};
use std::sync::Arc;
use log::info;
use tee_interface::types::TeeAttestation as AttestationReport;
use tee_interface::TeeError;

/// Enhanced discovery integration that provides better error handling and peer management
pub struct EnhancedDiscoveryIntegration {
    /// Internal discovery service
    discovery_service: Arc<DiscoveryService>,
    
    /// Core mesh coordinator for execution routing
    mesh: Arc<MeshCoordinator>,
    
    /// Service configuration
    config: EnhancedDiscoveryConfig,
}

/// Configuration for enhanced discovery integration
pub struct EnhancedDiscoveryConfig {
    /// Cache TTL in seconds
    pub cache_ttl: u64,
    
    /// Maximum batch size for operations
    pub max_batch_size: usize,
    
    /// Verification threshold for peer validation
    pub verification_threshold: u64,
    
    /// Refresh interval in seconds
    pub refresh_interval: u64,
}

impl Default for EnhancedDiscoveryConfig {
    fn default() -> Self {
        Self {
            cache_ttl: 300,
            max_batch_size: 50,
            verification_threshold: 10,
            refresh_interval: 60,
        }
    }
}

impl EnhancedDiscoveryIntegration {
    /// Create a new discovery integration with the given mesh coordinator
    pub async fn new(
        mesh: Arc<MeshCoordinator>,
    ) -> Result<Self, TeeError> {
        Self::new_with_config(mesh, EnhancedDiscoveryConfig::default()).await
    }
    
    /// Create a new discovery integration with custom configuration
    pub async fn new_with_config(
        mesh: Arc<MeshCoordinator>,
        config: EnhancedDiscoveryConfig,
    ) -> Result<Self, TeeError> {
        // Create discovery service config from the enhanced config
        let service_config = DiscoveryServiceConfig {
            max_batch_size: config.max_batch_size,
            verification_threshold: config.verification_threshold,
            cache_ttl: config.cache_ttl,
            max_regions: 100, // Default value
            refresh_interval: config.refresh_interval,
        };
        
        // Initialize the discovery service
        let discovery_service = DiscoveryService::new_with_params(
            mesh.clone(),
            service_config,
        ).await?;
        
        Ok(Self {
            discovery_service,
            mesh,
            config,
        })
    }
    
    /// Register a single peer with the discovery service
    pub async fn register_peer(
        &self,
        peer_info: &PeerInfo,
        attestation: AttestationReport,
    ) -> Result<String, TeeError> {
        self.discovery_service.register_peer(
            peer_info.tee_id.clone(),
            peer_info.region_id.clone(),
            attestation,
        ).await
    }
    
    /// Verify a peer's attestation
    pub async fn verify_peer(
        &self,
        peer_id: &str,
    ) -> Result<bool, TeeError> {
        let results = self.discovery_service.batch_verify_executors(vec![peer_id.to_string()]).await?;
        
        Ok(results.first().cloned().unwrap_or(false))
    }
    
    /// Batch verify multiple peers
    pub async fn batch_verify_peers(
        &self,
        peer_ids: Vec<String>,
    ) -> Result<Vec<bool>, TeeError> {
        self.discovery_service.batch_verify_executors(peer_ids).await
    }
    
    /// Batch register multiple peers
    pub async fn batch_register_peers(
        &self,
        peers: Vec<(PeerInfo, AttestationReport)>,
    ) -> Result<BatchRegistrationResult, TeeError> {
        if peers.is_empty() {
            return Ok(BatchRegistrationResult {
                success_count: 0,
                failed_count: 0,
                peers: Vec::new(),
            });
        }
        
        info!("Batch registering {} peers", peers.len());
        
        let mut transformed_peers = Vec::with_capacity(peers.len());
        for (peer_info, attestation) in peers {
            transformed_peers.push((
                peer_info.tee_id.clone(),
                peer_info.region_id.clone(),
                attestation,
            ));
        }
        
        self.discovery_service.batch_register_peers(transformed_peers).await
    }
    
    /// Refresh the discovery cache
    pub async fn refresh_cache(&self) -> Result<(), TeeError> {
        self.discovery_service.refresh_cache().await
    }
    
    /// Get all peers in a region
    pub async fn get_peers_by_region(&self, region_id: &str) -> Result<Vec<String>, TeeError> {
        self.discovery_service.get_peers_by_region(region_id).await
    }
    
    /// Get all known regions
    pub async fn get_all_regions(&self) -> Result<Vec<tee_interface::types::RegionInfo>, TeeError> {
        self.discovery_service.get_all_regions().await
    }
}
