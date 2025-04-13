/*!
 * NASDAQ ITCH binary message parser
 * 
 * High-performance parser for NASDAQ ITCH 5.0 binary format.
 * Designed for efficient processing within TEE environments.
 */

use crate::error::{Result, AdapterError};
use crate::protocol::ParameterFormat;
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::convert::TryInto;
use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use super::types::*;

/// Parser for NASDAQ ITCH 5.0 messages
pub struct ITCHParser {
    // Maps order_id to its details
    order_map: HashMap<u64, OrderDetails>,
    // Debug statistics
    pub messages_processed: u64,
    pub messages_by_type: HashMap<MessageType, u64>,
}

impl ITCHParser {
    /// Create a new ITCH parser
    pub fn new() -> Self {
        Self {
            order_map: HashMap::with_capacity(1_000_000), // Pre-allocate for performance
            messages_processed: 0,
            messages_by_type: HashMap::new(),
        }
    }
    
    /// Parse a single ITCH message from a byte slice
    pub fn parse_message(&mut self, data: &[u8]) -> Result<ITCHMessage> {
        if data.is_empty() {
            return Err(AdapterError::ResponseParsing("Empty ITCH message".to_string()).into());
        }
        
        let message_type = MessageType::from(data[0]);
        self.messages_processed += 1;
        
        let entry = self.messages_by_type.entry(message_type).or_insert(0);
        *entry += 1;
        
        let mut cursor = Cursor::new(&data[1..]); // Skip message type byte
        
        // All ITCH messages start with a timestamp
        let timestamp = self.read_timestamp(&mut cursor)?;
        
        let message = match message_type {
            MessageType::SystemEvent => {
                let payload = self.parse_system_event(&mut cursor)?;
                ITCHMessage {
                    message_type,
                    stock: None,
                    timestamp,
                    payload,
                }
            },
            MessageType::StockDirectory => {
                let (stock, payload) = self.parse_stock_directory(&mut cursor)?;
                ITCHMessage {
                    message_type,
                    stock: Some(stock.clone()),
                    timestamp,
                    payload,
                }
            },
            MessageType::TradingAction => {
                let (stock, payload) = self.parse_trading_action(&mut cursor)?;
                ITCHMessage {
                    message_type,
                    stock: Some(stock.clone()),
                    timestamp,
                    payload,
                }
            },
            MessageType::AddOrder => {
                let (stock, payload) = self.parse_add_order(&mut cursor)?;
                
                // Update order map for future reference
                if let MessagePayload::AddOrder(add_order) = &payload {
                    self.order_map.insert(add_order.order_reference_number, OrderDetails {
                        order_reference_number: add_order.order_reference_number,
                        stock: stock.clone(),
                        price: add_order.price,
                        size: add_order.shares,
                        buy_sell_indicator: add_order.buy_sell_indicator,
                    });
                }
                
                ITCHMessage {
                    message_type,
                    stock: Some(stock),
                    timestamp,
                    payload,
                }
            },
            MessageType::AddOrderWithMPID => {
                let (stock, payload) = self.parse_add_order_with_mpid(&mut cursor)?;
                
                // Update order map for future reference
                if let MessagePayload::AddOrderWithMPID(add_order) = &payload {
                    self.order_map.insert(add_order.order_reference_number, OrderDetails {
                        order_reference_number: add_order.order_reference_number,
                        stock: stock.clone(),
                        price: add_order.price,
                        size: add_order.shares,
                        buy_sell_indicator: add_order.buy_sell_indicator,
                    });
                }
                
                ITCHMessage {
                    message_type,
                    stock: Some(stock),
                    timestamp,
                    payload,
                }
            },
            MessageType::OrderExecuted => {
                let payload = self.parse_order_executed(&mut cursor)?;
                
                // Get stock from order map for this order
                let stock = if let MessagePayload::OrderExecuted(order_exec) = &payload {
                    self.order_map.get(&order_exec.order_reference_number)
                        .map(|details| details.stock.clone())
                } else {
                    None
                };
                
                // Update order size in map
                if let MessagePayload::OrderExecuted(order_exec) = &payload {
                    if let Some(details) = self.order_map.get_mut(&order_exec.order_reference_number) {
                        if details.size >= order_exec.executed_shares {
                            details.size -= order_exec.executed_shares;
                        } else {
                            // This can happen with bad data, just set to zero
                            details.size = 0;
                        }
                    }
                }
                
                ITCHMessage {
                    message_type,
                    stock,
                    timestamp,
                    payload,
                }
            },
            MessageType::OrderDelete => {
                let payload = self.parse_order_delete(&mut cursor)?;
                
                // Get stock from order map for this order and then remove the order
                let stock = if let MessagePayload::OrderDelete(order_delete) = &payload {
                    let stock_opt = self.order_map.get(&order_delete.order_reference_number)
                        .map(|details| details.stock.clone());
                    
                    // Remove from map since order is fully deleted
                    self.order_map.remove(&order_delete.order_reference_number);
                    
                    stock_opt
                } else {
                    None
                };
                
                ITCHMessage {
                    message_type,
                    stock,
                    timestamp,
                    payload,
                }
            },
            // Add other message types as needed
            _ => {
                // For unsupported messages, return a placeholder
                ITCHMessage {
                    message_type,
                    stock: None,
                    timestamp,
                    payload: MessagePayload::Unknown,
                }
            }
        };
        
        Ok(message)
    }
    
