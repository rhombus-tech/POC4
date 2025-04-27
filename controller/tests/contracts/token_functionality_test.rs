use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

// Import from asset tokenization contract
mod asset_tokenization {
    include!("asset_tokenization/src/lib.rs");
}

// Import from market data consumer contract
mod market_data_consumer {
    include!("market_data_consumer/src/lib.rs");
}

/// Test for token functionality with dual TEE cross-attestation
#[test]
fn test_token_functionality_with_dual_tee() {
    println!("Starting token functionality test with dual TEE attestation...");
    
    // Setup test environment
    let mut context = asset_tokenization::Context::new();
    init_test_environment(&mut context);
    
    // Step 1: Create attestation accumulator with dual TEE nodes
    let mut accumulator = TestAttestationAccumulator {
        version: 1,
        tees: vec!["sgx-12345".to_string(), "sev-67890".to_string()],
        regions: vec!["us-east-1".to_string()],
        last_updated: get_current_timestamp(),
    };
    
    // Store accumulator in context
    context.store_state(
        &"accumulator".to_string(), 
        &serde_json::to_string(&accumulator).unwrap()
    ).unwrap();
    
    // Step 2: Create a Treasury asset with dual TEE attestation
    let treasury_asset = create_test_treasury_asset();
    
    // Step 3: Tokenize the Treasury asset with TEE cross-attestation
    let token = asset_tokenization::treasury::create_treasury_token(
        &treasury_asset,
        "0xabcdef1234567890",  // Owner address
        10000.0,               // Amount
        2                      // Fraction precision
    );
    
    // Verify token properties
    assert_eq!(token.asset_symbol, "USTRSY-10Y");
    assert_eq!(token.owner, "0xabcdef1234567890");
    assert_eq!(token.amount, 10000.0);
    assert!(token.proof.primary_attestation.starts_with("sgx-att-"));
    assert!(token.proof.secondary_attestation.starts_with("sev-att-"));
    
    // Step 4: Store token in token store
    let mut token_store = asset_tokenization::TokenStore {
        tokens: BTreeMap::new(),
    };
    token_store.tokens.insert(token.id.clone(), token.clone());
    
    // Serialize and store token store in context
    let token_store_json = serde_json::to_string(&token_store).unwrap();
    context.store_state(&"token_store".to_string(), &token_store_json).unwrap();
    
    // Step 5: Process market data for the tokenized asset with dual TEE
    let market_data = create_test_market_data("USTRSY-10Y");
    let market_data_json = serde_json::to_string(&market_data).unwrap();
    
    let result = asset_tokenization::simulate_nasdaq_market_data(
        &mut context,
        &market_data_json,
        &market_data.primary_tee_id,
        &market_data.secondary_tee_id,
        &market_data.region_id
    );
    
    assert!(result.is_ok(), "Market data processing failed");
    
    // Step 6: Fractionalize the token with cross-attestation security
    let fractions = vec![
        ("0x1111111111111111".to_string(), 5000.0),
        ("0x2222222222222222".to_string(), 5000.0),
    ];
    
    let fractionalization_result = asset_tokenization::treasury::fractionalize_treasury_token(
        &token,
        &fractions,
        &treasury_asset.treasury_info,
        &convert_to_attestation_accumulator(&accumulator)
    );
    
    assert!(fractionalization_result.is_ok(), "Token fractionalization failed");
    
    let fractional_tokens = fractionalization_result.unwrap();
    assert_eq!(fractional_tokens.len(), 2, "Expected 2 fractional tokens");
    
    // Verify fractional tokens
    for (i, fractional_token) in fractional_tokens.iter().enumerate() {
        assert_eq!(fractional_token.asset_symbol, "USTRSY-10Y");
        assert_eq!(fractional_token.amount, 5000.0);
        assert!(fractional_token.parent_id.is_some());
        assert_eq!(fractional_token.parent_id.as_ref().unwrap(), &token.id);
        
        // Verify cross-attestation proof is present
        assert!(fractional_token.proof.primary_attestation.starts_with("sgx-att-"));
        assert!(fractional_token.proof.secondary_attestation.starts_with("sev-att-"));
    }
    
    // Step 7: Update token store with fractional tokens
    for fractional_token in fractional_tokens {
        token_store.tokens.insert(fractional_token.id.clone(), fractional_token);
    }
    
    // Store updated token store
    let updated_token_store_json = serde_json::to_string(&token_store).unwrap();
    context.store_state(&"token_store".to_string(), &updated_token_store_json).unwrap();
    
    // Step 8: Verify token price update from market data
    let updated_market_data = create_test_market_data_with_price_change("USTRSY-10Y", 102.75);
    let updated_market_data_json = serde_json::to_string(&updated_market_data).unwrap();
    
    let updated_result = asset_tokenization::simulate_nasdaq_market_data(
        &mut context,
        &updated_market_data_json,
        &updated_market_data.primary_tee_id,
        &updated_market_data.secondary_tee_id,
        &updated_market_data.region_id
    );
    
    assert!(updated_result.is_ok(), "Updated market data processing failed");
    
    // Parse result and verify price update
    let update_result_str = updated_result.unwrap();
    let update_result: asset_tokenization::MarketUpdateResult = 
        serde_json::from_str(&update_result_str).unwrap();
    
    assert_eq!(update_result.asset_id, "USTRSY-10Y");
    assert_eq!(update_result.new_price, 102.75);
    assert_eq!(update_result.previous_price, 100.0); // Initial price
    assert!(update_result.success);
    assert!(update_result.verification_time_us > 0, "Verification time should be positive");
    
    println!("Token functionality test with dual TEE attestation completed successfully!");
}

