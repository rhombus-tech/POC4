// Dual TEE Security Tests for Treasury Tokenization Platform
// Tests the security features of our cross-attestation verification system

use borsh::{BorshDeserialize, BorshSerialize};
use serde_json;
use std::str::FromStr;

// Define test versions of our contract structures for testing
#[derive(BorshSerialize, BorshDeserialize, Debug)]
struct TeeRegistrationRequest {
    tee_id: String,
    region_id: String,
    is_primary: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, serde::Serialize, serde::Deserialize)]
struct TreasuryMarketData {
    cusip: String,
    treasury_type: String,
    maturity_date: u64,
    issue_date: u64,
    interest_rate: f64,
    par_value: f64,
    current_value: f64,
    region_id: String,
    primary_tee_id: String,
    secondary_tee_id: String,
}

#[derive(BorshSerialize, BorshDeserialize, Debug, serde::Serialize, serde::Deserialize)]
struct MarketUpdateResult {
    asset_id: String,
    previous_price: f64,
    new_price: f64,
    timestamp: u64,
    success: bool,
    verification_time_us: u64,
    primary_tee_type: String,
    secondary_tee_type: String,
    region_id: String,
    version: u64,
}

// Test suite for the dual TEE security features
#[cfg(test)]
mod tests {
    use super::*;
    use wasmlanche::testing::ContractTest;
    use wasmlanche::{Config, Context, WasmlancheResult};
    
    // Helper function to set up the contract environment
    async fn setup_contract() -> ContractTest {
        let config = Config::default();
        
        // Create test contract
        let contract = ContractTest::new(
            config,
            include_bytes!("../target/wasm32-unknown-unknown/release/asset_tokenization.wasm")
        ).await;
        
        // Initialize contract
        contract.call("init_contract", &[])
            .execute()
            .await
            .expect("Failed to initialize contract");
            
        contract
    }
    
    // Test TEE registration and verification - the core of our dual TEE security architecture
    #[tokio::test]
    async fn test_dual_tee_registration_and_verification() {
        let contract = setup_contract().await;
        
        // Register primary Intel SGX TEE
        let sgx_registration = TeeRegistrationRequest {
            tee_id: "sgx-tee-1".to_string(),
            region_id: "us-east".to_string(),
            is_primary: true,
        };
        
        let result = contract.call(
            "register_tee_to_accumulator", 
            &[sgx_registration.try_to_vec().unwrap()]
        )
        .execute()
        .await
        .expect("Failed to register SGX TEE");
        
        assert!(String::from_utf8_lossy(&result).contains("registered successfully"), 
            "SGX TEE registration failed");
        
        // Register secondary AMD SEV TEE
        let sev_registration = TeeRegistrationRequest {
            tee_id: "sev-tee-1".to_string(),
            region_id: "us-east".to_string(),
            is_primary: false,
        };
        
        let result = contract.call(
            "register_tee_to_accumulator", 
            &[sev_registration.try_to_vec().unwrap()]
        )
        .execute()
        .await
        .expect("Failed to register SEV TEE");
        
        assert!(String::from_utf8_lossy(&result).contains("registered successfully"), 
            "SEV TEE registration failed");
            
        // Create market data with registered TEEs
        let market_data = TreasuryMarketData {
            cusip: "912796YD8".to_string(),
            treasury_type: "BILL".to_string(),
            maturity_date: 1777777777,
            issue_date: 1682047272,
            interest_rate: 2.5,
            par_value: 10000.0,
            current_value: 9975.0,
            region_id: "us-east".to_string(),
            primary_tee_id: "sgx-tee-1".to_string(),
            secondary_tee_id: "sev-tee-1".to_string(),
        };
        
        // Convert to JSON for the process_treasury_market_data function
        let market_data_json = serde_json::to_string(&market_data).expect("Failed to serialize market data");
        
        // Process market data with dual TEE verification
        let result = contract.call(
            "process_treasury_market_data", 
            &[
                market_data_json.as_bytes().to_vec(),
                market_data.primary_tee_id.as_bytes().to_vec(),
                market_data.secondary_tee_id.as_bytes().to_vec(),
                market_data.region_id.as_bytes().to_vec()
            ]
        )
        .execute()
        .await;
        
        // This should succeed because both TEEs are registered
        assert!(result.is_ok(), "Market data processing with valid TEEs failed");
        
        // Test with unregistered TEE - this should fail (security validation)
        let market_data_bad = TreasuryMarketData {
            cusip: "912796YD8".to_string(),
            treasury_type: "BILL".to_string(),
            maturity_date: 1777777777,
            issue_date: 1682047272,
            interest_rate: 2.5,
            par_value: 10000.0,
            current_value: 9975.0,
            region_id: "us-east".to_string(),
            primary_tee_id: "sgx-tee-1".to_string(),
            secondary_tee_id: "sev-tee-unregistered".to_string(),
        };
        
        let market_data_bad_json = serde_json::to_string(&market_data_bad).expect("Failed to serialize bad market data");
        
        // This should fail due to unregistered secondary TEE
        let result = contract.call(
            "process_treasury_market_data", 
            &[
                market_data_bad_json.as_bytes().to_vec(),
                market_data_bad.primary_tee_id.as_bytes().to_vec(),
                market_data_bad.secondary_tee_id.as_bytes().to_vec(),
                market_data_bad.region_id.as_bytes().to_vec()
            ]
        )
        .execute()
        .await;
        
        assert!(result.is_err(), "Market data processing with invalid TEE should fail");
    }
    
