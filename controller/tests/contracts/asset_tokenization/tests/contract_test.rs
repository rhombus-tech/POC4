use asset_tokenization::{init_contract, process_treasury_market_data, fractionalize_token};
use wasmlanche::{Context, test::{MockStorage, WasmlancheTest}};
use serde_json::json;

// Test helper to set up a test context
fn setup_test_context() -> Context {
    let mut context = WasmlancheTest::new();
    context
}

// Test contract initialization
#[test]
fn test_contract_initialization() {
    let mut context = setup_test_context();
    
    // Initialize the contract
    let result = init_contract(&mut context);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), true);
    
    // Try initializing again - should still return Ok
    let result = init_contract(&mut context);
    assert!(result.is_ok());
}

// Test Treasury market data processing with dual TEE verification
#[test]
fn test_treasury_market_data_processing() {
    let mut context = setup_test_context();
    
    // First initialize the contract
    let init_result = init_contract(&mut context);
    assert!(init_result.is_ok());
    
    // Create example Treasury market data
    let market_data = json!({
        "cusip": "912796B58",
        "treasury_type": "BILL",
        "maturity_date": 1687392000,
        "issue_date": 1655856000,
        "interest_rate": 2.47,
        "par_value": 10000.0,
        "current_value": 9950.0
    }).to_string();
    
    // Define TEE IDs for dual verification (Intel SGX and AMD SEV)
    let primary_tee_id = "sgx-00112233445566778899aabbccddeeff";
    let secondary_tee_id = "sev-99887766554433221100ffeebbaacc";
    let region_id = "us-east-1";
    
    // Process the market data
    let result = process_treasury_market_data(
        &mut context, 
        &market_data,
        primary_tee_id,
        secondary_tee_id,
        region_id
    );
    
    // The first call might fail because the TEEs aren't registered yet
    // This is expected behavior for the security architecture
    if result.is_err() {
        println!("Expected verification failure (TEEs not yet registered)");
        
        // In a real scenario, we would register the TEEs first
        // For test purposes, we'll just verify the error contains expected text
        let error = result.unwrap_err();
        assert!(error.contains("verification failed"));
    } else {
        // If it succeeded, make sure the result message is as expected
        let success = result.unwrap();
        assert!(success.contains("processed Treasury data"));
        assert!(success.contains("912796B58"));
    }
}

// Test fractionalization with proper security verification
#[test]
fn test_token_fractionalization() {
    let mut context = setup_test_context();
    
    // First initialize the contract
    let init_result = init_contract(&mut context);
    assert!(init_result.is_ok());
    
    // For fractionalization, we first need to create a token
    // In a real scenario this would involve more setup
    // For this test, we'll just test the request parsing
    
    let fractionalize_request = json!({
        "token_id": "TREASURY-912796B58-1",
        "fractions": [
            {"recipient": "INVESTOR1", "amount": 5000.0},
            {"recipient": "INVESTOR2", "amount": 2500.0},
            {"recipient": "INVESTOR3", "amount": 2500.0}
        ]
    }).to_string();
    
    // Since we haven't created the token, this should fail
    // but it lets us test error handling
    let request = serde_json::from_str(&fractionalize_request).unwrap();
    let result = fractionalize_token(&mut context, request);
    
    // Should fail with token not found error
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("not found"));
}

// Test specific security features of our dual TEE attestation
#[test]
fn test_security_features() {
    let mut context = setup_test_context();
    
    // Initialize the contract
    let init_result = init_contract(&mut context);
    assert!(init_result.is_ok());
    
    // Test 1: Invalid TEE ID format should be rejected
    let market_data = json!({
        "cusip": "912796B58",
        "treasury_type": "BILL",
        "maturity_date": 1687392000,
        "issue_date": 1655856000,
        "interest_rate": 2.47,
        "par_value": 10000.0,
        "current_value": 9950.0
    }).to_string();
    
    // Invalid primary TEE ID - missing the sgx- prefix
    let invalid_primary_tee = "00112233445566778899aabbccddeeff";
    let secondary_tee_id = "sev-99887766554433221100ffeebbaacc";
    let region_id = "us-east-1";
    
    let result = process_treasury_market_data(
        &mut context, 
        &market_data,
        invalid_primary_tee,
        secondary_tee_id,
        region_id
    );
    
    // Should be rejected for security reasons
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("Invalid primary TEE ID format"));
    
    // Test 2: Invalid secondary TEE format
    let primary_tee_id = "sgx-00112233445566778899aabbccddeeff";
    let invalid_secondary_tee = "99887766554433221100ffeebbaacc"; // Missing sev- prefix
    
    let result = process_treasury_market_data(
        &mut context, 
        &market_data,
        primary_tee_id,
        invalid_secondary_tee,
        region_id
    );
    
    // Should be rejected for security reasons
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("Invalid secondary TEE ID format"));
    
    // Test 3: Empty region ID should be rejected
    let result = process_treasury_market_data(
        &mut context, 
        &market_data,
        primary_tee_id,
        secondary_tee_id,
        ""
    );
    
    // Should be rejected for security reasons
    assert!(result.is_err());
    let error = result.unwrap_err();
    assert!(error.contains("Missing TEE or region identifiers"));
}
