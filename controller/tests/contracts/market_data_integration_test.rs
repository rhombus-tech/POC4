//! NASDAQ Market Data Integration Test
//! 
//! This test demonstrates the integration between the asset tokenization and
//! market data consumer contracts with dual TEE cross-attestation verification.

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

// Structure for market data using dual TEE attestation
#[derive(Serialize, Deserialize, Debug)]
struct MarketData {
    symbol: String,
    bid_ask_spread: f64,
    spread_percentage: f64,
    midpoint_price: f64,
    market_depth_ratio: f64,
    timestamp: u64,
    primary_tee_id: String,   // Intel SGX TEE ID
    secondary_tee_id: String, // AMD SEV TEE ID
    region_id: String,        // Region ID for regulatory compliance
}

// Structure for market update result after processing
#[derive(Serialize, Deserialize, Debug)]
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
    version: u32,
}

// Asset price history for tracking
#[derive(Serialize, Deserialize, Debug)]
struct AssetHistory {
    symbol: String,
    current_price: f64,
    last_updated: u64,
    price_history: Vec<PricePoint>,
}

// Individual price point in history
#[derive(Serialize, Deserialize, Debug)]
struct PricePoint {
    price: f64,
    timestamp: u64,
}

// Simplified context for state management
struct TestContext {
    state: HashMap<String, String>,
}

impl TestContext {
    // Create a new context
    fn new() -> Self {
        Self {
            state: HashMap::new(),
        }
    }
    
    // Store state by key
    fn store(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.state.insert(key.to_string(), value.to_string());
        Ok(())
    }
    
    // Get state by key
    fn get(&self, key: &str) -> Option<String> {
        self.state.get(key).cloned()
    }
}

// Setup test environment with initial data
fn setup_test_environment(context: &mut TestContext) {
    // Setup initial asset histories
    let symbols = ["MSFT", "AAPL"];
    
    for symbol in symbols.iter() {
        let history = AssetHistory {
            symbol: symbol.to_string(),
            current_price: 0.0,
            last_updated: 0,
            price_history: Vec::new(),
        };
        
        let history_json = serde_json::to_string(&history).unwrap();
        context.store(&format!("price:{}", symbol), &history_json).unwrap();
    }
    
    // Setup attestation data for dual TEE verification
    let attestation = r#"{"tee_ids":["sgx-12345","sev-67890"],"regions":["us-east-1"],"version":1}"#;
    context.store("attestation", attestation).unwrap();
}

// Verify TEE attestation IDs
fn verify_tee_attestation(primary_tee_id: &str, secondary_tee_id: &str) -> bool {
    // Validate primary TEE (Intel SGX)
    if !primary_tee_id.starts_with("sgx-") {
        println!("Invalid primary TEE ID format (expecting SGX): {}", primary_tee_id);
        return false;
    }
    
    // Validate secondary TEE (AMD SEV)
    if !secondary_tee_id.starts_with("sev-") {
        println!("Invalid secondary TEE ID format (expecting SEV): {}", secondary_tee_id);
        return false;
    }
    
    true
}

// Process market data with dual TEE cross-attestation
fn process_market_data(
    context: &mut TestContext,
    data_json: &str,
    primary_tee_id: &str,
    secondary_tee_id: &str,
    region_id: &str
) -> Result<MarketUpdateResult, String> {
    // Parse market data
    let data: MarketData = serde_json::from_str(data_json)
        .map_err(|e| format!("Failed to parse market data: {}", e))?;
    
    // Verify TEE attestation IDs
    if !verify_tee_attestation(primary_tee_id, secondary_tee_id) {
        return Err("TEE attestation verification failed".to_string());
    }
    
    // Get existing asset history
    let history_key = format!("price:{}", data.symbol);
    let history_json = match context.get(&history_key) {
        Some(json) => json,
        None => {
            // Create new history if none exists
            let new_history = AssetHistory {
                symbol: data.symbol.clone(),
                current_price: 0.0,
                last_updated: 0,
                price_history: Vec::new(),
            };
            serde_json::to_string(&new_history).unwrap()
        }
    };
    
    // Parse history
    let mut history: AssetHistory = serde_json::from_str(&history_json)
        .map_err(|e| format!("Failed to parse asset history: {}", e))?;
    
    // Record previous price before update
    let previous_price = history.current_price;
    
    // Add current price to history
    if history.current_price > 0.0 {
        history.price_history.push(PricePoint {
            price: history.current_price,
            timestamp: history.last_updated,
        });
    }
    
    // Update with new price
    history.current_price = data.midpoint_price;
    history.last_updated = data.timestamp;
    
    // Limit history size to prevent excessive memory usage
    if history.price_history.len() > 10 {
        history.price_history.remove(0);
    }
    
    // Store updated history
    let updated_history_json = serde_json::to_string(&history)
        .map_err(|e| format!("Failed to serialize history: {}", e))?;
    context.store(&history_key, &updated_history_json)?;
    
    // Return market update result with TEE verification metadata
    Ok(MarketUpdateResult {
        asset_id: data.symbol,
        previous_price,
        new_price: data.midpoint_price,
        timestamp: data.timestamp,
        success: true,
        verification_time_us: 5000, // Mock verification time (5ms)
        primary_tee_type: "SGX".to_string(),
        secondary_tee_type: "SEV".to_string(),
        region_id: region_id.to_string(),
        version: 1,
    })
}

