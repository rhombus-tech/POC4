// Security Tests for Treasury Tokenization Platform
// Tests the dual TEE architecture with cross-attestation verification

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use tokio::runtime::Runtime;
    use wasmlanche::{SimulatorImpl, Address, Context};
    use borsh::{BorshDeserialize, BorshSerialize};
    use serde::{Deserialize, Serialize};
    use serde_json;
    use std::collections::HashMap;

    // Helper function to create a sample market data for testing
    fn create_sample_treasury_data() -> TreasuryMarketData {
        TreasuryMarketData {
            cusip: "912796YD8".to_string(),
            treasury_type: "BILL".to_string(),
            maturity_date: 1777777777, // Future date
            issue_date: 1682047272,   // Past date
            interest_rate: 2.5,
            par_value: 10000.0,
            current_value: 9975.0,
            region_id: "us-east".to_string(),
            primary_tee_id: "sgx-tee-1".to_string(),
            secondary_tee_id: "sev-tee-1".to_string(),
        }
    }

    // Test the core of our security architecture: the attestation accumulator
    #[test]
    fn test_attestation_accumulator() {
        let mut accumulator = AttestationAccumulator::new();
        
        // Test adding TEEs
        assert!(accumulator.add_tee("sgx-tee-1", "us-east", true));
        assert!(accumulator.add_tee("sev-tee-1", "us-east", false));
        
        // Invalid format should be rejected
        assert!(!accumulator.add_tee("invalid-tee", "us-east", true));
        assert!(!accumulator.add_tee("x-sev-tee", "us-east", false));
        
        // Verify both TEEs (cross-attestation)
        let (verified, verification_time) = accumulator.verify_tees("sgx-tee-1", "sev-tee-1", "us-east");
        assert!(verified, "Cross-attestation verification failed");
        
        // Timing should be measured and be reasonable (positive, non-zero)
        assert!(verification_time > 0, "Verification time should be positive");
        
        // Test verification with invalid TEEs
        let (verified, _) = accumulator.verify_tees("sgx-tee-invalid", "sev-tee-1", "us-east");
        assert!(!verified, "Verification should fail with invalid primary TEE");
        
        let (verified, _) = accumulator.verify_tees("sgx-tee-1", "sev-tee-invalid", "us-east");
        assert!(!verified, "Verification should fail with invalid secondary TEE");
        
        // Test verification with invalid region
        let (verified, _) = accumulator.verify_tees("sgx-tee-1", "sev-tee-1", "invalid-region");
        assert!(!verified, "Verification should fail with invalid region");
        
        // Ensure verification count is updated properly
        assert_eq!(accumulator.verification_count, 3, "Verification count should be 3");
    }
    
    // Test the verification metrics system that guards against timing side-channel attacks
    #[test]
    fn test_verification_metrics() {
        let mut metrics = VerificationMetrics::new();
        
        // Record some successful verifications
        for i in 1..=10 {
            // Normal verification times (20-50 microseconds)
            metrics.record_verification(20 + (i * 3), true);
        }
        
        // Record a potential side-channel attack (abnormal verification time)
        metrics.record_verification(500, true);
        
        // Check metrics
        assert_eq!(metrics.total_verifications, 11);
        assert_eq!(metrics.successful_verifications, 11);
        assert_eq!(metrics.failed_verifications, 0);
        
        // Check timing anomaly detection
        assert_eq!(metrics.timing_anomalies, 1, "Should detect 1 timing anomaly");
        
        // Check verification types tracking
        assert!(metrics.verification_types.contains_key("success"));
        assert_eq!(*metrics.verification_types.get("success").unwrap(), 11);
        
        // Test regional statistics
        metrics.update_regional_stats("us-east", 30, true);
        metrics.update_regional_stats("us-east", 40, true);
        metrics.update_regional_stats("us-east", 35, true);
        
        let regional_stats = metrics.regional_stats.get("us-east").unwrap();
        assert_eq!(regional_stats.count, 3);
        assert_eq!(regional_stats.avg_time, 35); // (30+40+35)/3
        
        // Check compliance status calculation
        assert_eq!(regional_stats.compliance_status, ComplianceStatus::Compliant);
        
        // Test failure impact on compliance
        metrics.update_regional_stats("us-west", 30, true);
        metrics.update_regional_stats("us-west", 40, false);
        metrics.update_regional_stats("us-west", 35, false);
        
        let regional_stats = metrics.regional_stats.get("us-west").unwrap();
        assert_eq!(regional_stats.compliance_status, ComplianceStatus::NonCompliant);
    }
    
    // Test serialization of market update results (important for audit trails)
    #[test]
    fn test_market_update_result_serialization() {
        let result = MarketUpdateResult {
            asset_id: "912796YD8".to_string(),
            previous_price: 9980.0,
            new_price: 9975.0,
            timestamp: 1682047272,
            success: true,
            verification_time_us: 35,
            primary_tee_type: "SGX".to_string(),
            secondary_tee_type: "SEV".to_string(),
            region_id: "us-east".to_string(),
            version: 1,
        };
        
        // Serialize to JSON
        let json = serde_json::to_string(&result).expect("Failed to serialize");
        
        // Deserialize back
        let deserialized: MarketUpdateResult = serde_json::from_str(&json).expect("Failed to deserialize");
        
        // Verify fields maintained integrity
        assert_eq!(deserialized.asset_id, "912796YD8");
        assert_eq!(deserialized.previous_price, 9980.0);
        assert_eq!(deserialized.new_price, 9975.0);
        assert_eq!(deserialized.verification_time_us, 35);
        assert_eq!(deserialized.primary_tee_type, "SGX");
        assert_eq!(deserialized.secondary_tee_type, "SEV");
        assert_eq!(deserialized.region_id, "us-east");
        assert_eq!(deserialized.version, 1);
    }
    
    // Test security features of our dual TEE architecture
    #[test]
    fn test_dual_tee_security_features() {
        let mut accumulator = AttestationAccumulator::new();
        
        // Register two TEEs of different types
        accumulator.add_tee("sgx-tee-1", "us-east", true);
        accumulator.add_tee("sev-tee-1", "us-east", false);
        
        // Verify successful attestation
        let (verified, _) = accumulator.verify_tees("sgx-tee-1", "sev-tee-1", "us-east");
        assert!(verified, "Dual TEE verification should succeed with registered TEEs");
        
        // Hardware diversity test: Only one TEE type should fail
        let (verified, _) = accumulator.verify_tees("sgx-tee-1", "sev-tee-2", "us-east");
        assert!(!verified, "Dual TEE verification should fail if either TEE is unregistered");
        
        // Register another region with TEEs
        accumulator.add_tee("sgx-tee-2", "eu-central", true);
        accumulator.add_tee("sev-tee-2", "eu-central", false);
        
        // Verify cross-regional compliance
        let (verified, _) = accumulator.verify_tees("sgx-tee-2", "sev-tee-2", "eu-central");
        assert!(verified, "Cross-regional verification should succeed");
        
        // Test regional isolation (US TEEs shouldn't verify in EU)
        let (verified, _) = accumulator.verify_tees("sgx-tee-1", "sev-tee-1", "eu-central");
        assert!(!verified, "Regional isolation should prevent cross-regional TEE verification");
    }
    
    // Test timing side-channel defenses
    #[test]
    fn test_timing_side_channel_defenses() {
        let mut metrics = VerificationMetrics::new();
        
        // Record normal verifications
        for i in 1..=20 {
            metrics.record_verification(30 + i, true);
        }
        
        // Calculate expected average verification time
        let expected_avg = (30 + 21 + 30 + 20) / 2; // Approx 50
        
        // Record potential timing attack (very slow verification)
        metrics.record_verification(500, true);
        
        // Verify anomaly detection
        assert_eq!(metrics.timing_anomalies, 1, "Should detect timing anomaly");
        assert!(metrics.anomaly_patterns.contains_key("high_latency_0ms"));
        
        // Record several more attacks
        metrics.record_verification(600, true);
        metrics.record_verification(550, true);
        
        // Check pattern recording
        assert_eq!(metrics.timing_anomalies, 3, "Should detect all timing anomalies");
        
        // Regional impact of timing anomalies
        metrics.update_regional_stats("us-east", 30, true);
        metrics.update_regional_stats("us-east", 500, true); // Anomaly
        
        let regional_stats = metrics.regional_stats.get("us-east").unwrap();
        assert_eq!(regional_stats.avg_time, 265); // (30 + 500) / 2
    }
}
