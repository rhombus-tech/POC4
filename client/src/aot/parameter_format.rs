/*!
 * Parameter Format Specialization for WebAssembly Contracts
 * 
 * This module provides ahead-of-time optimization for the two supported parameter formats:
 * 1. Length-prefixed format (4-byte length + data)
 * 2. Direct data format (raw data without length prefix)
 * 
 * Instead of detecting the format at runtime for each message, specialized handlers are
 * pre-compiled for each format, improving performance in high-frequency trading scenarios.
 */

use crate::protocol::types::ParameterFormat;
use crate::error::AdapterError;
use std::collections::HashMap;

/// Format detection strategy
#[derive(Debug, Clone, Copy)]
pub enum FormatDetection {
    /// Dynamically detect format at runtime (slower)
    Dynamic,
    /// Use pre-determined format (faster)
    Static(ParameterFormat),
    /// Use optimized detection based on message type and origin
    Optimized,
}

/// Handler specialized for a specific parameter format
pub struct SpecializedHandler {
    /// Target parameter format
    format: ParameterFormat,
    /// Pre-compiled handler function
    handler: Box<dyn Fn(&[u8]) -> Result<Vec<u8>, AdapterError> + Send + Sync>,
    /// Performance statistics
    stats: HandlerStats,
}

/// Performance statistics for parameter format handlers
#[derive(Debug, Default, Clone)]
pub struct HandlerStats {
    /// Number of times handler was called
    pub calls: usize,
    /// Total processing time in nanoseconds
    pub total_time_ns: u64,
    /// Minimum processing time in nanoseconds
    pub min_time_ns: u64,
    /// Maximum processing time in nanoseconds
    pub max_time_ns: u64,
}

/// Parameter format specialization for WebAssembly contracts
pub struct ParameterSpecialization {
    /// Specialized handlers by format and contract ID
    handlers: HashMap<(ParameterFormat, String), SpecializedHandler>,
    /// Format detection configuration
    detection: FormatDetection,
}

impl ParameterSpecialization {
    /// Create a new parameter specialization system
    pub fn new(detection: FormatDetection) -> Self {
        Self {
            handlers: HashMap::new(),
            detection,
        }
    }
    
    /// Register a specialized handler for a specific format and contract
    pub fn register_handler<F>(&mut self, 
                               format: ParameterFormat, 
                               contract_id: &str, 
                               handler: F) -> Result<(), AdapterError> 
    where
        F: Fn(&[u8]) -> Result<Vec<u8>, AdapterError> + Send + Sync + 'static,
    {
        let key = (format, contract_id.to_string());
        self.handlers.insert(key, SpecializedHandler {
            format,
            handler: Box::new(handler),
            stats: HandlerStats::default(),
        });
        Ok(())
    }
    
    /// Process parameters using the appropriate specialized handler
    pub fn process(&mut self, 
                  contract_id: &str, 
                  parameters: &[u8], 
                  explicit_format: Option<ParameterFormat>) -> Result<Vec<u8>, AdapterError> {
        // Determine format to use
        let format = match explicit_format {
            Some(f) => f,
            None => self.detect_format(parameters)?,
        };
        
        // Try to get specialized handler
        let key = (format, contract_id.to_string());
        if let Some(handler) = self.handlers.get_mut(&key) {
            // Use specialized handler with performance tracking
            let start = std::time::Instant::now();
            let result = (handler.handler)(parameters);
            let elapsed = start.elapsed();
            let elapsed_ns = elapsed.as_nanos() as u64;
            
            // Update stats
            handler.stats.calls += 1;
            handler.stats.total_time_ns += elapsed_ns;
            handler.stats.min_time_ns = if handler.stats.min_time_ns == 0 {
                elapsed_ns
            } else {
                std::cmp::min(handler.stats.min_time_ns, elapsed_ns)
            };
            handler.stats.max_time_ns = std::cmp::max(handler.stats.max_time_ns, elapsed_ns);
            
            result
        } else {
            // Fallback to generic processing
            self.process_generic(parameters, format)
        }
    }
    
    /// Detect the parameter format from the data
    pub fn detect_format(&self, data: &[u8]) -> Result<ParameterFormat, AdapterError> {
        match self.detection {
            FormatDetection::Static(format) => Ok(format),
            FormatDetection::Dynamic => {
                // If data is too short for length prefix, must be direct format
                if data.len() < 4 {
                    return Ok(ParameterFormat::Direct);
                }
                
                // Check if first 4 bytes represent a valid length
                let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                if len > 0 && len <= 1024 && (len as usize) == data.len() - 4 {
                    Ok(ParameterFormat::LengthPrefixed)
                } else {
                    Ok(ParameterFormat::Direct)
                }
            },
            FormatDetection::Optimized => {
                // This would use heuristics based on contract and data patterns
                // For now, default to dynamic detection
                if data.len() < 4 {
                    return Ok(ParameterFormat::Direct);
                }
                
                let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                if len > 0 && len <= 1024 && (len as usize) == data.len() - 4 {
                    Ok(ParameterFormat::LengthPrefixed)
                } else {
                    Ok(ParameterFormat::Direct)
                }
            }
        }
    }
    
    /// Process parameters using generic (non-specialized) logic
    fn process_generic(&self, parameters: &[u8], format: ParameterFormat) -> Result<Vec<u8>, AdapterError> {
        match format {
            ParameterFormat::LengthPrefixed => {
                if parameters.len() < 4 {
                    return Err(AdapterError::ResponseParsing(
                        "Length-prefixed format requires at least 4 bytes".to_string()
                    ));
                }
                
                let len = u32::from_le_bytes([
                    parameters[0], parameters[1], parameters[2], parameters[3]
                ]);
                
                if (len as usize) + 4 != parameters.len() {
                    return Err(AdapterError::ResponseParsing(
                        format!("Declared length {} does not match actual data length {}", 
                               len, parameters.len() - 4)
                    ));
                }
                
                // Process the data after length prefix
                Ok(parameters[4..].to_vec())
            },
            ParameterFormat::Direct => {
                // Process raw data directly
                Ok(parameters.to_vec())
            },
            ParameterFormat::Empty => {
                // Empty parameters case
                Ok(Vec::new())
            }
        }
    }
    
    /// Get performance statistics for all handlers
    pub fn get_statistics(&self) -> HashMap<String, HandlerStats> {
        let mut result = HashMap::new();
        
        for ((format, contract_id), handler) in &self.handlers {
            let key = format!("{:?}_{}", format, contract_id);
            result.insert(key, handler.stats.clone());
        }
        
        result
    }
}
