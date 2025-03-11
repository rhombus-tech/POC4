use crate::discovery::{
    DiscoveryParams, RegionInfo, BatchAttestationResult,
    register_with_region, batch_register_attestations, batch_verify_executors,
    get_executors_by_region, get_all_regions, init_discovery
};
use crate::accumulator::{init, register_attestation, verify_attestation};
use tee_interface::prelude::*;
use wasmlanche::{Context, Address};
use std::time::{Duration, Instant};
use std::collections::HashSet;

// Helper function to create a test context
fn setup_discovery_test() -> (Context, Vec<AttestationReport>, Vec<Address>) {
    let mut context = Context::new();
    
    // Initialize the accumulator
    init(&mut context, Default::default()).unwrap();
    
    // Initialize the discovery extension
    let discovery_params = DiscoveryParams {
        cache_ttl: 3600,
        max_batch_size: 50,
        region_count_limit: 10,
        verification_threshold: 2,
    };
    init_discovery(&mut context, discovery_params).unwrap();
    
    // Create test attestations and addresses
    let mut attestations = Vec::new();
    let mut addresses = Vec::new();
    
    for i in 0..100 {
        let address = Address::new([i as u8; 33]);
        addresses.push(address);
        
        let attestation = AttestationReport {
            attestation_time: 1000 + i, // use incrementing timestamps
            mrenclave: [i as u8; 32],   // unique mrenclave for each attestation
            mrsigner: [0; 32],          // same signer for all
            report_data: vec![i as u8; 64],
            platform_id: "AMD-SEV-SNP".to_string(),
            platform_data: vec![0, 1, 2, 3],
        };
        attestations.push(attestation);
    }
    
    (context, attestations, addresses)
}

#[test]
fn test_region_registration() {
    let (mut context, attestations, addresses) = setup_discovery_test();
    
    // Register executors with different regions
    let regions = ["us-east", "us-west", "eu-central", "ap-southeast"];
    
    // Register each executor with a region
    for i in 0..40 {
        let region_idx = i % regions.len();
        let region = regions[region_idx].to_string();
        
        register_with_region(
            &mut context,
            addresses[i],
            attestations[i].clone(),
            Some(region),
        ).unwrap();
    }
    
    // Verify executors were registered with their regions
    for (i, region) in regions.iter().enumerate() {
        let executors = get_executors_by_region(&mut context, region).unwrap();
        
        // Each region should have 10 executors (40 total / 4 regions)
        assert_eq!(executors.len(), 10);
        
        // Verify the correct executors are in each region
        for j in 0..10 {
            let expected_idx = i + (j * regions.len());
            assert!(executors.contains(&addresses[expected_idx]));
        }
    }
    
    // Get all regions
    let all_regions = get_all_regions(&mut context).unwrap();
    assert_eq!(all_regions.len(), regions.len());
    
    for region in regions.iter() {
        let found = all_regions.iter().any(|r| &r.region_id == region);
        assert!(found, "Region {} should be in the list", region);
    }
}

#[test]
fn test_batch_registration() {
    let (mut context, attestations, addresses) = setup_discovery_test();
    
    // Prepare batch registration data
    let regions = ["us-east", "us-west", "eu-central", "ap-southeast"];
    let mut batch_data = Vec::new();
    
    for i in 0..40 {
        let region_idx = i % regions.len();
        let region = regions[region_idx].to_string();
        
        batch_data.push((
            addresses[i],
            attestations[i].clone(),
            Some(region),
        ));
    }
    
    // Batch register attestations
    let result = batch_register_attestations(&mut context, batch_data).unwrap();
    
    // Verify batch registration success
    assert_eq!(result.success_count, 40);
    assert_eq!(result.failed_count, 0);
    
    // Verify executors were registered in their regions
    for region in regions.iter() {
        let executors = get_executors_by_region(&mut context, region).unwrap();
        assert_eq!(executors.len(), 10);
    }
}

#[test]
fn test_batch_verification() {
    let (mut context, attestations, addresses) = setup_discovery_test();
    
    // Register some attestations
    for i in 0..20 {
        register_attestation(&mut context, addresses[i], attestations[i].clone()).unwrap();
    }
    
    // Test verification of a batch of executors
    let to_verify: Vec<Address> = addresses[0..15].to_vec();
    let verification_results = batch_verify_executors(&mut context, to_verify.clone()).unwrap();
    
    // First 15 should be verified, rest should be false
    assert_eq!(verification_results.len(), 15);
    
    for i in 0..15 {
        if i < 15 {
            assert!(verification_results[i], "Executor {} should be verified", i);
        }
    }
    
    // Test verifying unregistered executors
    let unregistered: Vec<Address> = addresses[30..35].to_vec();
    let verification_results = batch_verify_executors(&mut context, unregistered).unwrap();
    
    for result in verification_results {
        assert!(!result, "Unregistered executor should not be verified");
    }
}