/// Test attestation accumulator with simplified methods
#[derive(Serialize, Deserialize)]
struct TestAttestationAccumulator {
    version: u64,
    tees: Vec<String>,
    regions: Vec<String>,
    last_updated: u64,
}

/// Convert test accumulator to contract attestation accumulator
fn convert_to_attestation_accumulator(
    test_acc: &TestAttestationAccumulator
) -> asset_tokenization::AttestationAccumulator {
    asset_tokenization::AttestationAccumulator {
        version: test_acc.version,
        tees: test_acc.tees.clone(),
        regions: test_acc.regions.clone(),
        last_updated: test_acc.last_updated,
    }
}

/// Initialize test environment
fn init_test_environment(context: &mut asset_tokenization::Context) {
    // Initialize price history for test symbol
    let treasury_history = asset_tokenization::AssetHistory {
        symbol: "USTRSY-10Y".to_string(),
        current_price: 100.0,
        last_updated: get_current_timestamp(),
        price_history: vec![
            asset_tokenization::PricePoint {
                price: 100.0,
                timestamp: get_current_timestamp() - 86400, // 1 day ago
            }
        ],
    };
    
    // Store initial asset history
    context.store_state(
        &"price:USTRSY-10Y".to_string(), 
        &serde_json::to_string(&treasury_history).unwrap()
    ).unwrap();
}

/// Create test Treasury asset with dual TEE attestation
fn create_test_treasury_asset() -> asset_tokenization::treasury::TreasuryAsset {
    // Base asset details
    let base = asset_tokenization::Asset {
        symbol: "USTRSY-10Y".to_string(),
        asset_type: "TREASURY".to_string(),
        description: "US Treasury 10-Year Note".to_string(),
        current_price: 100.0,
        last_updated: get_current_timestamp(),
        compliance_info: asset_tokenization::ComplianceInfo {
            region_id: "us-east-1".to_string(),
            status: asset_tokenization::ComplianceStatus::Compliant,
            last_verified: get_current_timestamp(),
            regulatory_notes: "US Treasury securities are compliant for tokenization".to_string(),
        },
    };
    
    // Treasury specific info
    let treasury_info = asset_tokenization::treasury::TreasuryInfo {
        cusip: "912828M56".to_string(),
        treasury_type: asset_tokenization::treasury::TreasuryType::Note,
        maturity_years: 10,
        issue_date: get_current_timestamp() - 86400 * 30, // 30 days ago
        maturity_date: get_current_timestamp() + 86400 * 365 * 10, // 10 years in future
        coupon_rate: 2.5,
        face_value: 1000.0,
        current_yield: 2.5,
    };
    
    // Attestation with dual TEE
    let attestation = asset_tokenization::treasury::TreasuryAttestation {
        sgx_attestation: format!("sgx-att-{}", get_current_timestamp()),
        sev_attestation: format!("sev-att-{}", get_current_timestamp()),
        accumulator_value: format!("acc-{}", get_current_timestamp()),
        verified: true,
        verification_timestamp: get_current_timestamp(),
    };
    
    asset_tokenization::treasury::TreasuryAsset {
        base,
        treasury_info,
        attestation,
    }
}

/// Create test market data for dual TEE testing
fn create_test_market_data(symbol: &str) -> asset_tokenization::NasdaqMarketData {
    asset_tokenization::NasdaqMarketData {
        symbol: symbol.to_string(),
        security_type: "TREASURY".to_string(),
        timestamp: get_current_timestamp(),
        last_price: 100.0,
        best_bid: 99.95,
        best_ask: 100.05,
        bid_size: 1000,
        ask_size: 800,
        volume: 5000000,
        region_id: "us-east-1".to_string(),
        primary_tee_id: "sgx-12345".to_string(),
        secondary_tee_id: "sev-67890".to_string(),
    }
}

/// Create test market data with price change
fn create_test_market_data_with_price_change(symbol: &str, new_price: f64) -> asset_tokenization::NasdaqMarketData {
    let mut data = create_test_market_data(symbol);
    data.last_price = new_price;
    data.best_bid = new_price - 0.05;
    data.best_ask = new_price + 0.05;
    data.timestamp = get_current_timestamp();
    data
}

/// Get current timestamp in seconds
fn get_current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// TokenizationProof structure for dual TEE verification
/// Captures cross-attestation between Intel SGX and AMD SEV
#[derive(Serialize, Deserialize, Clone, Debug)]
struct TokenizationProof {
    pub primary_attestation: String,   // Intel SGX attestation
    pub secondary_attestation: String, // AMD SEV attestation
    pub accumulator_value: String,     // Cryptographic accumulator for hardware-rooted trust
    pub region_id: String,             // Region for regulatory compliance
    pub timestamp: u64,                // Creation timestamp
    pub verified: bool,                // Verification status
}
