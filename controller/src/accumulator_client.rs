use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tee_interface::{TeeError};
use serde::{Serialize, Deserialize};
use log::{info, warn, error, debug};
use reqwest::Client as HttpClient;
use async_trait::async_trait;
use sha2::Digest;

// Define these types locally since they can't be imported from tee_interface
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationReport {
    pub data: Vec<u8>,
    pub signature: Vec<u8>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum EnclaveType {
    IntelSGX,
    AMDSEV,
}

// Define LocalityInfoDto here
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalityInfoDto {
    pub region_id: String,
    pub zone: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub tier: Option<String>,
}

use crate::discovery_service::DiscoveryRegionInfo;

// Address type for the accumulator contract
type Address = [u8; 32];

/// AccumulatorClient interface trait - allows for real and mock implementations
#[async_trait]
pub trait AccumulatorClientTrait: Send + Sync {
    async fn register_peer(&self, peer_id: &str, region_id: &str) -> Result<(), String>;
    async fn get_peers_in_region(&self, region_id: &str) -> Result<Vec<String>, String>;
    async fn get_all_regions(&self) -> Result<Vec<DiscoveryRegionInfo>, String>;
    async fn verify_peer(&self, peer_id: &str) -> Result<bool, String>;
    async fn set_peer_locality(&self, peer_id: &str, locality: &LocalityInfoDto) -> Result<(), String>;
    async fn batch_verify_peers(&self, peers: &[String]) -> Result<Vec<bool>, String>;
    async fn set_region_hierarchy(&self, parent: &str, child: &str) -> Result<(), String>;
    async fn get_peers_by_proximity(&self, region_id: &str) -> Result<Vec<String>, String>;
    async fn get_super_peers(&self, region_id: &str) -> Result<Vec<String>, String>;
}

// Mock client for testing - replicates the existing mock implementation
#[derive(Debug, Clone)]
pub struct MockAccumulatorClient {
    pub endpoint: String,
}

#[async_trait]
impl AccumulatorClientTrait for MockAccumulatorClient {
    async fn register_peer(&self, _peer_id: &str, _region_id: &str) -> Result<(), String> {
        Ok(())
    }
    
    async fn get_peers_in_region(&self, _region_id: &str) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
    
    async fn get_all_regions(&self) -> Result<Vec<DiscoveryRegionInfo>, String> {
        let regions = vec![
            DiscoveryRegionInfo {
                region_id: "us-west".to_string(),
                executor_count: 5,
                is_leaf: true,
                parent_region: Some("us".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "us-east".to_string(),
                executor_count: 7,
                is_leaf: true,
                parent_region: Some("us".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "us".to_string(),
                executor_count: 12,
                is_leaf: false,
                parent_region: Some("global".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "eu".to_string(),
                executor_count: 8,
                is_leaf: false,
                parent_region: Some("global".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "global".to_string(),
                executor_count: 20,
                is_leaf: false,
                parent_region: None,
            },
        ];
        
        Ok(regions)
    }
    
    async fn verify_peer(&self, _peer_id: &str) -> Result<bool, String> {
        Ok(true)
    }
    
    async fn set_peer_locality(&self, _peer_id: &str, _locality: &LocalityInfoDto) -> Result<(), String> {
        Ok(())
    }
    
    async fn batch_verify_peers(&self, peers: &[String]) -> Result<Vec<bool>, String> {
        let mut results = Vec::with_capacity(peers.len());
        for _ in peers {
            results.push(true);
        }
        Ok(results)
    }
    
    async fn set_region_hierarchy(&self, _parent: &str, _child: &str) -> Result<(), String> {
        Ok(())
    }
    
    async fn get_peers_by_proximity(&self, _region_id: &str) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
    
    async fn get_super_peers(&self, _region_id: &str) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }
}

impl MockAccumulatorClient {
    pub fn new() -> Self {
        Self {
            endpoint: "http://localhost:8080".to_string(),
        }
    }
}

// Real implementation of AccumulatorClient that connects to the accumulator contract
#[derive(Debug, Clone)]
pub struct RealAccumulatorClient {
    endpoint: String,
    http_client: HttpClient,
    timeout: Duration,
    // Cache of attestation records for better performance
    attestation_cache: Arc<RwLock<std::collections::HashMap<String, CachedAttestationRecord>>>,
    // Local identity for this client
    local_identity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedAttestationRecord {
    executor_id: String,
    last_verified: std::time::SystemTime,
    is_valid: bool,
    sgx_measurement: Option<[u8; 32]>,
    sev_measurement: Option<[u8; 32]>,
    accumulator_witness: Option<AccumulatorWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccumulatorElement {
    pub executor: Address,
    pub measurement: [u8; 32],
    pub enclave_type: EnclaveTypeDto,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AccumulatorWitness {
    pub value: [u8; 32],
    pub last_accumulator: [u8; 32],
    pub element: AccumulatorElement,
    pub last_update: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
enum EnclaveTypeDto {
    IntelSGX,
    AMDSEV,
}

impl From<EnclaveType> for EnclaveTypeDto {
    fn from(e: EnclaveType) -> Self {
        match e {
            EnclaveType::IntelSGX => EnclaveTypeDto::IntelSGX,
            EnclaveType::AMDSEV => EnclaveTypeDto::AMDSEV,
        }
    }
}

// DTO for request/response with the contract API
#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerifyAttestationRequest {
    executor_id: String,
    sgx_attestation: Option<AttestationReport>,
    sev_attestation: Option<AttestationReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VerifyAttestationResponse {
    is_valid: bool,
    accumulator_value: [u8; 32],
    witness: Option<AccumulatorWitness>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegisterAttestationRequest {
    executor_id: String,
    attestation: AttestationReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegisterAttestationResponse {
    success: bool,
    accumulator_value: [u8; 32],
    witness: AccumulatorWitness,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetRegionsResponse {
    regions: Vec<RegionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegionDto {
    id: String,
    executor_count: usize,
    parent_id: Option<String>,
    is_leaf: bool,
}

impl RealAccumulatorClient {
    pub fn new(endpoint: &str, local_identity: &str) -> Self {
        let http_client = HttpClient::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| HttpClient::new());
            
        Self {
            endpoint: endpoint.to_string(),
            http_client,
            timeout: Duration::from_secs(10),
            attestation_cache: Arc::new(RwLock::new(std::collections::HashMap::new())),
            local_identity: local_identity.to_string(),
        }
    }
    
    // Helper to convert string ID to 32-byte address
    fn string_to_address(&self, id: &str) -> Address {
        let mut hasher = sha2::Sha256::new();
        hasher.update(id.as_bytes());
        hasher.finalize().into()
    }
    
    // Cache management helpers
    async fn update_cache(&self, executor_id: &str, record: CachedAttestationRecord) {
        let mut cache = self.attestation_cache.write().await;
        cache.insert(executor_id.to_string(), record);
    }
    
    async fn get_from_cache(&self, executor_id: &str) -> Option<CachedAttestationRecord> {
        let cache = self.attestation_cache.read().await;
        cache.get(executor_id).cloned()
    }
    
    // Clear expired cache entries (older than 1 hour)
    async fn clean_cache(&self) {
        let mut cache = self.attestation_cache.write().await;
        let now = std::time::SystemTime::now();
        cache.retain(|_, record| {
            match now.duration_since(record.last_verified) {
                Ok(duration) => duration < Duration::from_secs(3600),
                Err(_) => true, // If system time went backwards, keep the record
            }
        });
    }
}

#[async_trait]
impl AccumulatorClientTrait for RealAccumulatorClient {
    async fn register_peer(&self, peer_id: &str, region_id: &str) -> Result<(), String> {
        // For now, this is a placeholder that would interact with the real accumulator contract
        // We'll implement this as part of the integration
        debug!("Registering peer {} in region {}", peer_id, region_id);
        
        // In a real implementation, we would:
        // 1. Generate or retrieve attestation report(s)
        // 2. Call the accumulator contract to register the attestation
        // 3. Store the witness for future verification
        
        Ok(())
    }
    
    async fn get_peers_in_region(&self, region_id: &str) -> Result<Vec<String>, String> {
        // For now, this is a placeholder that would interact with the real accumulator contract
        debug!("Getting peers in region {}", region_id);
        
        // In a real implementation, we would query the contract for peers in the region
        
        Ok(Vec::new())
    }
    
    async fn get_all_regions(&self) -> Result<Vec<DiscoveryRegionInfo>, String> {
        // Mock implementation for now, to be replaced with actual contract calls
        debug!("Getting all regions");
        
        // For now, return the same mock data as the mock client
        let regions = vec![
            DiscoveryRegionInfo {
                region_id: "us-west".to_string(),
                executor_count: 5,
                is_leaf: true,
                parent_region: Some("us".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "us-east".to_string(),
                executor_count: 7,
                is_leaf: true,
                parent_region: Some("us".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "us".to_string(),
                executor_count: 12,
                is_leaf: false,
                parent_region: Some("global".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "eu".to_string(),
                executor_count: 8,
                is_leaf: false,
                parent_region: Some("global".to_string()),
            },
            DiscoveryRegionInfo {
                region_id: "global".to_string(),
                executor_count: 20,
                is_leaf: false,
                parent_region: None,
            },
        ];
        
        Ok(regions)
    }
    
    async fn verify_peer(&self, peer_id: &str) -> Result<bool, String> {
        debug!("Verifying peer {}", peer_id);
        
        // Check if we have a cached result
        if let Some(cached) = self.get_from_cache(peer_id).await {
            // Use cache if it's recent (less than 5 minutes old)
            let now = std::time::SystemTime::now();
            if let Ok(duration) = now.duration_since(cached.last_verified) {
                if duration < Duration::from_secs(300) {
                    return Ok(cached.is_valid);
                }
            }
        }
        
        // In a real implementation, we would:
        // 1. Retrieve the peer's attestation and witness
        // 2. Verify against our local accumulator value
        // 3. Cache the result
        
        // For now, return true (valid)
        Ok(true)
    }
    
    async fn set_peer_locality(&self, peer_id: &str, locality: &LocalityInfoDto) -> Result<(), String> {
        debug!("Setting locality for peer {} to region {}", peer_id, locality.region_id);
        
        // In a real implementation, we would update the contract with the peer's locality
        
        Ok(())
    }
    
    async fn batch_verify_peers(&self, peers: &[String]) -> Result<Vec<bool>, String> {
        debug!("Batch verifying {} peers", peers.len());
        
        // For performance, first check cache
        let mut results = Vec::with_capacity(peers.len());
        let mut uncached_peers = Vec::new();
        
        // Check cache first
        for peer_id in peers {
            if let Some(cached) = self.get_from_cache(peer_id).await {
                // Use cache if it's recent (less than 5 minutes old)
                let now = std::time::SystemTime::now();
                if let Ok(duration) = now.duration_since(cached.last_verified) {
                    if duration < Duration::from_secs(300) {
                        results.push(cached.is_valid);
                        continue;
                    }
                }
            }
            
            // Not in cache or cache expired
            uncached_peers.push(peer_id.clone());
            results.push(true); // Default to true for now
        }
        
        if !uncached_peers.is_empty() {
            // In a real implementation, we would batch verify the uncached peers
            // For now, just set them all to valid
            // This would be replaced with actual verification logic
        }
        
        Ok(results)
    }
    
    async fn set_region_hierarchy(&self, parent: &str, child: &str) -> Result<(), String> {
        debug!("Setting region hierarchy: {} is parent of {}", parent, child);
        
        // In a real implementation, we would update the contract with the region hierarchy
        
        Ok(())
    }
    
    async fn get_peers_by_proximity(&self, region_id: &str) -> Result<Vec<String>, String> {
        debug!("Getting peers by proximity to region {}", region_id);
        
        // In a real implementation, we would query the contract for peers by proximity
        
        Ok(Vec::new())
    }
    
    async fn get_super_peers(&self, region_id: &str) -> Result<Vec<String>, String> {
        debug!("Getting super peers for region {}", region_id);
        
        // In a real implementation, we would query the contract for super peers
        
        Ok(Vec::new())
    }
}

// Factory function to create the appropriate AccumulatorClient implementation
pub fn create_accumulator_client(
    enhanced_discovery: bool,
    endpoint: Option<&str>,
    local_identity: Option<&str>
) -> Arc<dyn AccumulatorClientTrait> {
    if enhanced_discovery && endpoint.is_some() && local_identity.is_some() {
        // Create a real client with the provided endpoint and identity
        Arc::new(RealAccumulatorClient::new(
            endpoint.unwrap(),
            local_identity.unwrap()
        ))
    } else {
        // Fall back to mock client for backward compatibility
        Arc::new(MockAccumulatorClient::new())
    }
}
