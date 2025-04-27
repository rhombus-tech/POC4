// wasmlanche integration tests for the Treasury Tokenization contract
use asset_tokenization::*;
use wasmlanche::{Context};
use wasmlanche::types::{Address, WasmlAddress};
use std::collections::HashMap;

// Helper function to setup a contract context with initialization
fn setup_context() -> Context {
    let address = Address::default();
    // Convert Address to WasmlAddress with proper parameter format handling
    let wasml_address = WasmlAddress::try_from(address.as_bytes()).unwrap();
    let mut context = Context::with_actor(wasml_address);
    
    // Initialize the contract
    init_contract(&mut context).expect("Contract initialization failed");
    
    context
}

#[test]
fn test_init_contract() {
    let address = Address::default();
    // Convert Address to WasmlAddress for proper parameter format handling
    let wasml_address = WasmlAddress::try_from(address.as_bytes()).unwrap();
    let mut context = Context::with_actor(wasml_address);
    
    // Initialize the contract
    let result = init_contract(&mut context);
    
    // Verify initialization succeeded
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), true);
}

#[test]
fn test_tokenize_treasury_asset() {
    let mut context = setup_context();
    
    // Create tokenization request
    let request = TreasuryTokenizationRequest {
        treasury_type: "Bill".to_string(),
        cusip: "912796YD8".to_string(),
        maturity_date: 1680505600, // Unix timestamp for future date
        par_value: 10000.0,
        owner_address: "0xTREASURY_HOLDER".to_string(),
        issuer: "US Treasury".to_string(),
        issue_date: 1649800000,
        primary_tee_id: "SGX-TEE-123456789".to_string(),
        secondary_tee_id: "AMD-SEV-987654321".to_string(),
        region_id: "US-EAST".to_string(),
        coupon_rate: 0.0, // Zero for T-Bills
        maturity_years: 0.25, // 3-month T-Bill
        current_price: 99.75, // Slightly below par for a T-Bill
        name: "US Treasury Bill 3-Month".to_string(),
    };
    
    // Call the tokenize function
    let token_result = tokenize_treasury_asset(&mut context, request);
    
    // Verify the token was created successfully
    assert!(token_result.is_ok());
    
    let token = token_result.unwrap();
    assert!(token.id.len() > 0);
    assert!(token.proof.verified);
    assert_eq!(token.owner, "0xTREASURY_HOLDER");
    assert_eq!(token.amount, 10000.0);
}

#[test]
fn test_register_tee_to_accumulator() {
    let mut context = setup_context();
    
    // Create TEE registration request
    let request = TeeRegistrationRequest {
        tee_id: "SGX-TEE-123456789".to_string(),
        region_id: "US-EAST".to_string(),
        is_primary: true,
        signature: "VALID-SIGNATURE-123".to_string(),
        attestation_report: "SGX-ATTESTATION-REPORT".to_string(),
    };
    
    // Register the TEE
    let result = register_tee_to_accumulator(&mut context, request);
    
    // Verify registration succeeded
    assert!(result.is_ok());
    
    let accumulator = result.unwrap();
    assert!(accumulator.tee_count > 0);
}

#[test]
fn test_verify_cross_attestation() {
    let mut context = setup_context();
    
    // Register primary TEE
    let primary_request = TeeRegistrationRequest {
        tee_id: "SGX-TEE-123456789".to_string(),
        region_id: "US-EAST".to_string(),
        is_primary: true,
        signature: "VALID-SIGNATURE-123".to_string(),
        attestation_report: "SGX-ATTESTATION-REPORT".to_string(),
    };
    register_tee_to_accumulator(&mut context, primary_request).expect("Failed to register primary TEE");
    
    // Register secondary TEE
    let secondary_request = TeeRegistrationRequest {
        tee_id: "AMD-SEV-987654321".to_string(),
        region_id: "US-EAST".to_string(),
        is_primary: false,
        signature: "VALID-SIGNATURE-456".to_string(),
        attestation_report: "AMD-ATTESTATION-REPORT".to_string(),
    };
    register_tee_to_accumulator(&mut context, secondary_request).expect("Failed to register secondary TEE");
    
    // Create verification request
    let request = VerificationRequest {
        primary_tee_id: "SGX-TEE-123456789".to_string(),
        secondary_tee_id: "AMD-SEV-987654321".to_string(),
        region_id: "US-EAST".to_string(),
    };
    
    // Verify cross-attestation
    let result = verify_cross_attestation(&mut context, request);
    
    // Verification should succeed
    assert!(result.is_ok());
    
    let attestation_result = result.unwrap();
    assert!(attestation_result.success);
}

