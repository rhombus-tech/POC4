/*!
 * NASDAQ Capital Access Platform client implementation
 * 
 * This client provides specialized integration with NASDAQ's market data and trading APIs.
 * It's designed to work with your cross-regional TEE architecture, transforming market data
 * into contract-compatible formats for your WebAssembly contracts.
 */

use crate::error::{Result, AdapterError};
use crate::adapters::ApiAdapter;
use crate::adapters::types::{ApiRequest, Method};
use crate::protocol::ParameterFormat;
use super::types::*;

/// Client for interacting with NASDAQ's Capital Access Platform
pub struct NasdaqClient {
    /// Underlying API adapter
    adapter: ApiAdapter,
}

impl NasdaqClient {
    /// Create a new NASDAQ client with the given adapter
    pub fn new(adapter: ApiAdapter) -> Self {
        Self { adapter }
    }
    
    /// Get a quote for a security
    pub async fn get_quote(&self, symbol: &str) -> Result<Quote> {
        let request = ApiRequest {
            method: Method::GET,
            path: format!("/v1/quotes/{}", symbol),
            query: Default::default(),
            headers: Default::default(),
            body: None,
            timeout_ms: None,
        };
        
        let response = self.adapter.execute(request).await?;
        
        if response.status != 200 {
            return Err(AdapterError::ResponseParsing(format!(
                "Unexpected status code: {}", response.status
            )).into());
        }
        
        // In a real implementation, this would parse the response from NASDAQ
        // For now, we'll create mock data to support the interface
        Ok(Quote {
            symbol: symbol.to_string(),
            exchange: "NASDAQ".to_string(),
            bid: 150.25,
            bid_size: 100,
            ask: 150.50,
            ask_size: 200,
            timestamp: self.current_timestamp(),
        })
    }
    
    /// Get a trade for a security
    pub async fn get_last_trade(&self, symbol: &str) -> Result<Trade> {
        let request = ApiRequest {
            method: Method::GET,
            path: format!("/v1/trades/{}/last", symbol),
            query: Default::default(),
            headers: Default::default(),
            body: None,
            timeout_ms: None,
        };
        
        let response = self.adapter.execute(request).await?;
        
        if response.status != 200 {
            return Err(AdapterError::ResponseParsing(format!(
                "Unexpected status code: {}", response.status
            )).into());
        }
        
        // In a real implementation, this would parse the response from NASDAQ
        // For now, we'll create mock data to support the interface
        Ok(Trade {
            symbol: symbol.to_string(),
            exchange: "NASDAQ".to_string(),
            price: 150.35,
            size: 100,
            timestamp: self.current_timestamp(),
            conditions: vec!["@".to_string()],
            trade_id: format!("T{}", self.current_timestamp()),
        })
    }
    
    /// Get the order book for a security
    pub async fn get_order_book(&self, symbol: &str, depth: u32) -> Result<OrderBook> {
        let request = ApiRequest {
            method: Method::GET,
            path: format!("/v1/book/{}", symbol),
            query: [("depth".to_string(), depth.to_string())].into(),
            headers: Default::default(),
            body: None,
            timeout_ms: None,
        };
        
        let response = self.adapter.execute(request).await?;
        
        if response.status != 200 {
            return Err(AdapterError::ResponseParsing(format!(
                "Unexpected status code: {}", response.status
            )).into());
        }
        
        // In a real implementation, this would parse the response from NASDAQ
        // For now, we'll create mock data to support the interface
        let timestamp = self.current_timestamp();
        
        let bids = (0..depth).map(|i| {
            Level {
                price: 150.25 - (i as f64 * 0.01),
                size: 100 + (i as u64 * 100),
                order_count: Some(5 + i as u64),
            }
        }).collect();
        
        let asks = (0..depth).map(|i| {
            Level {
                price: 150.50 + (i as f64 * 0.01),
                size: 200 + (i as u64 * 100),
                order_count: Some(7 + i as u64),
            }
        }).collect();
        
        Ok(OrderBook {
            symbol: symbol.to_string(),
            exchange: "NASDAQ".to_string(),
            bids,
            asks,
            timestamp,
        })
    }
    