    /// Read a timestamp from the cursor
    fn read_timestamp(&self, cursor: &mut Cursor<&[u8]>) -> Result<u64> {
        let seconds = cursor.read_u32::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read timestamp seconds: {}", e)))?;
        
        let nanoseconds = cursor.read_u32::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read timestamp nanoseconds: {}", e)))?;
        
        // Convert to nanoseconds since midnight
        Ok((seconds as u64 * 1_000_000_000) + nanoseconds as u64)
    }
    
    /// Read a fixed-length stock symbol
    fn read_stock(&self, cursor: &mut Cursor<&[u8]>) -> Result<String> {
        let mut symbol_bytes = [0u8; 8];
        cursor.read_exact(&mut symbol_bytes)
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read stock symbol: {}", e)))?;
        
        // Trim trailing spaces and convert to string
        let trimmed_bytes: Vec<u8> = symbol_bytes.iter()
            .take_while(|&&b| b != b' ')
            .cloned()
            .collect();
        
        String::from_utf8(trimmed_bytes)
            .map_err(|e| AdapterError::ResponseParsing(format!("Invalid UTF-8 in stock symbol: {}", e)).into())
    }
    
    /// Parse System Event message
    fn parse_system_event(&self, cursor: &mut Cursor<&[u8]>) -> Result<MessagePayload> {
        let event_code_byte = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read system event code: {}", e)))?;
        
        let event_code = SystemEventCode::from(event_code_byte);
        
        Ok(MessagePayload::SystemEvent(SystemEventMessage {
            event_code,
        }))
    }
    
    /// Parse Stock Directory message
    fn parse_stock_directory(&self, cursor: &mut Cursor<&[u8]>) -> Result<(String, MessagePayload)> {
        let stock = self.read_stock(cursor)?;
        
        let market_category = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read market category: {}", e)))?;
        
        let financial_status_indicator = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read financial status: {}", e)))?;
        
        let round_lot_size = cursor.read_u32::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read round lot size: {}", e)))?;
        
        let round_lots_only = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read round lots only: {}", e)))? == b'Y';
        
        let issue_classification = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read issue classification: {}", e)))?;
        
        let mut issue_sub_type = [0u8; 2];
        cursor.read_exact(&mut issue_sub_type)
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read issue subtype: {}", e)))?;
        
        let authenticity = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read authenticity: {}", e)))?;
        
        let short_sale_threshold_indicator = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read short sale threshold: {}", e)))? == b'Y';
        
        let ipo_flag = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read IPO flag: {}", e)))? == b'Y';
        
        let luld_reference_price_tier = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read LULD tier: {}", e)))?;
        
        let etp_flag = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read ETP flag: {}", e)))? == b'Y';
        
        let etp_leverage_factor = cursor.read_u32::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read ETP leverage: {}", e)))?;
        
        let inverse_indicator = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read inverse indicator: {}", e)))? == b'Y';
        
        Ok((stock.clone(), MessagePayload::StockDirectory(StockDirectoryMessage {
            stock,
            market_category,
            financial_status_indicator,
            round_lot_size,
            round_lots_only,
            issue_classification,
            issue_sub_type,
            authenticity,
            short_sale_threshold_indicator,
            ipo_flag,
            luld_reference_price_tier,
            etp_flag,
            etp_leverage_factor,
            inverse_indicator,
        })))
    }
    
    /// Parse Trading Action message
    fn parse_trading_action(&self, cursor: &mut Cursor<&[u8]>) -> Result<(String, MessagePayload)> {
        let stock = self.read_stock(cursor)?;
        
        let trading_state_byte = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read trading state: {}", e)))?;
        
        let trading_state = TradingState::from(trading_state_byte);
        
        let mut reason = [0u8; 4];
        cursor.read_exact(&mut reason)
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read trading state reason: {}", e)))?;
        
        Ok((stock.clone(), MessagePayload::TradingAction(TradingActionMessage {
            stock,
            trading_state,
            reason,
        })))
    }
    
