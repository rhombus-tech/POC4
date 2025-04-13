/*!
 * Type definitions for NASDAQ ITCH 5.0 protocol
 * 
 * These types represent the binary structures defined in the NASDAQ ITCH 5.0
 * specification, designed for efficient parsing and TEE integration.
 */

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fmt;

/// Message types defined in NASDAQ ITCH 5.0 protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MessageType {
    SystemEvent = b'S' as isize,
    StockDirectory = b'R' as isize,
    TradingAction = b'H' as isize,
    AddOrder = b'A' as isize,
    AddOrderWithMPID = b'F' as isize,
    OrderExecuted = b'E' as isize,
    OrderExecutedWithPrice = b'C' as isize,
    OrderCancel = b'X' as isize,
    OrderDelete = b'D' as isize,
    OrderReplace = b'U' as isize,
    Trade = b'P' as isize,
    CrossTrade = b'Q' as isize,
    NOII = b'I' as isize,
    RPII = b'N' as isize,
    LULDAuctionCollar = b'J' as isize,
    UnknownType = 0,
}

impl fmt::Display for MessageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = *self as u8 as char;
        write!(f, "{} ({})", c, match self {
            MessageType::SystemEvent => "System Event",
            MessageType::StockDirectory => "Stock Directory",
            MessageType::TradingAction => "Trading Action",
            MessageType::AddOrder => "Add Order",
            MessageType::AddOrderWithMPID => "Add Order with MPID",
            MessageType::OrderExecuted => "Order Executed",
            MessageType::OrderExecutedWithPrice => "Order Executed with Price",
            MessageType::OrderCancel => "Order Cancel",
            MessageType::OrderDelete => "Order Delete",
            MessageType::OrderReplace => "Order Replace",
            MessageType::Trade => "Trade",
            MessageType::CrossTrade => "Cross Trade",
            MessageType::NOII => "NOII",
            MessageType::RPII => "RPII",
            MessageType::LULDAuctionCollar => "LULD Auction Collar",
            MessageType::UnknownType => "Unknown",
        })
    }
}

impl From<u8> for MessageType {
    fn from(byte: u8) -> Self {
        match byte {
            b'S' => MessageType::SystemEvent,
            b'R' => MessageType::StockDirectory,
            b'H' => MessageType::TradingAction,
            b'A' => MessageType::AddOrder,
            b'F' => MessageType::AddOrderWithMPID,
            b'E' => MessageType::OrderExecuted,
            b'C' => MessageType::OrderExecutedWithPrice,
            b'X' => MessageType::OrderCancel,
            b'D' => MessageType::OrderDelete,
            b'U' => MessageType::OrderReplace,
            b'P' => MessageType::Trade,
            b'Q' => MessageType::CrossTrade,
            b'I' => MessageType::NOII,
            b'N' => MessageType::RPII,
            b'J' => MessageType::LULDAuctionCollar,
            _ => MessageType::UnknownType,
        }
    }
}

/// Buy/Sell indicator for orders
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BuySellIndicator {
    Buy,
    Sell,
    Unknown,
}

impl From<u8> for BuySellIndicator {
    fn from(byte: u8) -> Self {
        match byte {
            b'B' => BuySellIndicator::Buy,
            b'S' => BuySellIndicator::Sell,
            _ => BuySellIndicator::Unknown,
        }
    }
}

/// System event codes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemEventCode {
    StartOfMessages,
    StartOfSystemHours,
    StartOfMarketHours,
    EndOfMarketHours,
    EndOfSystemHours,
    EndOfMessages,
    Unknown,
}

impl From<u8> for SystemEventCode {
    fn from(byte: u8) -> Self {
        match byte {
            b'O' => SystemEventCode::StartOfMessages,
            b'S' => SystemEventCode::StartOfSystemHours,
            b'Q' => SystemEventCode::StartOfMarketHours,
            b'M' => SystemEventCode::EndOfMarketHours,
            b'E' => SystemEventCode::EndOfSystemHours,
            b'C' => SystemEventCode::EndOfMessages,
            _ => SystemEventCode::Unknown,
        }
    }
}

/// Trading state for a security
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingState {
    Halted,
    Trading,
    Paused,
    Unknown,
}

impl From<u8> for TradingState {
    fn from(byte: u8) -> Self {
        match byte {
            b'H' => TradingState::Halted,
            b'T' => TradingState::Trading,
            b'P' => TradingState::Paused,
            _ => TradingState::Unknown,
        }
    }
}

/// Base structure for all ITCH messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ITCHMessage {
    /// Message type
    pub message_type: MessageType,
    /// Stock symbol if applicable
    pub stock: Option<String>,
    /// Timestamp in nanoseconds from midnight
    pub timestamp: u64,
    /// Message-specific payload
    pub payload: MessagePayload,
}

/// Enum to hold all possible message payloads
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    SystemEvent(SystemEventMessage),
    StockDirectory(StockDirectoryMessage),
    TradingAction(TradingActionMessage),
    AddOrder(AddOrderMessage),
    AddOrderWithMPID(AddOrderWithMPIDMessage),
    OrderExecuted(OrderExecutedMessage),
    OrderExecutedWithPrice(OrderExecutedWithPriceMessage),
    OrderCancel(OrderCancelMessage),
    OrderDelete(OrderDeleteMessage),
    OrderReplace(OrderReplaceMessage),
    Trade(TradeMessage),
    CrossTrade(CrossTradeMessage),
    Unknown,
}

