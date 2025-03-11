use crate::accumulator::*;
use std::collections::HashMap;
use wasmlanche::{
    public, Address, Context, 
    state_schema,
    borsh::{BorshSerialize, BorshDeserialize}
};
use sha2::{Sha256, Digest};
use tee_interface::prelude::*;

// Additional state schema for discovery service optimization
state_schema! {
    /// Mapping of region IDs to their accumulator values
    RegionAccumulator(String) => [u8; 32],
    
    /// List of executors by region for quick lookup
    ExecutorsByRegion(String) => Vec<Address>,
    
    /// Mapping of executor to its region
    ExecutorRegion(Address) => String,
    
    /// Cache of recently verified executors to reduce verification overhead
    VerifiedExecutorCache(Address) => u64,  // Timestamp of last verification
    
    /// Additional metadata for discovery service
    DiscoveryServiceParams => DiscoveryParams,

    /// Hierarchical region structure - parent region mapping
    RegionParent(String) => String,
    
    /// Hierarchical region structure - child regions
    RegionChildren(String) => Vec<String>,
    
    /// Region metadata including locality information
    RegionMetadata(String) => RegionMetadataInfo,
    
    /// Executor locality information for optimized routing
    ExecutorLocality(Address) => LocalityInfo,
    
    /// Proximity map for locality-aware routing
    ProximityMap(String, String) => u32, // Region to region distance
    
    /// Super-peers for each region that handle inter-region communication
    RegionSuperPeers(String) => Vec<Address>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct DiscoveryParams {
    pub cache_ttl: u64,             // Time-to-live for verified executor cache (seconds)
    pub max_batch_size: u64,        // Maximum attestation batch size
    pub region_count_limit: u64,    // Maximum number of regions to track
    pub verification_threshold: u64, // Minimum number of attestations for automatic verification
    pub gossip_interval: u64,       // Interval for gossip protocol updates (seconds)
    pub max_super_peers: u32,       // Maximum number of super-peers per region
    pub proximity_update_interval: u64, // Interval for updating proximity map (seconds)
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct RegionInfo {
    pub region_id: String,
    pub executor_count: u64,
    pub accumulator_value: [u8; 32],
    pub last_update: u64,
    pub parent_region: Option<String>,   // Parent region ID if exists
    pub child_regions: Vec<String>,      // List of child region IDs
    pub super_peers: Vec<Address>,       // List of super-peers for this region
    pub locality_info: Option<LocalityInfo>, // Locality information for routing
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct LocalityInfo {
    pub geographic_region: String,   // Geographic region (e.g., "us-west", "asia-east")
    pub network_zone: String,        // Network zone identifier
    pub latency_profile: LatencyProfile, // Latency characteristics 
    pub coordinates: Option<NetworkCoordinates>, // Virtual network coordinates
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct LatencyProfile {
    pub avg_latency_ms: u32,        // Average latency in milliseconds
    pub min_latency_ms: u32,        // Minimum observed latency
    pub max_latency_ms: u32,        // Maximum observed latency
    pub jitter_ms: u32,             // Latency variation
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct NetworkCoordinates {
    pub coordinates: [f32; 3],      // 3D coordinates for network locality
    pub last_updated: u64,          // When coordinates were last updated
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct RegionMetadataInfo {
    pub created_at: u64,            // When this region was created
    pub description: String,        // Human-readable description
    pub locality: LocalityInfo,     // Locality information
    pub tier: u32,                  // Hierarchy tier (0 = root, higher = deeper nesting)
    pub region_weight: u32,         // Weight for load distribution
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct BatchAttestationResult {
    pub success_count: u64,
    pub failed_count: u64,
    pub results: Vec<(Address, bool)>,
}

/// Initialize the discovery service extension
#[public]
pub fn init_discovery(context: &mut Context, params: DiscoveryParams) -> Result<(), TeeError> {
    if context.get(DiscoveryServiceParams)?.is_some() {
        return Err(TeeError::InitializationError("Discovery service already initialized".into()));
    }
    
    // Default values if not specified
    let params = if params.cache_ttl == 0 {
        DiscoveryParams {
            cache_ttl: 3600, // Default 1 hour TTL
            gossip_interval: 30, // Default 30 seconds
            max_super_peers: 3, // Default 3 super-peers per region
            proximity_update_interval: 300, // Default 5 minutes
            ..params
        }
    } else {
        params
    };
    
    context.store((DiscoveryServiceParams, params))?;
    
    Ok(())
}

/// Register an executor with a specific region
#[public]
pub fn register_with_region(
    context: &mut Context,
    executor: Address,
    region_id: String,
    attestation: AttestationReport,
    locality_info: Option<LocalityInfo>,
) -> Result<bool, TeeError> {
    // First register the attestation using the core function
    // This ensures all the basic verification happens
    register_attestation(context, attestation)?;
    
    // Now add the region-specific data
    let mut executors = context.get(ExecutorsByRegion(region_id.clone()))?.unwrap_or_default();
    
    // Check if executor is already in this region
    if !executors.contains(&executor) {
        executors.push(executor);
        context.store((ExecutorsByRegion(region_id.clone()), executors))?;
    }
    
    // Store the region for this executor
    context.store((ExecutorRegion(executor), region_id.clone()))?;
    
    // Store locality information if provided
    if let Some(locality) = locality_info {
        context.store((ExecutorLocality(executor), locality))?;
    }
    
    // Check if this executor should be a super-peer
    maybe_designate_super_peer(context, executor, &region_id)?;
    
    // Update the region accumulator
    update_region_accumulator(context, executor)?;
    
    Ok(true)
}

/// Create a hierarchical relationship between regions
#[public]
pub fn set_region_hierarchy(
    context: &mut Context,
    parent_region: String,
    child_region: String,
) -> Result<bool, TeeError> {
    // Ensure both regions exist
    let parent_executors = context.get(ExecutorsByRegion(parent_region.clone()))?.unwrap_or_default();
    let child_executors = context.get(ExecutorsByRegion(child_region.clone()))?.unwrap_or_default();
    
    if parent_executors.is_empty() || child_executors.is_empty() {
        return Err(TeeError::ValidationError("Both parent and child regions must exist".into()));
    }
    
    // Set parent-child relationship
    context.store((RegionParent(child_region.clone()), parent_region.clone()))?;
    
    // Update children list for parent
    let mut children = context.get(RegionChildren(parent_region.clone()))?.unwrap_or_default();
    if !children.contains(&child_region) {
        children.push(child_region.clone());
        context.store((RegionChildren(parent_region), children))?;
    }
    
    // Ensure we update the proximity map
    update_region_proximity(context, &child_region)?;
    
    Ok(true)
}

/// Update region proximity information for optimized routing
#[public]
pub fn update_region_proximity(
    context: &mut Context,
    region_id: &str,
) -> Result<(), TeeError> {
    // Get all regions
    let all_regions = get_all_regions(context)?;
    
    // Get parent region if any
    let parent_opt = context.get(RegionParent(region_id.to_string()))?.clone();
    
    // Update proximity for this region to all other regions
    for other_region in &all_regions {
        if other_region.region_id == region_id {
            continue;
        }
        
        // Calculate proximity based on hierarchy and locality
        let proximity = calculate_region_proximity(context, region_id, &other_region.region_id, parent_opt.as_ref())?;
        
        // Store in proximity map (both directions)
        context.store((ProximityMap(region_id.to_string(), other_region.region_id.clone()), proximity))?;
        context.store((ProximityMap(other_region.region_id.clone(), region_id.to_string()), proximity))?;
    }
    
    Ok(())
}

/// Process a batch of attestations at once for improved throughput
#[public]
pub fn batch_register_attestations(
    context: &mut Context,
    attestations: Vec<(Address, AttestationReport, Option<String>)>,
) -> Result<BatchAttestationResult, TeeError> {
    let params = context.get(DiscoveryServiceParams)?.ok_or(
        TeeError::InitializationError("Discovery service not initialized".into())
    )?;
    
    if attestations.len() as u64 > params.max_batch_size {
        return Err(TeeError::ValidationError("Batch size exceeds maximum allowed".into()));
    }
    
    let mut results = Vec::new();
    let mut success_count = 0;
    let mut failed_count = 0;
    
    // Process each attestation
    for (executor, attestation, region_opt) in attestations {
        let result = match register_attestation(context, attestation.clone()) {
            Ok(_) => {
                // If region specified, register with that region
                if let Some(region) = region_opt {
                    match register_with_region(context, executor, region, attestation, None) {
                        Ok(_) => {
                            success_count += 1;
                            true
                        },
                        Err(_) => {
                            failed_count += 1;
                            false
                        }
                    }
                } else {
                    success_count += 1;
                    true
                }
            },
            Err(_) => {
                failed_count += 1;
                false
            }
        };
        
        results.push((executor, result));
    }
    
    Ok(BatchAttestationResult {
        success_count,
        failed_count,
        results,
    })
}

/// Get all executors in a specific region
#[public]
pub fn get_executors_by_region(
    context: &mut Context,
    region_id: String,
) -> Result<Vec<Address>, TeeError> {
    let executors = context.get(ExecutorsByRegion(region_id))?.unwrap_or_default();
    Ok(executors)
}

/// Get all executors in a specific region and all its child regions recursively
#[public]
pub fn get_executors_by_region_recursive(
    context: &mut Context,
    region_id: String,
) -> Result<Vec<Address>, TeeError> {
    let mut all_executors = context.get(ExecutorsByRegion(region_id.clone()))?.unwrap_or_default();
    
    // Get child regions
    let children = context.get(RegionChildren(region_id))?.unwrap_or_default();
    
    // Recursively get executors from child regions
    for child in children {
        let child_executors = get_executors_by_region_recursive(context, child)?;
        all_executors.extend(child_executors);
    }
    
    Ok(all_executors)
}

/// Get all known regions and their executor counts
#[public]
pub fn get_all_regions(context: &mut Context) -> Result<Vec<RegionInfo>, TeeError> {
    let regions = context.scan_keys(RegionAccumulator("".to_string()))?.collect::<Vec<_>>();
    
    let mut region_infos = Vec::new();
    for region_key in regions {
        if let Some(region_id) = extract_region_id(&region_key) {
            let executors = context.get(ExecutorsByRegion(region_id.clone()))?.unwrap_or_default();
            let acc_value = context.get(RegionAccumulator(region_id.clone()))?.unwrap_or([0; 32]);
            
            // Get hierarchical information
            let parent_region = context.get(RegionParent(region_id.clone()))?.clone();
            let child_regions = context.get(RegionChildren(region_id.clone()))?.unwrap_or_default();
            let super_peers = context.get(RegionSuperPeers(region_id.clone()))?.unwrap_or_default();
            
            // Get locality information from region metadata
            let region_metadata = context.get(RegionMetadata(region_id.clone()))?;
            let locality_info = region_metadata.map(|meta| meta.locality.clone());
            
            region_infos.push(RegionInfo {
                region_id,
                executor_count: executors.len() as u64,
                accumulator_value: acc_value,
                last_update: context.timestamp(),
                parent_region,
                child_regions,
                super_peers,
                locality_info,
            });
        }
    }
    
    Ok(region_infos)
}

/// Set region metadata including locality information
#[public]
pub fn set_region_metadata(
    context: &mut Context,
    region_id: String,
    metadata: RegionMetadataInfo,
) -> Result<bool, TeeError> {
    // Make sure region exists
    let executors = context.get(ExecutorsByRegion(region_id.clone()))?.unwrap_or_default();
    if executors.is_empty() {
        return Err(TeeError::ValidationError("Region does not exist".into()));
    }
    
    // Store metadata
    context.store((RegionMetadata(region_id), metadata))?;
    
    Ok(true)
}

/// Find nearest executors based on locality information
#[public]
pub fn find_nearest_executors(
    context: &mut Context,
    from_region: String,
    count: u64,
) -> Result<Vec<(Address, u32)>, TeeError> {
    // Get all regions sorted by proximity to this region
    let all_regions = get_all_regions(context)?;
    let mut region_distances = Vec::new();
    
    for region in all_regions {
        if region.region_id == from_region {
            continue;
        }
        
        let distance = context.get(ProximityMap(from_region.clone(), region.region_id.clone()))?.unwrap_or(u32::MAX);
        region_distances.push((region.region_id, distance));
    }
    
    // Sort by distance (ascending)
    region_distances.sort_by_key(|(_, distance)| *distance);
    
    // Collect executors from nearest regions until we have enough
    let mut nearest_executors = Vec::new();
    for (region_id, distance) in region_distances {
        let executors = context.get(ExecutorsByRegion(region_id))?.unwrap_or_default();
        
        for executor in executors {
            nearest_executors.push((executor, distance));
            
            if nearest_executors.len() as u64 >= count {
                break;
            }
        }
        
        if nearest_executors.len() as u64 >= count {
            break;
        }
    }
    
    Ok(nearest_executors)
}

/// Verify a batch of executors at once
#[public]
pub fn batch_verify_executors(
    context: &mut Context,
    executors: Vec<Address>,
) -> Result<Vec<bool>, TeeError> {
    let params = context.get(DiscoveryServiceParams)?.ok_or(
        TeeError::InitializationError("Discovery service not initialized".into())
    )?;
    
    let mut results = Vec::new();
    let current_time = context.timestamp();
    
    for executor in executors {
        // First check the cache
        let cached_time = context.get(VerifiedExecutorCache(executor))?.unwrap_or(0);
        
        if current_time - cached_time <= params.cache_ttl {
            // Cache hit - executor was verified recently
            results.push(true);
            continue;
        }
        
        // Cache miss - need to do a real verification
        let record = match context.get(AttestationRecord(executor))? {
            Some(record) => record,
            None => {
                results.push(false);
                continue;
            }
        };
        
        // Simple verification - check attestation count exceeds threshold
        if record.attestation_count >= params.verification_threshold {
            // Update cache
            context.store((VerifiedExecutorCache(executor), current_time))?;
            results.push(true);
        } else {
            results.push(false);
        }
    }
    
    Ok(results)
}

// Helper function to update the region accumulator
fn update_region_accumulator(
    context: &mut Context,
    executor: Address,
) -> Result<(), TeeError> {
    let region_id = match context.get(ExecutorRegion(executor))? {
        Some(region) => region,
        None => return Ok(()), // No region, nothing to update
    };
    
    let witness = match context.get(Witness(executor))? {
        Some(w) => w,
        None => return Ok(()), // No witness, can't update accumulator
    };
    
    // Get or initialize the region accumulator
    let mut region_acc = context.get(RegionAccumulator(region_id.clone()))?.unwrap_or([0; 32]);
    
    // Update with this executor's information
    let mut hasher = Sha256::new();
    hasher.update(&region_acc);
    hasher.update(&witness.value);
    region_acc = hasher.finalize().into();
    
    // Store updated accumulator
    context.store((RegionAccumulator(region_id), region_acc))?;
    
    Ok(())
}

// Helper to extract region ID from a key
fn extract_region_id(key: &str) -> Option<String> {
    // Expected format: "RegionAccumulator(region_id)"
    let start = key.find('(')?;
    let end = key.find(')')?;
    if start < end {
        Some(key[start+1..end].to_string())
    } else {
        None
    }
}

// Helper to designate super-peers for a region based on certain criteria
fn maybe_designate_super_peer(
    context: &mut Context,
    executor: Address,
    region_id: &str,
) -> Result<bool, TeeError> {
    let params = context.get(DiscoveryServiceParams)?.ok_or(
        TeeError::InitializationError("Discovery service not initialized".into())
    )?;
    
    let mut super_peers = context.get(RegionSuperPeers(region_id.to_string()))?.unwrap_or_default();
    
    // If we have fewer super-peers than the maximum, consider adding this one
    if super_peers.len() < params.max_super_peers as usize {
        // Check if this executor is eligible to be a super-peer
        // We could implement more sophisticated criteria here
        
        // For now, just add it if not already a super-peer
        if !super_peers.contains(&executor) {
            super_peers.push(executor);
            context.store((RegionSuperPeers(region_id.to_string()), super_peers))?;
            return Ok(true);
        }
    }
    
    Ok(false)
}

// Helper to calculate proximity between regions
fn calculate_region_proximity(
    context: &mut Context,
    region1: &str,
    region2: &str,
    parent_of_region1: Option<&String>,
) -> Result<u32, TeeError> {
    // Start with a high base distance
    let mut proximity = 1000u32;
    
    // Check if they share a parent-child relationship
    if let Some(parent) = parent_of_region1 {
        if parent == region2 {
            // Direct parent-child relationship
            proximity = 10;
        } else {
            // Check if they share a common ancestor
            let parent_of_region2 = context.get(RegionParent(region2.to_string()))?;
            if let Some(parent2) = parent_of_region2 {
                if parent2 == *parent {
                    // Sibling regions
                    proximity = 20;
                }
            }
        }
    }
    
    // Further refine based on locality information if available
    let metadata1 = context.get(RegionMetadata(region1.to_string()))?;
    let metadata2 = context.get(RegionMetadata(region2.to_string()))?;
    
    if let (Some(meta1), Some(meta2)) = (metadata1, metadata2) {
        // If in same geographic region, reduce distance
        if meta1.locality.geographic_region == meta2.locality.geographic_region {
            proximity = proximity.saturating_sub(5);
        }
        
        // If in same network zone, reduce distance further
        if meta1.locality.network_zone == meta2.locality.network_zone {
            proximity = proximity.saturating_sub(5);
        }
        
        // Use network coordinates if available for more precise distance
        if let (Some(coords1), Some(coords2)) = (
            &meta1.locality.coordinates, 
            &meta2.locality.coordinates
        ) {
            // Calculate Euclidean distance between coordinates
            let mut squared_dist = 0.0;
            for i in 0..3 {
                let diff = coords1.coordinates[i] - coords2.coordinates[i];
                squared_dist += diff * diff;
            }
            let euclidean_dist = (squared_dist.sqrt() * 100.0) as u32;
            
            // Use the smaller of the hierarchical or coordinate-based distance
            proximity = proximity.min(euclidean_dist);
        }
    }
    
    Ok(proximity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmlanche::simulator::{Simulator, SimpleState};
{{ ... }}