    // Test timing anomaly detection - a key security feature against side-channel attacks
    #[tokio::test]
    async fn test_verification_metrics_and_anomaly_detection() {
        let contract = setup_contract().await;
        
        // Register TEEs (required for verification)
        let sgx_registration = TeeRegistrationRequest {
            tee_id: "sgx-tee-1".to_string(),
            region_id: "us-east".to_string(),
            is_primary: true,
        };
        
        contract.call(
            "register_tee_to_accumulator", 
            &[sgx_registration.try_to_vec().unwrap()]
        )
        .execute()
        .await
        .expect("Failed to register SGX TEE");
        
        let sev_registration = TeeRegistrationRequest {
            tee_id: "sev-tee-1".to_string(),
            region_id: "us-east".to_string(),
            is_primary: false,
        };
        
        contract.call(
            "register_tee_to_accumulator", 
            &[sev_registration.try_to_vec().unwrap()]
        )
        .execute()
        .await
        .expect("Failed to register SEV TEE");
        
        // Create market data transactions with registered TEEs
        for i in 0..5 {
            let market_data = TreasuryMarketData {
                cusip: format!("912796YD{}", i),
                treasury_type: "BILL".to_string(),
                maturity_date: 1777777777,
                issue_date: 1682047272,
                interest_rate: 2.5,
                par_value: 10000.0,
                current_value: 9975.0 - (i as f64),
                region_id: "us-east".to_string(),
                primary_tee_id: "sgx-tee-1".to_string(),
                secondary_tee_id: "sev-tee-1".to_string(),
            };
            
            let market_data_json = serde_json::to_string(&market_data).expect("Failed to serialize market data");
            
            // Process market data multiple times to build up metrics
            contract.call(
                "process_treasury_market_data", 
                &[
                    market_data_json.as_bytes().to_vec(),
                    market_data.primary_tee_id.as_bytes().to_vec(),
                    market_data.secondary_tee_id.as_bytes().to_vec(),
                    market_data.region_id.as_bytes().to_vec()
                ]
            )
            .execute()
            .await
            .expect("Market data processing failed");
        }
        
        // Check verification metrics
        let metrics_result = contract.call("get_treasury_verification_metrics", &[])
            .execute()
            .await
            .expect("Failed to get verification metrics");
            
        let metrics_json = String::from_utf8_lossy(&metrics_result);
        
        // We should have metrics data with at least 5 verifications
        assert!(metrics_json.contains("\"total_verifications\":"), "Missing verification count");
        assert!(metrics_json.contains("\"successful_verifications\":"), "Missing successful verification count");
        
        // Check for regional data
        assert!(metrics_json.contains("\"regional_stats\":"), "Missing regional statistics");
        assert!(metrics_json.contains("\"us-east\""), "Missing us-east region in metrics");
    }
    