/// System event message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemEventMessage {
    /// Event code indicating the type of system event
    pub event_code: SystemEventCode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockDirectoryMessage {
    pub stock: String,
    pub market_category: u8,
    pub financial_status_indicator: u8,
    pub round_lot_size: u32,
    pub round_lots_only: bool,
    pub issue_classification: u8,
    pub issue_sub_type: [u8; 2],
    pub authenticity: u8,
    pub short_sale_threshold_indicator: bool,
    pub ipo_flag: bool,
    pub luld_reference_price_tier: u8,
    pub etp_flag: bool,
    pub etp_leverage_factor: u32,
    pub inverse_indicator: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradingActionMessage {
    pub stock: String,
    pub trading_state: TradingState,
    pub reason: [u8; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddOrderMessage {
    pub order_reference_number: u64,
    pub buy_sell_indicator: BuySellIndicator,
    pub shares: u32,
    pub stock: String,
    pub price: u64, // Price in 10^-4 dollars
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddOrderWithMPIDMessage {
    pub order_reference_number: u64,
    pub buy_sell_indicator: BuySellIndicator,
    pub shares: u32,
    pub stock: String,
    pub price: u64, // Price in 10^-4 dollars
    pub attribution: [u8; 4],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderExecutedMessage {
    pub order_reference_number: u64,
    pub executed_shares: u32,
    pub match_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderExecutedWithPriceMessage {
    pub order_reference_number: u64,
    pub executed_shares: u32,
    pub match_number: u64,
    pub printable: bool,
    pub execution_price: u64, // Price in 10^-4 dollars
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderCancelMessage {
    pub order_reference_number: u64,
    pub cancelled_shares: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDeleteMessage {
    pub order_reference_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderReplaceMessage {
    pub original_order_reference_number: u64,
    pub new_order_reference_number: u64,
    pub shares: u32,
    pub price: u64, // Price in 10^-4 dollars
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeMessage {
    pub order_reference_number: u64,
    pub buy_sell_indicator: BuySellIndicator,
    pub shares: u32,
    pub stock: String,
    pub price: u64, // Price in 10^-4 dollars
    pub match_number: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossTradeMessage {
    pub shares: u64,
    pub stock: String,
    pub cross_price: u64, // Price in 10^-4 dollars
    pub match_number: u64,
    pub cross_type: u8,
}

/// Details about an order in the book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderDetails {
    pub order_reference_number: u64,
    pub stock: String,
    pub price: u64,
    pub size: u32,
    pub buy_sell_indicator: BuySellIndicator,
}

/// Order book entry for a given price level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookEntry {
    pub price: f64,  // Price in dollars
    pub size: u32,
    pub order_count: u32,
}

/// Full order book for a stock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub timestamp: u64,
    pub bids: Vec<OrderBookEntry>,
    pub asks: Vec<OrderBookEntry>,
}

impl OrderBook {
    /// Create a new empty order book
    pub fn new() -> Self {
        Self {
            symbol: String::new(),
            timestamp: 0,
            bids: Vec::new(),
            asks: Vec::new(),
        }
    }
    
    /// Process a message and update the order book
    pub fn process_message(&mut self, message: &ITCHMessage) -> Result<(), crate::error::AdapterError> {
        // Update symbol if not set
        if self.symbol.is_empty() {
            if let Some(stock) = &message.stock {
                self.symbol = stock.clone();
            }
        }
        
        // Update timestamp
        self.timestamp = message.timestamp;
        
        // Process based on message type
        match &message.payload {
            // For now, this is a simplified implementation
            // In a real system, you would handle all message types to maintain the order book
            _ => {} // No-op for other message types
        }
        
        Ok(())
    }
    
    /// Get statistics about the order book
    pub fn get_statistics(&self) -> std::collections::HashMap<String, u64> {
        let mut stats = std::collections::HashMap::new();
        stats.insert("timestamp".to_string(), self.timestamp);
        stats.insert("bid_levels".to_string(), self.bids.len() as u64);
        stats.insert("ask_levels".to_string(), self.asks.len() as u64);
        let total_bid_size: u32 = self.bids.iter().map(|level| level.size).sum();
        let total_ask_size: u32 = self.asks.iter().map(|level| level.size).sum();
        stats.insert("total_bid_size".to_string(), total_bid_size as u64);
        stats.insert("total_ask_size".to_string(), total_ask_size as u64);
        stats
    }
    
    /// Reset statistics
    pub fn reset_statistics(&mut self) {
        // Reset any tracking statistics, but keep the core book data
    }
}

/// Helper to convert price from 10^-4 dollars to float
pub fn price_to_float(price: u64) -> f64 {
    (price as f64) / 10000.0
}

/// Helper to convert price from float to 10^-4 dollars
pub fn price_from_float(price: f64) -> u64 {
    (price * 10000.0).round() as u64
}