#[test]
fn test_fractionalize_token() {
    let mut context = setup_context();
    
    // First create a token to fractionalize
    let treasury_request = TreasuryTokenizationRequest {
        treasury_type: "Bill".to_string(),
        cusip: "912796YD8".to_string(),
        maturity_date: 1680505600, // Unix timestamp for future date
        par_value: 10000.0,
        owner_address: "0xTREASURY_HOLDER".to_string(),
        issuer: "US Treasury".to_string(),
        issue_date: 1649800000,
        primary_tee_id: "SGX-TEE-123456789".to_string(),
        secondary_tee_id: "AMD-SEV-987654321".to_string(),
        region_id: "US-EAST".to_string(),
    };
    
    // Register TEEs for verification to succeed
    let primary_request = TeeRegistrationRequest {
        tee_id: "SGX-TEE-123456789".to_string(),
        region_id: "US-EAST".to_string(),
        is_primary: true,
        signature: "VALID-SIGNATURE-123".to_string(),
        attestation_report: "SGX-ATTESTATION-REPORT".to_string(),
    };
    register_tee_to_accumulator(&mut context, primary_request).expect("Failed to register primary TEE");
    
    let secondary_request = TeeRegistrationRequest {
        tee_id: "AMD-SEV-987654321".to_string(),
        region_id: "US-EAST".to_string(),
        is_primary: false,
        signature: "VALID-SIGNATURE-456".to_string(),
        attestation_report: "AMD-ATTESTATION-REPORT".to_string(),
    };
    register_tee_to_accumulator(&mut context, secondary_request).expect("Failed to register secondary TEE");
    
    // Create the token
    let token_result = tokenize_treasury_asset(&mut context, treasury_request).expect("Failed to create token");
    
    // Define fractionalization allocations
    let allocations = vec![
        FractionAllocation {
            recipient: "0xFRACTION_OWNER_1".to_string(),
            amount: 5000.0,
        },
        FractionAllocation {
            recipient: "0xFRACTION_OWNER_2".to_string(),
            amount: 5000.0,
        },
    ];
    
    // Create fractionalization request
    let fractionalize_request = FractionalizeRequest {
        token_id: token_result.id.clone(),
        owner: "0xTREASURY_HOLDER".to_string(),
        allocations,
        metadata: HashMap::new(),
        primary_tee_id: "SGX-TEE-123456789".to_string(),
        secondary_tee_id: "AMD-SEV-987654321".to_string(),
    };
    
    // Fractionalize the token
    let result = fractionalize_token(&mut context, fractionalize_request);
    
    // Verify fractionalization succeeded
    assert!(result.is_ok());
    
    let fractional_tokens = result.unwrap();
    assert_eq!(fractional_tokens.len(), 2);
    
    // Verify the fractional tokens
    assert_eq!(fractional_tokens[0].amount, 5000.0);
    assert_eq!(fractional_tokens[1].amount, 5000.0);
    
    assert_eq!(fractional_tokens[0].owner, "0xFRACTION_OWNER_1");
    assert_eq!(fractional_tokens[1].owner, "0xFRACTION_OWNER_2");
}

#[test]
fn test_get_treasury_verification_metrics() {
    let mut context = setup_context();
    
    // Register and verify multiple TEEs to generate metrics
    for i in 0..5 {
        // Register primary TEE
        let primary_tee_id = format!("SGX-TEE-{}", i);
        let primary_request = TeeRegistrationRequest {
            tee_id: primary_tee_id.clone(),
            region_id: "US-EAST".to_string(),
            is_primary: true,
            signature: format!("SIGNATURE-{}", i),
            attestation_report: format!("REPORT-{}", i),
        };
        register_tee_to_accumulator(&mut context, primary_request).expect("Failed to register primary TEE");
        
        // Register secondary TEE
        let secondary_tee_id = format!("AMD-SEV-{}", i);
        let secondary_request = TeeRegistrationRequest {
            tee_id: secondary_tee_id.clone(),
            region_id: "US-EAST".to_string(),
            is_primary: false,
            signature: format!("SIGNATURE-S-{}", i),
            attestation_report: format!("REPORT-S-{}", i),
        };
        register_tee_to_accumulator(&mut context, secondary_request).expect("Failed to register secondary TEE");
        
        // Verify cross-attestation
        let verify_request = VerificationRequest {
            primary_tee_id,
            secondary_tee_id,
            region_id: "US-EAST".to_string(),
        };
        verify_cross_attestation(&mut context, verify_request).expect("Cross-attestation failed");
    }
    
    // Get verification metrics
    let result = get_treasury_verification_metrics(&mut context);
    
    // Verify metrics are available
    assert!(result.is_ok());
    
    let metrics = result.unwrap();
    assert!(metrics.verification_count > 0);
}
