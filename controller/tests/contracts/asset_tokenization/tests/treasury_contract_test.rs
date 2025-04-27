// Treasury Tokenization Contract Tests for Dual TEE Architecture
// Tests the wasmlanche contract implementation with special focus on the
// cryptographic accumulator pattern for O(1) verification.

#[cfg(all(test, feature = "simulator"))]
mod tests {
    use std::sync::Arc;
    use tokio::runtime::Runtime;
    use tokio::sync::RwLock;
    use wasmlanche::{
        SimulatorImpl,
        simulator::SimulatorExt,
        Address, Context,
    };
    use borsh::{BorshDeserialize, BorshSerialize};
    use serde::{Deserialize, Serialize};

    // Import our contract definitions - adjust path as needed
    use asset_tokenization::{
        TreasuryTokenizationRequest,
        TeeRegistrationRequest,
        VerificationRequest,
        VerificationMetrics,
    
    // Create contract test environment
    let contract = wasmlanche::testing::ContractTest::new(
        config,
        include_bytes!("../target/wasm32-unknown-unknown/release/asset_tokenization.wasm")
    ).await;
    
    // Initialize the contract
    contract.call("init_contract", &[])
        .execute()
        .await
        .expect("Failed to initialize contract");
    
    contract
            let request = VerificationRequest {
                primary_tee_id: format!("SGX-TEE-{}", i),
                secondary_tee_id: format!("AMD-SEV-{}", i),
                region_id: "US-EAST".to_string(),
            };
            
            // First verification
            simulator.execute_function::<_, VerificationResult>(
                "verify_attestation",
                &[BorshSerialize::try_to_vec(&request).unwrap()]
            ).unwrap();
            
            // Do a second verification for each pair
            let _ = simulator.call_function(
                Address::from_hex("0x1").unwrap(),
                "verify_cross_attestation",
                &verification_request.try_to_vec().unwrap(),
            ).await.unwrap();
            
            // Check final metrics
            let metrics_result = simulator.call_function(
                Address::from_hex("0x1").unwrap(),
                "get_treasury_verification_metrics",
                &vec![],
            ).await.unwrap();
            
            let metrics: VerificationMetrics = BorshDeserialize::deserialize(
                &mut metrics_result.data.as_slice()
            ).unwrap();
            
            // Verify performance metrics
            println!("Performance Benchmark Results:");
            println!("Total verifications: {}", metrics.verification_count);
            println!("Average verification time: {:.2} ms", metrics.avg_verification_time_ms);
            println!("Fastest verification: {} ms", metrics.fastest_verification_ms);
            println!("Slowest verification: {} ms", metrics.slowest_verification_ms);
            
            // These should pass if implementation meets performance targets
            assert!(metrics.avg_verification_time_ms < 100.0, "Average verification time exceeds 100ms target");
            assert!(metrics.verification_count >= 40, "Expected at least 40 verifications");
            
            // Verification metrics should include our specific types
            assert!(metrics.verification_by_type.contains_key("cross-attestation"), 
                   "Missing cross-attestation verification metrics");
            assert!(metrics.verification_by_type.contains_key("tee-registration-sgx"), 
                   "Missing SGX registration metrics");
        });
    }
}
