/*!
 * NASDAQ ITCH protocol implementation for high-performance market data
 * 
 * This module provides direct access to NASDAQ's TotalView-ITCH data feed,
 * offering microsecond-level market data with full order book depth.
 * It's optimized for integration with the Aristo TEE mesh architecture.
 * 
 * Key features:
 * - High-performance binary ITCH message parsing
 * - Full order book reconstruction
 * - WebAssembly-compatible parameter formats
 * - Microsecond-level timestamp precision
 * - Support for both live feeds and historical file processing
 */

pub mod client;
pub mod multicast;
pub mod parser;
pub mod realtime;
pub mod types;
pub mod book;
#[cfg(test)]
mod tests;

// Main client for end users
pub use client::ITCHClient;

// Core message types and structures
pub use types::{
    ITCHMessage, OrderBook, MessageType, MessagePayload,
    AddOrderMessage, AddOrderWithMPIDMessage, OrderExecutedMessage,
    OrderExecutedWithPriceMessage, OrderCancelMessage, OrderDeleteMessage,
    OrderReplaceMessage, TradeMessage, CrossTradeMessage, 
    BuySellIndicator, OrderBookEntry
};

// Parser for ITCH binary messages
pub use parser::ITCHParser;

// Order book reconstruction
pub use book::OrderBookReconstructor;
