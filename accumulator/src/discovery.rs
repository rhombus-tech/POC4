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
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct DiscoveryParams {
    pub cache_ttl: u64,             // Time-to-live for verified executor cache (seconds)
    pub max_batch_size: u64,        // Maximum attestation batch size
    pub region_count_limit: u64,    // Maximum number of regions to track
    pub verification_threshold: u64, // Minimum number of attestations for automatic verification
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct RegionInfo {
    pub region_id: String,
    pub executor_count: u64,
    pub accumulator_value: [u8; 32],
    pub last_update: u64,
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
    
    // Default TTL if not specified
    let params = if params.cache_ttl == 0 {
        DiscoveryParams {
            cache_ttl: 3600, // Default 1 hour TTL
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
    context.store((ExecutorRegion(executor), region_id))?;
    
    // Update the region accumulator
    update_region_accumulator(context, executor)?;
    
    Ok(true)
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
                    match register_with_region(context, executor, region, attestation) {
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

/// Get all known regions and their executor counts
#[public]
pub fn get_all_regions(context: &mut Context) -> Result<Vec<RegionInfo>, TeeError> {
    let regions = context.scan_keys(RegionAccumulator("".to_string()))?.collect::<Vec<_>>();
    
    let mut region_infos = Vec::new();
    for region_key in regions {
        if let Some(region_id) = extract_region_id(&region_key) {
            let executors = context.get(ExecutorsByRegion(region_id.clone()))?.unwrap_or_default();
            let acc_value = context.get(RegionAccumulator(region_id.clone()))?.unwrap_or([0; 32]);
            
            region_infos.push(RegionInfo {
                region_id,
                executor_count: executors.len() as u64,
                accumulator_value: acc_value,
                last_update: context.timestamp(),
            });
        }
    }
    
    Ok(region_infos)
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

#[cfg(test)]
mod tests {
    use super::*;
    use wasmlanche::simulator::{Simulator, SimpleState};

    fn setup_discovery() -> (Simulator, Address) {
        let mut state = SimpleState::new();
        let mut sim = Simulator::new(&mut state);
        
        // Initialize core accumulator
        let params = AccumulatorParams {
            max_size: 1000,
            max_witness_age: 7 * 24 * 60 * 60, // 1 week
            min_attestations: 2,
        };
        let ctx = &mut sim;
        init(ctx, params).unwrap();
        
        // Initialize discovery extension
        let discovery_params = DiscoveryParams {
            cache_ttl: 3600,
            max_batch_size: 100,
            region_count_limit: 50,
            verification_threshold: 1,
        };
        init_discovery(ctx, discovery_params).unwrap();
        
        let executor = Address::new([1; 33]);
        sim.set_actor(executor);
        
        (sim, executor)
    }

    #[test]
    fn test_region_registration() {
        let (mut sim, executor) = setup_discovery();
        let ctx = &mut sim;
        
        let attestation = AttestationReport {
            enclave_type: EnclaveType::IntelSGX,
            measurement: [1; 32],
            timestamp: ctx.timestamp(),
            platform_data: vec![1],
        };
        
        // Register with a region
        let result = register_with_region(ctx, executor, "us-west".to_string(), attestation);
        assert!(result.is_ok());
        
        // Verify the executor is in the region
        let executors = get_executors_by_region(ctx, "us-west".to_string()).unwrap();
        assert!(executors.contains(&executor));
        
        // Get all regions
        let regions = get_all_regions(ctx).unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].region_id, "us-west");
        assert_eq!(regions[0].executor_count, 1);
    }

    #[test]
    fn test_batch_attestation() {
        let (mut sim, executor) = setup_discovery();
        let ctx = &mut sim;
        
        // Create multiple executors and attestations
        let mut attestations = Vec::new();
        
        for i in 0..5 {
            let exec = Address::new([i as u8 + 1; 33]);
            
            let attestation = AttestationReport {
                enclave_type: EnclaveType::IntelSGX,
                measurement: [i as u8 + 1; 32],
                timestamp: ctx.timestamp(),
                platform_data: vec![i as u8 + 1],
            };
            
            attestations.push((exec, attestation, Some(format!("region-{}", i % 2))));
        }
        
        // Process batch
        let result = batch_register_attestations(ctx, attestations).unwrap();
        
        // Verify results
        assert_eq!(result.success_count, 5);
        assert_eq!(result.failed_count, 0);
        
        // Check regions
        let regions = get_all_regions(ctx).unwrap();
        assert_eq!(regions.len(), 2); // region-0 and region-1
        
        // Each region should have multiple executors
        for region in regions {
            let executors = get_executors_by_region(ctx, region.region_id).unwrap();
            assert!(executors.len() > 1);
        }
    }

    #[test]
    fn test_batch_verification() {
        let (mut sim, executor) = setup_discovery();
        let ctx = &mut sim;
        
        // Register an executor
        let attestation = AttestationReport {
            enclave_type: EnclaveType::IntelSGX,
            measurement: [1; 32],
            timestamp: ctx.timestamp(),
            platform_data: vec![1],
        };
        
        register_with_region(ctx, executor, "test-region".to_string(), attestation).unwrap();
        
        // Create a non-registered executor
        let unknown_executor = Address::new([2; 33]);
        
        // Batch verify
        let results = batch_verify_executors(ctx, vec![executor, unknown_executor]).unwrap();
        
        // First executor should be verified, second should fail
        assert_eq!(results.len(), 2);
        assert!(results[0]); // First executor verified
        assert!(!results[1]); // Second executor not verified
    }
}
