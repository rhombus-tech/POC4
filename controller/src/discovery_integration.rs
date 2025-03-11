use crate::discovery_service::{DiscoveryService, LocalityInfoDto, HierarchicalRegionInfo};
use std::sync::Arc;
use log::{info, error};
use serde::{Serialize, Deserialize};
use tee_interface::{TeeError, types::TeeAttestation};
use tee_interface::types::RegionInfo;

/// Configuration for the enhanced discovery integration
#[derive(Debug, Clone)]
pub struct EnhancedDiscoveryConfig {
    /// Maximum number of super peers per region
    pub max_super_peers: usize,
    
    /// Maximum latency considered for proximity calculations (ms)
    pub max_latency_ms: u32,
    
    /// Number of peers to sample for locality-based routing
    pub locality_sample_size: usize,
    
    /// Weight factor for connection health in routing decisions (0.0-1.0)
    pub connection_health_weight: f64,
    
    /// Weight factor for latency in routing decisions (0.0-1.0)
    pub latency_weight: f64,
    
    /// Refresh interval for discovery cache (seconds)
    pub cache_refresh_interval_sec: u64,
}

impl Default for EnhancedDiscoveryConfig {
    fn default() -> Self {
        EnhancedDiscoveryConfig {
            max_super_peers: 5,
            max_latency_ms: 500,
            locality_sample_size: 10,
            connection_health_weight: 0.7,
            latency_weight: 0.3,
            cache_refresh_interval_sec: 300,
        }
    }
}

/// Enhanced discovery integration with support for hierarchical regions and locality-aware routing
pub struct EnhancedDiscoveryIntegration {
    discovery_service: Arc<DiscoveryService>,
}

impl EnhancedDiscoveryIntegration {
    /// Create a new instance of the discovery integration
    pub fn new(discovery_service: Arc<DiscoveryService>) -> Self {
        EnhancedDiscoveryIntegration {
            discovery_service,
        }
    }
    
    /// Register a peer with a region
    pub async fn register_peer(
        &self,
        executor_id: String,
        region_id: String,
        attestation: TeeAttestation,
    ) -> Result<bool, TeeError> {
        // Convert the String return type to bool by checking if operation was successful
        match self.discovery_service.register_peer(executor_id, region_id, attestation).await {
            Ok(_) => Ok(true),
            Err(e) => Err(e),
        }
    }
    
    /// Register a peer with a region and locality information
    pub async fn register_peer_with_locality(
        &self,
        executor_id: String,
        region_id: String,
        attestation: TeeAttestation,
        locality_info: LocalityInfoDto,
    ) -> Result<bool, TeeError> {
        self.discovery_service.register_with_region_and_locality(
            executor_id, 
            region_id, 
            attestation, 
            Some(locality_info)
        ).await
    }
    
    /// Verify a peer
    pub async fn verify_peer(&self, executor_id: String) -> Result<bool, TeeError> {
        // Since we don't have direct methods to get attestation,
        // we'll check if the peer exists in any region
        let all_regions = self.discovery_service.get_all_regions().await?;
        
        for region_info in all_regions {
            let region_peers = self.discovery_service.get_region_peers(&region_info.region_id).await?;
            if region_peers.contains(&executor_id) {
                // If we found the executor in a region, we consider it verified
                // In a real implementation, we'd do proper attestation verification here
                return Ok(true);
            }
        }
        
        // Executor not found in any region
        info!("Peer not found in any region: {}", executor_id);
        Ok(false)
    }
    
    /// Get all peers in a region
    pub async fn get_region_peers(&self, region_id: &str) -> Result<Vec<String>, TeeError> {
        self.discovery_service.get_region_peers(region_id).await
    }
    
    /// Get all peers in a region and its child regions
    pub async fn get_region_peers_recursive(&self, region_id: &str) -> Result<Vec<String>, TeeError> {
        self.discovery_service.get_region_peers_recursive(region_id).await
    }
    
    /// Get all regions
    pub async fn get_all_regions(&self) -> Result<Vec<RegionInfo>, TeeError> {
        let discovery_regions = self.discovery_service.get_all_regions().await?;
        
        // Convert DiscoveryRegionInfo to RegionInfo
        let regions = discovery_regions.into_iter()
            .map(|region| RegionInfo {
                id: region.region_id,
                worker_ids: Vec::new(), // We don't have this information in DiscoveryRegionInfo
                max_tasks: 100, // Use a default value since DiscoveryRegionInfo doesn't have this
            })
            .collect();
        
        Ok(regions)
    }
    
    /// Get all regions with their hierarchical relationships
    pub async fn get_regions_with_hierarchy(&self) -> Result<Vec<HierarchicalRegionInfo>, TeeError> {
        self.discovery_service.get_regions_with_hierarchy().await
    }
    
    /// Set up a parent-child relationship between regions
    pub async fn set_region_hierarchy(
        &self,
        parent_region: String,
        child_region: String,
    ) -> Result<(), TeeError> {
        self.discovery_service.set_region_hierarchy(parent_region, child_region).await
    }
    
    /// Find the nearest executors to a given region
    pub async fn find_nearest_executors(
        &self,
        from_region: &str,
        count: u64,
    ) -> Result<Vec<(String, u32)>, TeeError> {
        self.discovery_service.find_nearest_executors(from_region, count).await
    }
    
    /// Find the optimal executor for a request based on locality
    pub async fn find_optimal_executor(
        &self,
        region_id: &str,
    ) -> Result<Option<String>, TeeError> {
        // First try to get an executor from the specified region
        let region_peers = self.discovery_service.get_region_peers(region_id).await?;
        
        if !region_peers.is_empty() {
            // In a more sophisticated implementation, we could select based on load, health, etc.
            // For now, just return the first available executor
            return Ok(Some(region_peers[0].clone()));
        }
        
        // If no executor found in the region, find the nearest one
        let nearest = self.discovery_service.find_nearest_executors(region_id, 1).await?;
        
        if !nearest.is_empty() {
            return Ok(Some(nearest[0].0.clone()));
        }
        
        // No suitable executor found
        Ok(None)
    }
    
    /// Refresh the discovery service cache
    pub async fn refresh_cache(&self) -> Result<(), TeeError> {
        self.discovery_service.refresh_cache().await
    }
}
