use log::{debug, info};
use tee_interface::{TeeError, ExecutionPayload};
use std::io::Write;

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
    
    /// Encode an ExecutionPayload into the format expected by the keep manager
    /// 
    /// This combines the function call and parameters into a single binary format:
    /// <function_name>\0<parameters>
    ///
    /// # Arguments
    /// 
    /// * `payload` - The execution payload to encode
    /// 
    /// # Returns
    /// 
    /// Encoded parameters as bytes or error
    pub fn encode(payload: &ExecutionPayload) -> Result<Vec<u8>, TeeError> {
        // Combine the function name and parameters into a single payload
        let mut result = Vec::new();
        
        // Use the function from the payload
        if !payload.params.function_call.is_empty() {
            debug!("Encoding function call: {}", payload.params.function_call);
            
            // Write the function name followed by a null terminator
            result.write_all(payload.params.function_call.as_bytes())
                .map_err(|e| TeeError::ExecutionError(format!("Failed to encode function name: {}", e)))?;
            result.push(0); // Null terminator
        } else {
            // Default to "execute" if no function is specified
            debug!("No function call specified, defaulting to 'execute'");
            result.write_all(b"execute")
                .map_err(|e| TeeError::ExecutionError(format!("Failed to encode default function name: {}", e)))?;
            result.push(0); // Null terminator
        }
        
        // Write the input parameters
        if !payload.input.is_empty() {
            debug!("Encoding {} bytes of input parameters", payload.input.len());
            result.write_all(&payload.input)
                .map_err(|e| TeeError::ExecutionError(format!("Failed to encode parameters: {}", e)))?;
        }
        
        debug!("Encoded payload size: {} bytes", result.len());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_length_prefixed_format() {
        let handler = ParamHandler::new();
        
        // Create a length-prefixed parameter
        let data = b"test data";
        let mut params = Vec::new();
        params.extend_from_slice(&(data.len() as u32).to_le_bytes());
        params.extend_from_slice(data);
        
        let result = handler.process_params(&params).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_direct_format() {
        let handler = ParamHandler::new();
        
        // Direct format parameter
        let params = b"direct data".to_vec();
        
        let result = handler.process_params(&params).unwrap();
        assert_eq!(result, params);
    }

    #[test]
    fn test_empty_input() {
        let handler = ParamHandler::new();
        let result = handler.process_params(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_invalid_length_prefix() {
        let handler = ParamHandler::new();
        
        // Create an invalid length-prefixed parameter (length too large)
        let mut params = Vec::new();
        params.extend_from_slice(&(2000u32).to_le_bytes());
        params.extend_from_slice(b"short data");
        
        let result = handler.process_params(&params).unwrap();
        assert_eq!(result, params);
    }
    
    #[test]
    fn test_encode_payload() {
        // Create a simple ExecutionPayload
        let mut params = tee_interface::ExecutionParams::default();
        params.function_call = "add".to_string();
        
        let payload = tee_interface::ExecutionPayload {
            operation_id: Some("1".to_string()),
            input: vec![1, 2, 3, 4],
            params,
            previous_operation_id: None,
            operation_context: None,
        };
        
        // Encode the payload
        let result = ParamHandler::encode(&payload).unwrap();
        
        // Expected result: "add" + null terminator + [1, 2, 3, 4]
        let mut expected = Vec::new();
        expected.extend_from_slice(b"add");
        expected.push(0);
        expected.extend_from_slice(&[1, 2, 3, 4]);
        
        assert_eq!(result, expected);
    }
    
    #[test]
    fn test_encode_payload_no_function() {
        // Create a simple ExecutionPayload with empty function
        let mut params = tee_interface::ExecutionParams::default();
        params.function_call = "".to_string();
        
        let payload = tee_interface::ExecutionPayload {
            operation_id: Some("1".to_string()),
            input: vec![1, 2, 3, 4],
            params,
            previous_operation_id: None,
            operation_context: None,
        };
        
        // Encode the payload
        let result = ParamHandler::encode(&payload).unwrap();
        
        // Expected result: "execute" + null terminator + [1, 2, 3, 4]
        let mut expected = Vec::new();
        expected.extend_from_slice(b"execute");
        expected.push(0);
        expected.extend_from_slice(&[1, 2, 3, 4]);
        
        assert_eq!(result, expected);
    }
}
