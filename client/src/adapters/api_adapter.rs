/*!
 * API adapter implementation for external service integration
 */

use crate::error::{Result, AdapterError};
use super::types::*;
use std::time::Duration;

/// Adapter for connecting external APIs to the Aristo TEE mesh network
pub struct ApiAdapter {
    /// Name of the adapter
    name: String,
    
    /// Configuration for the API
    config: ApiConfig,
    
    /// HTTP client for making requests
    #[allow(dead_code)]
    client: reqwest::Client,
}

impl ApiAdapter {
    /// Create a new API adapter with the given name and configuration
    pub fn new(name: String, config: ApiConfig) -> Self {
        // Build HTTP client with the appropriate configuration
        let client_builder = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.timeout_ms))
            .default_headers(Self::build_default_headers(&config));
            
        let client_builder = if !config.verify_ssl {
            client_builder.danger_accept_invalid_certs(true)
        } else {
            client_builder
        };
        
        let client = client_builder.build().expect("Failed to build HTTP client");
        
        Self {
            name,
            config,
            client,
        }
    }
    
    /// Execute a request against the external API
    pub async fn execute(&self, request: ApiRequest) -> Result<ApiResponse> {
        // In a real implementation, this would make HTTP requests
        // For now, we'll create a mock response to support the interface
        // This will be replaced with actual implementation when the NASDAQ API is available
        
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        Ok(ApiResponse {
            status: 200,
            headers: [
                ("content-type".to_string(), "application/json".to_string()),
            ].into(),
            body: Some(serde_json::json!({
                "success": true,
                "message": format!("Mocked response from {} adapter", self.name),
                "requestPath": request.path,
                "method": format!("{:?}", request.method),
            })),
            raw_body: b"{\"success\":true}".to_vec(),
        })
    }
    
    /// Transform market data to contract parameters
    pub fn transform_market_data<T: serde::Serialize>(
        &self,
        data: &T,
        format: crate::protocol::ParameterFormat,
    ) -> Result<Vec<u8>> {
        match format {
            crate::protocol::ParameterFormat::LengthPrefixed => {
                // Serialize to JSON
                let json = serde_json::to_vec(data)
                    .map_err(|e| AdapterError::ResponseParsing(e.to_string()))?;
                
                // Prefix with length (4 bytes, little endian)
                let mut result = Vec::with_capacity(4 + json.len());
                result.extend_from_slice(&(json.len() as u32).to_le_bytes());
                result.extend_from_slice(&json);
                
                Ok(result)
            },
            crate::protocol::ParameterFormat::Direct => {
                // Directly serialize to JSON without length prefix
                serde_json::to_vec(data)
                    .map_err(|e| AdapterError::ResponseParsing(e.to_string()).into())
            },
            crate::protocol::ParameterFormat::Empty => {
                // Return empty parameter
                Ok(vec![0, 0, 0, 0])
            },
        }
    }
    
    /// Build default headers from configuration
    fn build_default_headers(config: &ApiConfig) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        
        // Add default headers from configuration
        for (name, value) in &config.default_headers {
            if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(name.as_bytes()) {
                if let Ok(header_value) = reqwest::header::HeaderValue::from_str(value) {
                    headers.insert(header_name, header_value);
                }
            }
        }
        
        // Add authentication headers if applicable
        match &config.auth {
            AuthMethod::ApiKey { name, value, in_header: true } => {
                if let Ok(header_name) = reqwest::header::HeaderName::from_bytes(name.as_bytes()) {
                    if let Ok(header_value) = reqwest::header::HeaderValue::from_str(value) {
                        headers.insert(header_name, header_value);
                    }
                }
            },
            AuthMethod::BearerToken(token) => {
                if let Ok(value) = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token)) {
                    headers.insert(reqwest::header::AUTHORIZATION, value);
                }
            },
            _ => {}, // Other auth methods handled during request execution
        }
        
        headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn create_test_adapter() -> ApiAdapter {
        let config = ApiConfig {
            base_url: "https://api.example.com".to_string(),
            auth: AuthMethod::None,
            default_headers: [
                ("User-Agent".to_string(), "Aristo-Client/0.1".to_string()),
            ].into(),
            timeout_ms: 5000,
            verify_ssl: true,
        };
        
        ApiAdapter::new("test".to_string(), config)
    }
    
    #[tokio::test]
    async fn test_execute_request() {
        let adapter = create_test_adapter();
        
        let request = ApiRequest {
            method: Method::GET,
            path: "/test".to_string(),
            query: Default::default(),
            headers: Default::default(),
            body: None,
            timeout_ms: None,
        };
        
        let response = adapter.execute(request).await.unwrap();
        assert_eq!(response.status, 200);
    }
    
    #[test]
    fn test_transform_market_data_length_prefixed() {
        let adapter = create_test_adapter();
        let data = serde_json::json!({"symbol": "AAPL", "price": 150.42});
        
        let result = adapter.transform_market_data(&data, crate::protocol::ParameterFormat::LengthPrefixed).unwrap();
        
        // First 4 bytes should be the length
        let length_bytes = [result[0], result[1], result[2], result[3]];
        let length = u32::from_le_bytes(length_bytes) as usize;
        
        // Remaining bytes should be the JSON
        let json_bytes = &result[4..];
        assert_eq!(json_bytes.len(), length);
        
        // Parse the JSON to verify it's correct
        let parsed: serde_json::Value = serde_json::from_slice(json_bytes).unwrap();
        assert_eq!(parsed["symbol"], "AAPL");
        assert_eq!(parsed["price"], 150.42);
    }
    
    #[test]
    fn test_transform_market_data_direct() {
        let adapter = create_test_adapter();
        let data = serde_json::json!({"symbol": "AAPL", "price": 150.42});
        
        let result = adapter.transform_market_data(&data, crate::protocol::ParameterFormat::Direct).unwrap();
        
        // Parse the JSON to verify it's correct
        let parsed: serde_json::Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(parsed["symbol"], "AAPL");
        assert_eq!(parsed["price"], 150.42);
    }
}