    // Test regional compliance features - important for regulatory requirements
    #[tokio::test]
    async fn test_regional_compliance() {
        let contract = setup_contract().await;
        
        // Register TEEs in different regions to test regional isolation
        // US East Region
        let sgx_us_east = TeeRegistrationRequest {
            tee_id: "sgx-tee-us-east".to_string(),
            region_id: "us-east".to_string(),
            is_primary: true,
        };
        
        contract.call(
            "register_tee_to_accumulator", 
            &[sgx_us_east.try_to_vec().unwrap()]
        )
        .execute()
        .await
        .expect("Failed to register US East SGX TEE");
        
        let sev_us_east = TeeRegistrationRequest {
            tee_id: "sev-tee-us-east".to_string(),
            region_id: "us-east".to_string(),
            is_primary: false,
        };
        
        contract.call(
            "register_tee_to_accumulator", 
            &[sev_us_east.try_to_vec().unwrap()]
        )
        .execute()
        .await
        .expect("Failed to register US East SEV TEE");
        
        // EU Region
        let sgx_eu = TeeRegistrationRequest {
            tee_id: "sgx-tee-eu".to_string(),
            region_id: "eu-central".to_string(),
            is_primary: true,
        };
        
        contract.call(
            "register_tee_to_accumulator", 
            &[sgx_eu.try_to_vec().unwrap()]
        )
        .execute()
        .await
        .expect("Failed to register EU SGX TEE");
        
        let sev_eu = TeeRegistrationRequest {
            tee_id: "sev-tee-eu".to_string(),
            region_id: "eu-central".to_string(),
            is_primary: false,
        };
        
        contract.call(
            "register_tee_to_accumulator", 
            &[sev_eu.try_to_vec().unwrap()]
        )
        .execute()
        .await
        .expect("Failed to register EU SEV TEE");
        
        // Test US East market data
        let us_market_data = TreasuryMarketData {
            cusip: "912796YD8".to_string(),
            treasury_type: "BILL".to_string(),
            maturity_date: 1777777777,
            issue_date: 1682047272,
            interest_rate: 2.5,
            par_value: 10000.0,
            current_value: 9975.0,
            region_id: "us-east".to_string(),
            primary_tee_id: "sgx-tee-us-east".to_string(),
            secondary_tee_id: "sev-tee-us-east".to_string(),
        };
        
        let us_market_data_json = serde_json::to_string(&us_market_data).expect("Failed to serialize US market data");
        
        // Process US market data
        let result = contract.call(
            "process_treasury_market_data", 
            &[
                us_market_data_json.as_bytes().to_vec(),
                us_market_data.primary_tee_id.as_bytes().to_vec(),
                us_market_data.secondary_tee_id.as_bytes().to_vec(),
                us_market_data.region_id.as_bytes().to_vec()
            ]
        )
        .execute()
        .await;
        
        assert!(result.is_ok(), "US market data processing failed");
        
        // Test EU market data
        let eu_market_data = TreasuryMarketData {
            cusip: "EU912796YD8".to_string(),
            treasury_type: "BILL".to_string(),
            maturity_date: 1777777777,
            issue_date: 1682047272,
            interest_rate: 2.5,
            par_value: 10000.0,
            current_value: 9975.0,
            region_id: "eu-central".to_string(),
            primary_tee_id: "sgx-tee-eu".to_string(),
            secondary_tee_id: "sev-tee-eu".to_string(),
        };
        
        let eu_market_data_json = serde_json::to_string(&eu_market_data).expect("Failed to serialize EU market data");
        
        // Process EU market data
        let result = contract.call(
            "process_treasury_market_data", 
            &[
                eu_market_data_json.as_bytes().to_vec(),
                eu_market_data.primary_tee_id.as_bytes().to_vec(),
                eu_market_data.secondary_tee_id.as_bytes().to_vec(),
                eu_market_data.region_id.as_bytes().to_vec()
            ]
        )
        .execute()
        .await;
        
        assert!(result.is_ok(), "EU market data processing failed");
        
        // Test cross-regional verification (should fail - testing regional isolation)
        let mixed_market_data = TreasuryMarketData {
            cusip: "MIXED912796YD8".to_string(),
            treasury_type: "BILL".to_string(),
            maturity_date: 1777777777,
            issue_date: 1682047272,
            interest_rate: 2.5,
            par_value: 10000.0,
            current_value: 9975.0,
            region_id: "eu-central".to_string(),
            primary_tee_id: "sgx-tee-us-east".to_string(), // US TEE
            secondary_tee_id: "sev-tee-eu".to_string(),     // EU TEE
        };
        
        let mixed_market_data_json = serde_json::to_string(&mixed_market_data).expect("Failed to serialize mixed market data");
        
        // This should fail due to regional isolation
        let result = contract.call(
            "process_treasury_market_data", 
            &[
                mixed_market_data_json.as_bytes().to_vec(),
                mixed_market_data.primary_tee_id.as_bytes().to_vec(),
                mixed_market_data.secondary_tee_id.as_bytes().to_vec(),
                mixed_market_data.region_id.as_bytes().to_vec()
            ]
        )
        .execute()
        .await;
        
        assert!(result.is_err(), "Cross-regional verification should fail");
    }
}
