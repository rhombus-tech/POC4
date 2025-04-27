// Simple test for Treasury tokenization contract
// This test uses minimal dependencies to avoid version conflicts

#[cfg(test)]
mod tests {
    use asset_tokenization::{
        TreasuryTokenizationRequest,
        TeeRegistrationRequest,
        VerificationRequest,
        Token,
    };
    
    #[test]
    fn test_basic_structures() {
        // Test that we can create and use the basic data structures
        let sgx_request = TeeRegistrationRequest {
            tee_id: "sgx-tee-1".to_string(),
            region_id: "us-east".to_string(),
            tee_type: "sgx".to_string(),
            measurement_hash: "a".repeat(64), // 64 hex characters
            version: "1.0.0".to_string(),
        };
        
        assert_eq!(sgx_request.tee_id, "sgx-tee-1");
        assert_eq!(sgx_request.region_id, "us-east");
        
        let tokenization_request = TreasuryTokenizationRequest {
            cusip: "912796YD8".to_string(),
            name: "US Treasury Bill".to_string(),
            treasury_type: "Bill".to_string(),
            maturity_years: 0.5,
            coupon_rate: 0.0,
            owner: "0xTREASURY_HOLDER".to_string(),
            amount: 10000.0,
            region_id: "us-east".to_string(),
            primary_tee_id: "sgx-tee-1".to_string(),
            secondary_tee_id: "amd-tee-1".to_string(),
            current_price: 99.75,
            fraction_precision: 2,
        };
        
        assert_eq!(tokenization_request.cusip, "912796YD8");
        assert_eq!(tokenization_request.treasury_type, "Bill");
        assert!(tokenization_request.maturity_years > 0.0);
        
        // Test verification request
        let verification_request = VerificationRequest {
            primary_tee_id: "sgx-tee-1".to_string(),
            secondary_tee_id: "amd-tee-1".to_string(),
            region_id: "us-east".to_string(),
        };
        
        assert_eq!(verification_request.primary_tee_id, "sgx-tee-1");
        assert_eq!(verification_request.secondary_tee_id, "amd-tee-1");
    }
}