    /// Parse Add Order message
    fn parse_add_order(&self, cursor: &mut Cursor<&[u8]>) -> Result<(String, MessagePayload)> {
        let order_reference_number = cursor.read_u64::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to parse order reference: {}", e)))?;
        
        let buy_sell_indicator_byte = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read buy/sell indicator: {}", e)))?;
        
        let buy_sell_indicator = BuySellIndicator::from(buy_sell_indicator_byte);
        
        let shares = cursor.read_u32::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read shares: {}", e)))?;
        
        let stock = self.read_stock(cursor)?;
        
        let price = cursor.read_u64::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read price: {}", e)))?;
        
        Ok((stock.clone(), MessagePayload::AddOrder(AddOrderMessage {
            order_reference_number,
            buy_sell_indicator,
            shares,
            stock,
            price,
        })))
    }
    
    /// Parse Add Order with MPID message
    fn parse_add_order_with_mpid(&self, cursor: &mut Cursor<&[u8]>) -> Result<(String, MessagePayload)> {
        let order_reference_number = cursor.read_u64::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to parse order reference: {}", e)))?;
        
        let buy_sell_indicator_byte = cursor.read_u8()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read buy/sell indicator: {}", e)))?;
        
        let buy_sell_indicator = BuySellIndicator::from(buy_sell_indicator_byte);
        
        let shares = cursor.read_u32::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read shares: {}", e)))?;
        
        let stock = self.read_stock(cursor)?;
        
        let price = cursor.read_u64::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read price: {}", e)))?;
        
        let mut attribution = [0u8; 4];
        cursor.read_exact(&mut attribution)
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read attribution: {}", e)))?;
        
        Ok((stock.clone(), MessagePayload::AddOrderWithMPID(AddOrderWithMPIDMessage {
            order_reference_number,
            buy_sell_indicator,
            shares,
            stock,
            price,
            attribution,
        })))
    }
    
    /// Parse Order Executed message
    fn parse_order_executed(&self, cursor: &mut Cursor<&[u8]>) -> Result<MessagePayload> {
        let order_reference_number = cursor.read_u64::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to parse order reference: {}", e)))?;
        
        let executed_shares = cursor.read_u32::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read executed shares: {}", e)))?;
        
        let match_number = cursor.read_u64::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to read match number: {}", e)))?;
        
        Ok(MessagePayload::OrderExecuted(OrderExecutedMessage {
            order_reference_number,
            executed_shares,
            match_number,
        }))
    }
    
    /// Parse Order Delete message
    fn parse_order_delete(&self, cursor: &mut Cursor<&[u8]>) -> Result<MessagePayload> {
        let order_reference_number = cursor.read_u64::<BigEndian>()
            .map_err(|e| AdapterError::ResponseParsing(format!("Failed to parse order reference: {}", e)))?;
        
        Ok(MessagePayload::OrderDelete(OrderDeleteMessage {
            order_reference_number,
        }))
    }
    
    /// Get statistics about messages processed
    pub fn get_statistics(&self) -> HashMap<String, u64> {
        let mut stats = HashMap::new();
        stats.insert("total_messages".to_string(), self.messages_processed);
        stats.insert("orders_in_memory".to_string(), self.order_map.len() as u64);
        
        for (msg_type, count) in &self.messages_by_type {
            stats.insert(format!("msg_type_{}", *msg_type as u8 as char), *count);
        }
        
        stats
    }
    
    /// Reset statistics
    pub fn reset_statistics(&mut self) {
        self.messages_processed = 0;
        self.messages_by_type.clear();
    }
    
    /// Transform message to a specific parameter format for WebAssembly contracts
    pub fn transform_message(&self, message: &ITCHMessage, format: ParameterFormat) -> Result<Vec<u8>> {
        self.prepare_for_contract(message, format)
    }
    
    /// Prepare message for contract use with specified parameter format
    /// 
    /// This method is used to transform ITCH messages into WebAssembly contract-compatible formats
    pub fn prepare_for_contract(&self, message: &ITCHMessage, format: ParameterFormat) -> Result<Vec<u8>> {
        match format {
            ParameterFormat::LengthPrefixed => {
                // Serialize to JSON
                let json = serde_json::to_vec(message)
                    .map_err(|e| AdapterError::ResponseParsing(e.to_string()))?;
                
                // Prefix with length (4 bytes, little endian)
                let mut result = Vec::with_capacity(4 + json.len());
                result.extend_from_slice(&(json.len() as u32).to_le_bytes());
                result.extend_from_slice(&json);
                
                Ok(result)
            },
            ParameterFormat::Direct => {
                // Directly serialize to JSON without length prefix
                serde_json::to_vec(message)
                    .map_err(|e| AdapterError::ResponseParsing(e.to_string()).into())
            },
            ParameterFormat::Empty => {
                // Return empty parameter
                Ok(vec![0, 0, 0, 0])
            },
        }
    }
}
