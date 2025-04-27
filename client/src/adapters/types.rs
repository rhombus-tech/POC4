/*!
 * Types for external API adapters
 */

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Authentication method for external APIs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    /// No authentication
    None,
    
    /// API key authentication
    ApiKey {
        /// Name of the header or parameter
        name: String,
        
        /// Value of the API key
        value: String,
        
        /// Whether to use header (true) or query parameter (false)
        in_header: bool,
    },
    
    /// Bearer token authentication
    BearerToken(String),
    
    /// OAuth2 authentication
    OAuth2 {
        /// Token endpoint
        token_url: String,
        
        /// Client ID
        client_id: String,
        
        /// Client secret
        client_secret: String,
        
        /// Scopes to request
        scopes: Vec<String>,
    },
}

/// Configuration for an external API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiConfig {
    /// Base URL for the API
    pub base_url: String,
    
    /// Authentication method
    pub auth: AuthMethod,
    
    /// Default headers to include in all requests
    pub default_headers: HashMap<String, String>,
    
    /// Default timeout in milliseconds
    pub timeout_ms: u64,
    
    /// Whether to verify SSL certificates
    pub verify_ssl: bool,
}

/// API request method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
}

/// Request to an external API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    /// HTTP method
    pub method: Method,
    
    /// Path relative to the base URL
    pub path: String,
    
    /// Query parameters
    pub query: HashMap<String, String>,
    
    /// Request headers
    pub headers: HashMap<String, String>,
    
    /// Request body
    pub body: Option<serde_json::Value>,
    
    /// Timeout for this specific request (overrides default)
    pub timeout_ms: Option<u64>,
}

/// Response from an external API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    /// HTTP status code
    pub status: u16,
    
    /// Response headers
    pub headers: HashMap<String, String>,
    
    /// Response body as JSON
    pub body: Option<serde_json::Value>,
    
    /// Response body as raw bytes
    pub raw_body: Vec<u8>,
}

/// Typed parameter for API requests
#[derive(Debug, Clone, Serialize)]
pub struct Parameter<T> {
    /// The parameter value
    pub value: T,
    
    /// Parameter metadata
    pub metadata: ParameterMetadata,
}

/// Metadata for API parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterMetadata {
    /// Parameter name
    pub name: String,
    
    /// Parameter description
    pub description: Option<String>,
    
    /// Whether the parameter is required
    pub required: bool,
}
