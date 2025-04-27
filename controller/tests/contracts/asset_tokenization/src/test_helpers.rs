//! Test helpers for the asset tokenization contract
//! Provides simplified implementations for testing the dual TEE cross-attestation flow
//! without requiring all the production dependencies

#[cfg(test)]
mod tests;

use alloc::vec::Vec;
use alloc::string::{String, ToString};
use serde::{Serialize, Deserialize};
use alloc::format;
use std::collections::HashMap;

// Re-export structures needed for tests
pub use crate::{MarketUpdateResult, SimulateParams};

/// Simplified context for testing
pub struct TestContext {
    state: HashMap<String, String>,
}

impl TestContext {
    /// Create a new test context
    pub fn new() -> Self {
        Self {
            state: HashMap::new(),
        }
    }
    
    /// Store state by key
    pub fn store(&mut self, key: &str, value: &str) -> Result<(), String> {
        self.state.insert(key.to_string(), value.to_string());
        Ok(())
    }
    
    /// Get state by key
    pub fn get(&self, key: &str) -> Option<String> {
        self.state.get(key).cloned()
    }
}

/// Simplified asset history for testing
#[derive(Serialize, Deserialize)]
pub struct AssetHistory {
    pub symbol: String,
    pub current_price: f64,
    pub last_updated: u64,
    pub price_history: Vec<PricePoint>,
}

/// Price point for history tracking
#[derive(Serialize, Deserialize)]
pub struct PricePoint {
    pub price: f64,
    pub timestamp: u64,
}

/// Simplified TEE attestation for testing
#[derive(Serialize, Deserialize)]
pub struct TestAttestation {
    pub tee_ids: Vec<String>,
    pub regions: Vec<String>,
    pub version: u32,
}

/// Initialize test environment with necessary data
pub fn setup_test_environment(context: &mut TestContext) {
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
    
    // Setup attestation data
    let attestation = TestAttestation {
        tee_ids: vec!["sgx-12345".to_string(), "sev-67890".to_string()],
        regions: vec!["us-east-1".to_string()],
        version: 1,
    };
    
    let attestation_json = serde_json::to_string(&attestation).unwrap();
    context.store("attestation", &attestation_json).unwrap();
}

/// Simplified market data processing function for testing
pub fn process_market_data(
    context: &mut TestContext,
    data_json: &str,
    primary_tee_id: &str,
    secondary_tee_id: &str,
    region_id: &str
) -> Result<MarketUpdateResult, String> {
    // Parse market data
    let data: serde_json::Value = serde_json::from_str(data_json)
        .map_err(|e| format!("Failed to parse market data: {}", e))?;
    
    // Extract data fields
    let symbol = data["symbol"].as_str().unwrap_or("UNKNOWN").to_string();
    let price = data["midpoint_price"].as_f64().unwrap_or(0.0);
    let timestamp = data["timestamp"].as_u64().unwrap_or(0);
    
    // Get existing asset data
    let history_json = context.get(&format!("price:{}", symbol))
        .unwrap_or_else(|| {
            // Create new history if none exists
            let new_history = AssetHistory {
                symbol: symbol.clone(),
                current_price: 0.0,
                last_updated: 0,
                price_history: Vec::new(),
            };
            serde_json::to_string(&new_history).unwrap()
        });
    
    let mut history: AssetHistory = serde_json::from_str(&history_json)
        .map_err(|e| format!("Failed to parse history: {}", e))?;
    
    // Store previous price
    let previous_price = history.current_price;
    
    // Update history
    if history.current_price > 0.0 {
        history.price_history.push(PricePoint {
            price: history.current_price,
            timestamp: history.last_updated,
        });
    }
    
    history.current_price = price;
    history.last_updated = timestamp;
    
    // Store updated history
    let updated_history_json = serde_json::to_string(&history)
        .map_err(|e| format!("Failed to serialize history: {}", e))?;
    context.store(&format!("price:{}", symbol), &updated_history_json)?;
    
    // Return market update result
    Ok(MarketUpdateResult {
        asset_id: symbol,
        previous_price,
        new_price: price,
        timestamp,
        success: true,
        verification_time_us: 5000, // Mock verification time (5ms)
        primary_tee_type: "SGX".to_string(),
        secondary_tee_type: "SEV".to_string(),
        region_id: region_id.to_string(),
        version: 1,
    })
}

/// Get current timestamp (simplified for testing)
pub fn get_test_timestamp() -> u64 {
    #[cfg(test)]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }
    
    #[cfg(not(test))]
    {
        1682000000 // Fixed timestamp for non-test environments
    }
}
