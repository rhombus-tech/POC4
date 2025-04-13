/*!
 * WebAssembly Parameter Format Handling for TEE Integration
 * 
 * This module provides utilities for handling WebAssembly parameter formats
 * used in TEE contract execution. It supports:
 * 
 * 1. Length-prefixed format (4-byte little-endian length + data)
 * 2. Direct data format (no length prefix, fixed size)
 * 
 * It automatically detects which format is being used and provides
 * conversions between them to ensure TEE contracts can work with
 * any consumer regardless of the parameter passing convention.
 */
use crate::protocol::binary_protocol::{ParameterData, ParameterFormatType};
use crate::error::Result;
use tracing::{debug, warn};

/// Maximum allowed parameter size (1MB)
const MAX_PARAMETER_SIZE: usize = 1_048_576;

/// Handle WebAssembly parameter types for TEE contracts
#[derive(Debug, Clone)]
pub struct WasmParameterHandler;

impl WasmParameterHandler {
    /// Parse input data using smart format detection
    ///
    /// This function handles two different parameter passing styles:
    /// 1. Length-prefixed: First 4 bytes are little-endian u32 length followed by data
    /// 2. Direct: Data is passed directly, and is expected to be fixed size
    ///
    /// If expected_direct_size is provided, it will be used when direct format is detected
    pub fn parse_parameters(data: &[u8], expected_direct_size: Option<usize>) -> Result<Vec<u8>> {
        let param_data = ParameterData::parse(data, expected_direct_size);
        debug!("Parameter detected as {:?} format", param_data.format_type);
        
        match param_data.format_type {
            ParameterFormatType::LengthPrefixed => {
                debug!("Length-prefixed parameter: {} bytes", param_data.data.len());
                if param_data.data.len() > MAX_PARAMETER_SIZE {
                    warn!("Parameter size exceeds maximum allowed: {}", param_data.data.len());
                }
            },
            ParameterFormatType::Direct => {
                debug!("Direct parameter: {} bytes", param_data.data.len());
                if let Some(expected) = expected_direct_size {
                    if param_data.data.len() != expected {
                        warn!("Direct parameter size {} doesn't match expected {}", 
                              param_data.data.len(), expected);
                    }
                }
            }
        }
        
        Ok(param_data.data)
    }
    
    /// Format data as length-prefixed parameter
    pub fn to_length_prefixed(data: &[u8]) -> Vec<u8> {
        ParameterData::to_length_prefixed(data)
    }
    
    /// Format data as direct parameter
    pub fn to_direct(data: &[u8]) -> Vec<u8> {
        ParameterData::to_direct(data)
    }
    
    /// Create market data parameter suitable for WebAssembly contracts
    pub fn create_market_data_parameter(symbol: &str, price_data: &[u8]) -> Vec<u8> {
        // Structure: 4-byte length + symbol length byte + symbol bytes + price data
        let mut data = Vec::with_capacity(5 + symbol.len() + price_data.len());
        
        // Add symbol with length prefix (single byte for symbol length)
        data.push(symbol.len() as u8);
        data.extend_from_slice(symbol.as_bytes());
        
        // Add price data
        data.extend_from_slice(price_data);
        
        // Wrap in length-prefixed format for WebAssembly
        Self::to_length_prefixed(&data)
    }
    
    /// Extract symbol and price data from a WebAssembly parameter
    pub fn extract_market_data(param_data: &[u8]) -> Result<(String, Vec<u8>)> {
        // Parse the parameter data first (handling both formats)
        let data = Self::parse_parameters(param_data, None)?;
        
        if data.is_empty() {
            return Err(crate::error::ProtocolError::ParameterFormat("Empty parameter data".to_string()).into());
        }
        
        // First byte is symbol length
        let symbol_len = data[0] as usize;
        if symbol_len == 0 || 1 + symbol_len > data.len() {
            return Err(crate::error::ProtocolError::ParameterFormat("Invalid symbol length in market data parameter".to_string()).into());
        }
        
        // Extract symbol
        let symbol = match std::str::from_utf8(&data[1..1+symbol_len]) {
            Ok(s) => s.to_string(),
            Err(_) => return Err(crate::error::ProtocolError::ParameterFormat("Invalid UTF-8 in symbol".to_string()).into()),
        };
        
        // Extract price data
        let price_data = data[1+symbol_len..].to_vec();
        
        Ok((symbol, price_data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_length_prefixed() {
        // Create length-prefixed data: 4-byte length (5) + "hello"
        let data = vec![5, 0, 0, 0, b'h', b'e', b'l', b'l', b'o'];
        
        let result = WasmParameterHandler::parse_parameters(&data, None).unwrap();
        assert_eq!(result, b"hello");
    }
    
    #[test]
    fn test_parse_direct() {
        // Direct data with no length prefix
        let data = vec![b'w', b'o', b'r', b'l', b'd'];
        
        // With expected size
        let result = WasmParameterHandler::parse_parameters(&data, Some(5)).unwrap();
        assert_eq!(result, b"world");
    }
    
    #[test]
    fn test_market_data_round_trip() {
        let symbol = "AAPL";
        let price_data = vec![1, 2, 3, 4, 5];
        
        // Create parameter
        let param = WasmParameterHandler::create_market_data_parameter(symbol, &price_data);
        
        // Extract data
        let (extracted_symbol, extracted_price) = WasmParameterHandler::extract_market_data(&param).unwrap();
        
        assert_eq!(extracted_symbol, symbol);
        assert_eq!(extracted_price, price_data);
    }
}