    /// Subscribe to market data
    pub async fn subscribe_market_data(
        &self,
        symbols: Vec<String>,
        data_types: Vec<DataType>,
    ) -> Result<SubscriptionResponse> {
        let request = ApiRequest {
            method: Method::POST,
            path: "/v1/subscriptions".to_string(),
            query: Default::default(),
            headers: Default::default(),
            body: Some(serde_json::to_value(SubscriptionRequest {
                symbols: symbols.clone(),
                data_types: data_types.clone(),
                frequency_ms: 0,
                include_snapshots: true,
            }).map_err(|e| AdapterError::ResponseParsing(e.to_string()))?),
            timeout_ms: None,
        };
        
        let response = self.adapter.execute(request).await?;
        
        if response.status != 200 && response.status != 201 {
            return Err(AdapterError::ResponseParsing(format!(
                "Unexpected status code: {}", response.status
            )).into());
        }
        
        // In a real implementation, this would parse the response from NASDAQ
        // For now, we'll create mock data to support the interface
        Ok(SubscriptionResponse {
            subscription_id: format!("sub-{}", self.current_timestamp()),
            symbols,
            data_types,
            status: SubscriptionStatus::Active,
            error_message: None,
        })
    }
    
    /// Create a new order
    pub async fn create_order(&self, order: OrderRequest) -> Result<OrderResponse> {
        let request = ApiRequest {
            method: Method::POST,
            path: "/v1/orders".to_string(),
            query: Default::default(),
            headers: Default::default(),
            body: Some(serde_json::to_value(order.clone())
                .map_err(|e| AdapterError::ResponseParsing(e.to_string()))?),
            timeout_ms: None,
        };
        
        let response = self.adapter.execute(request).await?;
        
        if response.status != 200 && response.status != 201 {
            return Err(AdapterError::ResponseParsing(format!(
                "Unexpected status code: {}", response.status
            )).into());
        }
        
        // In a real implementation, this would parse the response from NASDAQ
        // For now, we'll create mock data to support the interface
        Ok(OrderResponse {
            order_id: format!("ord-{}", self.current_timestamp()),
            client_order_id: order.client_order_id,
            symbol: order.symbol,
            side: order.side,
            order_type: order.order_type,
            status: OrderStatus::New,
            filled_quantity: 0.0,
            remaining_quantity: order.quantity,
            average_price: None,
            timestamp: self.current_timestamp(),
        })
    }
    
    /// Transform market data for WebAssembly contract integration
    pub fn transform_market_data<T: serde::Serialize>(
        &self,
        data: &T,
        format: ParameterFormat,
    ) -> Result<Vec<u8>> {
        self.adapter.transform_market_data(data, format)
    }
    
    /// Get the current timestamp in milliseconds
    fn current_timestamp(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::types::{ApiConfig, AuthMethod};
    
    fn create_test_client() -> NasdaqClient {
        let config = ApiConfig {
            base_url: "https://api.nasdaq.com".to_string(),
            auth: AuthMethod::None,
            default_headers: Default::default(),
            timeout_ms: 5000,
            verify_ssl: true,
        };
        
        let adapter = ApiAdapter::new("nasdaq".to_string(), config);
        NasdaqClient::new(adapter)
    }
    
    #[tokio::test]
    async fn test_get_quote() {
        let client = create_test_client();
        let result = client.get_quote("AAPL").await;
        
        assert!(result.is_ok());
        let quote = result.unwrap();
        assert_eq!(quote.symbol, "AAPL");
        assert_eq!(quote.exchange, "NASDAQ");
    }
    
    #[tokio::test]
    async fn test_market_data_transformation() {
        let client = create_test_client();
        let quote = Quote {
            symbol: "AAPL".to_string(),
            exchange: "NASDAQ".to_string(),
            bid: 150.25,
            bid_size: 100,
            ask: 150.50,
            ask_size: 200,
            timestamp: 1618000000000,
        };
        
        // Test length-prefixed format
        let length_prefixed = client.transform_market_data(&quote, ParameterFormat::LengthPrefixed).unwrap();
        assert!(length_prefixed.len() > 4); // Should have length prefix + data
        
        // Test direct format
        let direct = client.transform_market_data(&quote, ParameterFormat::Direct).unwrap();
        
        // Parse and verify data
        let parsed: Quote = serde_json::from_slice(&direct).unwrap();
        assert_eq!(parsed.symbol, "AAPL");
        assert_eq!(parsed.bid, 150.25);
    }
}