#[test]
fn test_regional_cache() {
    let (mut context, attestations, addresses) = setup_discovery_test();
    
    // Register executors with a region
    let region = "us-east".to_string();
    
    for i in 0..20 {
        register_with_region(
            &mut context,
            addresses[i],
            attestations[i].clone(),
            Some(region.clone()),
        ).unwrap();
    }
    
    // Verify region cache works
    let executors = get_executors_by_region(&mut context, &region).unwrap();
    assert_eq!(executors.len(), 20);
    
    // Add more executors to the same region
    for i in 20..30 {
        register_with_region(
            &mut context,
            addresses[i],
            attestations[i].clone(),
            Some(region.clone()),
        ).unwrap();
    }
    
    // Check region info is updated
    let regions = get_all_regions(&mut context).unwrap();
    let region_info = regions.iter().find(|r| r.region_id == region).unwrap();
    
    assert_eq!(region_info.executor_count, 30);
}

#[test]
fn test_performance_comparison() {
    let (mut context, attestations, addresses) = setup_discovery_test();
    
    // Test individual registration performance
    let individual_start = Instant::now();
    
    for i in 0..50 {
        register_attestation(&mut context, addresses[i], attestations[i].clone()).unwrap();
    }
    
    let individual_duration = individual_start.elapsed();
    
    // Clear context and reinitialize
    let mut context = Context::new();
    init(&mut context, Default::default()).unwrap();
    let discovery_params = DiscoveryParams {
        cache_ttl: 3600,
        max_batch_size: 50,
        region_count_limit: 10,
        verification_threshold: 2,
    };
    init_discovery(&mut context, discovery_params).unwrap();
    
    // Test batch registration performance
    let batch_data: Vec<(Address, AttestationReport, Option<String>)> = 
        addresses[0..50].iter()
        .zip(attestations[0..50].iter())
        .map(|(addr, att)| (*addr, att.clone(), None))
        .collect();
    
    let batch_start = Instant::now();
    batch_register_attestations(&mut context, batch_data).unwrap();
    let batch_duration = batch_start.elapsed();
    
    println!("Individual registration time: {:?}", individual_duration);
    println!("Batch registration time: {:?}", batch_duration);
    println!("Speedup factor: {:.2}x", individual_duration.as_micros() as f64 / batch_duration.as_micros() as f64);
    
    // We expect batch processing to be significantly faster
    assert!(batch_duration < individual_duration);
}

#[test]
fn test_discovery_with_regions() {
    let (mut context, attestations, addresses) = setup_discovery_test();
    
    // Register executors across multiple regions
    let regions = ["us-east", "us-west", "eu-central", "ap-southeast"];
    
    // Create a batch with multiple regions
    let mut batch_data = Vec::new();
    
    for i in 0..80 {
        let region_idx = i % regions.len();
        let region = regions[region_idx].to_string();
        
        batch_data.push((
            addresses[i],
            attestations[i].clone(),
            Some(region),
        ));
    }
    
    // Register in batch
    batch_register_attestations(&mut context, batch_data).unwrap();
    
    // Test getting executors by region
    for region in regions.iter() {
        let executors = get_executors_by_region(&mut context, region).unwrap();
        assert_eq!(executors.len(), 20);
    }
    
    // Test batch verification across regions
    let mut to_verify = Vec::new();
    for i in 0..40 {
        to_verify.push(addresses[i]);
    }
    
    let verification_start = Instant::now();
    let results = batch_verify_executors(&mut context, to_verify).unwrap();
    let verification_duration = verification_start.elapsed();
    
    println!("Batch verification time for 40 executors: {:?}", verification_duration);
    
    // All executors should be verified
    assert_eq!(results.len(), 40);
    assert!(results.iter().all(|&r| r));
    
    // Compare with individual verification
    let individual_verify_start = Instant::now();
    
    for i in 0..40 {
        let _ = verify_attestation(&mut context, addresses[i]).unwrap();
    }
    
    let individual_verify_duration = individual_verify_start.elapsed();
    
    println!("Individual verification time for 40 executors: {:?}", individual_verify_duration);
    println!("Verification speedup factor: {:.2}x", 
        individual_verify_duration.as_micros() as f64 / verification_duration.as_micros() as f64);
    
    // Batch verification should be significantly faster
    assert!(verification_duration < individual_verify_duration);
}
