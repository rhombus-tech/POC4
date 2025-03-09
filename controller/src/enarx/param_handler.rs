use log::{debug, info};
use tee_interface::TeeError;

/// Helper struct for handling input parameters to contracts
#[derive(Default)]
pub struct ParamHandler {}

impl ParamHandler {
    /// Create a new instance of ParamHandler
    pub fn new() -> Self {
        Self {}
    }

    /// Process input parameters for contract execution
    /// 
    /// # Arguments
    /// 
    /// * `params` - Raw input parameters
    /// 
    /// # Returns
    /// 
    /// Processed parameters or error
    pub fn process_params(&self, params: &[u8]) -> Result<Vec<u8>, TeeError> {
        if params.is_empty() {
            info!("Empty input parameters received");
            return Ok(vec![]);
        }

        // Log the incoming parameters for debugging
        debug!("Processing input parameters, size: {} bytes", params.len());

        // Check if the parameters start with a length prefix
        if params.len() >= 4 {
            // Try to interpret first 4 bytes as a little-endian u32 length
            let len_bytes = [params[0], params[1], params[2], params[3]];
            let claimed_len = u32::from_le_bytes(len_bytes) as usize;

            // Validate the length
            if claimed_len > 0 && claimed_len <= 1024 && params.len() >= claimed_len + 4 {
                // This is a length-prefixed parameter format
                debug!("Length-prefixed parameter format detected, claimed length: {}", claimed_len);
                let data = &params[4..4 + claimed_len];
                return Ok(data.to_vec());
            }
        }

        // If we can't interpret as length-prefixed, assume direct data format
        debug!("Direct parameter format assumed, returning raw data");
        Ok(params.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_length_prefixed_format() {
        // Create a length-prefixed input
        let data = b"hello world";
        let length = data.len() as u32;
        let mut input = Vec::new();
        input.extend_from_slice(&length.to_le_bytes());
        input.extend_from_slice(data);
        
        // Process it
        let result = ParamHandler::new().process_params(&input).unwrap();
        
        // Check the result
        assert_eq!(result, data);
    }
    
    #[test]
    fn test_direct_format() {
        // Direct data without length prefix
        let data = b"contract_id_123456789012345678901234567890";
        
        // Process it
        let result = ParamHandler::new().process_params(data).unwrap();
        
        // Check that we got all bytes
        assert_eq!(result, data);
    }
    
    #[test]
    fn test_empty_input() {
        let empty: [u8; 0] = [];
        let result = ParamHandler::new().process_params(&empty).unwrap();
        assert_eq!(result, empty);
    }
    
    #[test]
    fn test_invalid_length_prefix() {
        // Create an input with an invalid length prefix (too large)
        let mut input = Vec::new();
        let invalid_length: u32 = 0xFFFFFFFF; // Very large number
        input.extend_from_slice(&invalid_length.to_le_bytes());
        input.extend_from_slice(b"some data");
        
        // Process it
        let result = ParamHandler::new().process_params(&input).unwrap();
        
        // Should treat as direct format since length is unreasonable
        assert_eq!(result, input);
    }
}
