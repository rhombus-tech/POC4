/*!
 * Type definitions for NASDAQ Capital Access Platform integration
 * 
 * These types are based on common financial data structures, and will be
 * updated when the actual NASDAQ API specifications become available.
 */

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// A market data quote
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    /// Symbol of the security
    pub symbol: String,
    
    /// Exchange code
    pub exchange: String,
    
    /// Best bid price
    pub bid: f64,
    
    /// Best bid size
    pub bid_size: u64,
    
    /// Best ask price
    pub ask: f64,
    
    /// Best ask size
    pub ask_size: u64,
    
    /// Timestamp of the quote in milliseconds since epoch
    pub timestamp: u64,
}

/// A market data trade
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    /// Symbol of the security
    pub symbol: String,
    
    /// Exchange code
    pub exchange: String,
    
    /// Trade price
    pub price: f64,
    
    /// Trade size
    pub size: u64,
    
    /// Timestamp of the trade in milliseconds since epoch
    pub timestamp: u64,
    
    /// Trade condition codes
    pub conditions: Vec<String>,
    
    /// Trade ID
    pub trade_id: String,
}

/// An order book snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    /// Symbol of the security
    pub symbol: String,
    
    /// Exchange code
    pub exchange: String,
    
    /// Bid levels (price -> size)
    pub bids: Vec<Level>,
    
    /// Ask levels (price -> size)
    pub asks: Vec<Level>,
    
    /// Timestamp of the order book in milliseconds since epoch
    pub timestamp: u64,
}

/// A price level in an order book
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Level {
    /// Price level
    pub price: f64,
    
    /// Aggregated size at this price level
    pub size: u64,
    
    /// Number of orders at this price level
    pub order_count: Option<u64>,
}

/// A security definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityDefinition {
    /// Symbol of the security
    pub symbol: String,
    
    /// Security description
    pub description: String,
    
    /// Security type (e.g., "CS" for common stock)
    pub security_type: String,
    
    /// Primary exchange
    pub primary_exchange: String,
    
    /// Currency
    pub currency: String,
    
    /// Whether the security is active
    pub active: bool,
    
    /// Additional attributes
    pub attributes: HashMap<String, String>,
}

/// Order creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    /// Symbol of the security
    pub symbol: String,
    
    /// Order side (BUY or SELL)
    pub side: OrderSide,
    
    /// Order type
    pub order_type: OrderType,
    
    /// Order quantity
    pub quantity: f64,
    
    /// Limit price (required for limit orders)
    pub price: Option<f64>,
    
    /// Time in force
    pub time_in_force: TimeInForce,
    
    /// Client order ID
    pub client_order_id: String,
    
    /// Additional parameters
    pub parameters: HashMap<String, String>,
}

/// Order side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    /// Buy order
    #[serde(rename = "BUY")]
    Buy,
    
    /// Sell order
    #[serde(rename = "SELL")]
    Sell,
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderType {
    /// Market order
    #[serde(rename = "MARKET")]
    Market,
    
    /// Limit order
    #[serde(rename = "LIMIT")]
    Limit,
}

/// Time in force
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce {
    /// Day order
    #[serde(rename = "DAY")]
    Day,
    
    /// Good till canceled
    #[serde(rename = "GTC")]
    GoodTillCanceled,
    
    /// Immediate or cancel
    #[serde(rename = "IOC")]
    ImmediateOrCancel,
    
    /// Fill or kill
    #[serde(rename = "FOK")]
    FillOrKill,
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    /// Order is new
    #[serde(rename = "NEW")]
    New,
    
    /// Order is partially filled
    #[serde(rename = "PARTIALLY_FILLED")]
    PartiallyFilled,
    
    /// Order is filled
    #[serde(rename = "FILLED")]
    Filled,
    
    /// Order is canceled
    #[serde(rename = "CANCELED")]
    Canceled,
    
    /// Order is rejected
    #[serde(rename = "REJECTED")]
    Rejected,
}

/// Order response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResponse {
    /// Order ID
    pub order_id: String,
    
    /// Client order ID
    pub client_order_id: String,
    
    /// Symbol of the security
    pub symbol: String,
    
    /// Order side
    pub side: OrderSide,
    
    /// Order type
    pub order_type: OrderType,
    
    /// Order status
    pub status: OrderStatus,
    
    /// Filled quantity
    pub filled_quantity: f64,
    
    /// Remaining quantity
    pub remaining_quantity: f64,
    
    /// Average fill price
    pub average_price: Option<f64>,
    
    /// Timestamp of the order in milliseconds since epoch
    pub timestamp: u64,
}

/// Market data subscription request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionRequest {
    /// Symbols to subscribe to
    pub symbols: Vec<String>,
    
    /// Data types to subscribe to
    pub data_types: Vec<DataType>,
    
    /// Frequency of updates in milliseconds (0 for real-time)
    pub frequency_ms: u64,
    
    /// Whether to include snapshots
    pub include_snapshots: bool,
}

/// Market data types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataType {
    /// Best bid and offer
    #[serde(rename = "QUOTE")]
    Quote,
    
    /// Trade reports
    #[serde(rename = "TRADE")]
    Trade,
    
    /// Order book
    #[serde(rename = "BOOK")]
    Book,
}

/// Market data subscription response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionResponse {
    /// Subscription ID
    pub subscription_id: String,
    
    /// Symbols included in the subscription
    pub symbols: Vec<String>,
    
    /// Data types included in the subscription
    pub data_types: Vec<DataType>,
    
    /// Status of the subscription
    pub status: SubscriptionStatus,
    
    /// Error message if the subscription failed
    pub error_message: Option<String>,
}

/// Subscription status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubscriptionStatus {
    /// Subscription is active
    #[serde(rename = "ACTIVE")]
    Active,
    
    /// Subscription is pending
    #[serde(rename = "PENDING")]
    Pending,
    
    /// Subscription failed
    #[serde(rename = "FAILED")]
    Failed,
    
    /// Subscription is canceled
    #[serde(rename = "CANCELED")]
    Canceled,
}

/// Market data update
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketDataUpdate {
    /// Subscription ID
    pub subscription_id: String,
    
    /// Symbol of the security
    pub symbol: String,
    
    /// Type of data
    pub data_type: DataType,
    
    /// Quote data (if data_type is Quote)
    pub quote: Option<Quote>,
    
    /// Trade data (if data_type is Trade)
    pub trade: Option<Trade>,
    
    /// Order book data (if data_type is Book)
    pub book: Option<OrderBook>,
    
    /// Sequence number
    pub sequence: u64,
    
    /// Timestamp of the update in milliseconds since epoch
    pub timestamp: u64,
}