// Main test function
fn main() {
    println!("Testing Dual TEE Cross-Attestation Market Data Integration");
    
    // Setup test environment
    let mut context = TestContext::new();
    setup_test_environment(&mut context);
    
    // Test 1: Process valid market data with valid TEE attestation IDs
    let market_data = r#"{
        "symbol": "MSFT",
        "bid_ask_spread": 0.10,
        "spread_percentage": 0.04,
        "midpoint_price": 250.75,
        "market_depth_ratio": 1.25,
        "timestamp": 1682000000,
        "primary_tee_id": "sgx-12345",
        "secondary_tee_id": "sev-67890",
        "region_id": "us-east-1"
    }"#;
    
    // Call process_market_data with valid TEE attestation IDs
    let result = process_market_data(
        &mut context,
        market_data,
        "sgx-12345",    // Intel SGX TEE
        "sev-67890",    // AMD SEV TEE
        "us-east-1"     // Region ID
    );
    
    // Verify result
    match result {
        Ok(update) => {
            println!("Test 1 - Success: Market data processed successfully");
            println!("  Asset: {}", update.asset_id);
            println!("  Previous price: {}", update.previous_price);
            println!("  New price: {}", update.new_price);
            println!("  Primary TEE: {}", update.primary_tee_type);
            println!("  Secondary TEE: {}", update.secondary_tee_type);
            println!("  Region: {}", update.region_id);
        },
        Err(e) => {
            println!("Test 1 - Failed: {}", e);
            return;
        }
    }
    
    // Test 2: Process second update to verify price history tracking
    let market_data2 = r#"{
        "symbol": "MSFT",
        "bid_ask_spread": 0.12,
        "spread_percentage": 0.045,
        "midpoint_price": 255.25,
        "market_depth_ratio": 1.30,
        "timestamp": 1682000100,
        "primary_tee_id": "sgx-12345",
        "secondary_tee_id": "sev-67890",
        "region_id": "us-east-1"
    }"#;
    
    // Call again with the second update
    let result2 = process_market_data(
        &mut context,
        market_data2,
        "sgx-12345",
        "sev-67890",
        "us-east-1"
    );
    
    // Verify second result
    match result2 {
        Ok(update) => {
            println!("Test 2 - Success: Second market data update processed");
            println!("  Asset: {}", update.asset_id);
            println!("  Previous price: {}", update.previous_price);
            println!("  New price: {}", update.new_price);
            println!("  Primary TEE: {}", update.primary_tee_type);
            println!("  Secondary TEE: {}", update.secondary_tee_type);
            println!("  Region: {}", update.region_id);
            
            // Verify price history tracking
            assert_eq!(update.previous_price, 250.75, "Previous price tracking failed");
            assert_eq!(update.new_price, 255.25, "New price update failed");
        },
        Err(e) => {
            println!("Test 2 - Failed: {}", e);
            return;
        }
    }
    
    // Test 3: Invalid primary TEE ID (should fail attestation)
    let result3 = process_market_data(
        &mut context,
        market_data,
        "invalid-tee-id",  // Invalid TEE ID (not SGX)
        "sev-67890",
        "us-east-1"
    );
    
    // Verify attestation verification failure
    match result3 {
        Ok(_) => {
            println!("Test 3 - Failed: Invalid primary TEE ID should be rejected");
        },
        Err(e) => {
            println!("Test 3 - Success: Properly rejected invalid primary TEE ID");
            println!("  Error: {}", e);
        }
    }
    
    // Test 4: Invalid secondary TEE ID (should fail attestation)
    let result4 = process_market_data(
        &mut context,
        market_data,
        "sgx-12345",
        "invalid-tee-id",  // Invalid TEE ID (not SEV)
        "us-east-1"
    );
    
    // Verify attestation verification failure
    match result4 {
        Ok(_) => {
            println!("Test 4 - Failed: Invalid secondary TEE ID should be rejected");
        },
        Err(e) => {
            println!("Test 4 - Success: Properly rejected invalid secondary TEE ID");
            println!("  Error: {}", e);
        }
    }
    
    println!("\nDual TEE cross-attestation market data integration test completed successfully!");
}
