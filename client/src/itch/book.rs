/*!
 * Order book reconstruction from NASDAQ ITCH messages
 * 
 * Efficiently maintains a full-depth limit order book based on
 * real-time ITCH message processing, optimized for TEE environments.
 */

use crate::error::{Result, AdapterError};
use std::collections::{HashMap, BTreeMap};
use super::types::*;

/// Reconstructs and maintains a limit order book from ITCH messages
pub struct OrderBookReconstructor {
    /// Maps stock symbol to its order book
    books: HashMap<String, StockOrderBook>,
    
    /// Maps order ID to its details (for order tracking across messages)
    orders: HashMap<u64, OrderDetails>,
    
    /// Statistics for monitoring
    pub messages_processed: u64,
    pub orders_added: u64,
    pub orders_executed: u64,
    pub orders_deleted: u64,
}

/// Internal representation of a stock's order book
struct StockOrderBook {
    /// Symbol of the stock
    symbol: String,
    
    /// Last update timestamp
    timestamp: u64,
    
    /// Bid side of the book (price -> orders)
    /// Using reverse ordering (highest first) for bids
    bids: BTreeMap<u64, PriceLevel>,
    
    /// Ask side of the book (price -> orders)
    /// Using normal ordering (lowest first) for asks
    asks: BTreeMap<u64, PriceLevel>,
}

/// Represents a single price level in the order book
#[derive(Default)]
struct PriceLevel {
    /// Total size at this price level
    size: u32,
    
    /// Number of orders at this price level
    order_count: u32,
    
    /// Individual orders at this price level (order_id -> size)
    orders: HashMap<u64, u32>,
}

impl OrderBookReconstructor {
    /// Create a new order book reconstructor
    pub fn new() -> Self {
        Self {
            books: HashMap::new(),
            orders: HashMap::with_capacity(1_000_000), // Pre-allocate for performance
            messages_processed: 0,
            orders_added: 0,
            orders_executed: 0,
            orders_deleted: 0,
        }
    }
    
    /// Process a message and update the order book
    pub fn process_message(&mut self, message: &ITCHMessage) -> Result<()> {
        self.messages_processed += 1;
        
        match &message.payload {
            MessagePayload::AddOrder(add_order) => {
                self.process_add_order(message.timestamp, add_order)?;
            },
            MessagePayload::AddOrderWithMPID(add_order) => {
                self.process_add_order_with_mpid(message.timestamp, add_order)?;
            },
            MessagePayload::OrderExecuted(order_exec) => {
                self.process_order_executed(message.timestamp, order_exec)?;
            },
            MessagePayload::OrderExecutedWithPrice(order_exec) => {
                self.process_order_executed_with_price(message.timestamp, order_exec)?;
            },
            MessagePayload::OrderCancel(order_cancel) => {
                self.process_order_cancel(message.timestamp, order_cancel)?;
            },
            MessagePayload::OrderDelete(order_delete) => {
                self.process_order_delete(message.timestamp, order_delete)?;
            },
            MessagePayload::OrderReplace(order_replace) => {
                self.process_order_replace(message.timestamp, order_replace)?;
            },
            // Other message types don't affect the order book
            _ => {},
        }
        
        Ok(())
    }
    
    /// Process an Add Order message
    fn process_add_order(&mut self, timestamp: u64, add_order: &AddOrderMessage) -> Result<()> {
        let order_id = add_order.order_reference_number;
        let price = add_order.price;
        let size = add_order.shares;
        let symbol = &add_order.stock;
        let is_buy = add_order.buy_sell_indicator == BuySellIndicator::Buy;
        
        // Store order details for future reference
        self.orders.insert(order_id, OrderDetails {
            order_reference_number: order_id,
            stock: symbol.clone(),
            price,
            size,
            buy_sell_indicator: add_order.buy_sell_indicator,
        });
        
        // Get or create the book for this stock
        let book = self.books
            .entry(symbol.clone())
            .or_insert_with(|| StockOrderBook::new(symbol.clone()));
        
        // Update the book
        book.timestamp = timestamp;
        
        if is_buy {
            book.add_bid(order_id, price, size);
        } else {
            book.add_ask(order_id, price, size);
        }
        
        self.orders_added += 1;
        
        Ok(())
    }
    
    /// Process an Add Order with MPID message
    fn process_add_order_with_mpid(&mut self, timestamp: u64, add_order: &AddOrderWithMPIDMessage) -> Result<()> {
        let order_id = add_order.order_reference_number;
        let price = add_order.price;
        let size = add_order.shares;
        let symbol = &add_order.stock;
        let is_buy = add_order.buy_sell_indicator == BuySellIndicator::Buy;
        
        // Store order details for future reference
        self.orders.insert(order_id, OrderDetails {
            order_reference_number: order_id,
            stock: symbol.clone(),
            price,
            size,
            buy_sell_indicator: add_order.buy_sell_indicator,
        });
        
        // Get or create the book for this stock
        let book = self.books
            .entry(symbol.clone())
            .or_insert_with(|| StockOrderBook::new(symbol.clone()));
        
        // Update the book
        book.timestamp = timestamp;
        
        if is_buy {
            book.add_bid(order_id, price, size);
        } else {
            book.add_ask(order_id, price, size);
        }
        
        self.orders_added += 1;
        
        Ok(())
    }
    
    /// Process an Order Executed message
    fn process_order_executed(&mut self, timestamp: u64, order_exec: &OrderExecutedMessage) -> Result<()> {
        let order_id = order_exec.order_reference_number;
        let executed_shares = order_exec.executed_shares;
        
        // Find the order details
        let order_details = match self.orders.get(&order_id) {
            Some(details) => details,
            None => return Ok(()), // Order not found, possibly from before we started listening
        };
        
        let symbol = &order_details.stock;
        let price = order_details.price;
        let is_buy = order_details.buy_sell_indicator == BuySellIndicator::Buy;
        
        // Get the book for this stock
        let book = match self.books.get_mut(symbol) {
            Some(book) => book,
            None => return Ok(()), // Book not found, possibly inconsistent state
        };
        
        // Update the book
        book.timestamp = timestamp;
        
        if is_buy {
            book.execute_bid(order_id, price, executed_shares);
        } else {
            book.execute_ask(order_id, price, executed_shares);
        }
        
        // Update the order details
        if let Some(details) = self.orders.get_mut(&order_id) {
            if details.size <= executed_shares {
                // Order fully executed, remove it
                self.orders.remove(&order_id);
            } else {
                // Order partially executed, update size
                details.size -= executed_shares;
            }
        }
        
        self.orders_executed += 1;
        
        Ok(())
    }
    
    /// Process an Order Executed with Price message
    fn process_order_executed_with_price(&mut self, timestamp: u64, order_exec: &OrderExecutedWithPriceMessage) -> Result<()> {
        let order_id = order_exec.order_reference_number;
        let executed_shares = order_exec.executed_shares;
        
        // Find the order details
        let order_details = match self.orders.get(&order_id) {
            Some(details) => details,
            None => return Ok(()), // Order not found, possibly from before we started listening
        };
        
        let symbol = &order_details.stock;
        let price = order_details.price;
        let is_buy = order_details.buy_sell_indicator == BuySellIndicator::Buy;
        
        // Get the book for this stock
        let book = match self.books.get_mut(symbol) {
            Some(book) => book,
            None => return Ok(()), // Book not found, possibly inconsistent state
        };
        
        // Update the book
        book.timestamp = timestamp;
        
        if is_buy {
            book.execute_bid(order_id, price, executed_shares);
        } else {
            book.execute_ask(order_id, price, executed_shares);
        }
        
        // Update the order details
        if let Some(details) = self.orders.get_mut(&order_id) {
            if details.size <= executed_shares {
                // Order fully executed, remove it
                self.orders.remove(&order_id);
            } else {
                // Order partially executed, update size
                details.size -= executed_shares;
            }
        }
        
        self.orders_executed += 1;
        
        Ok(())
    }
    
    /// Process an Order Cancel message
    fn process_order_cancel(&mut self, timestamp: u64, order_cancel: &OrderCancelMessage) -> Result<()> {
        let order_id = order_cancel.order_reference_number;
        let cancelled_shares = order_cancel.cancelled_shares;
        
        // Find the order details
        let order_details = match self.orders.get(&order_id) {
            Some(details) => details,
            None => return Ok(()), // Order not found, possibly from before we started listening
        };
        
        let symbol = &order_details.stock;
        let price = order_details.price;
        let is_buy = order_details.buy_sell_indicator == BuySellIndicator::Buy;
        
        // Get the book for this stock
        let book = match self.books.get_mut(symbol) {
            Some(book) => book,
            None => return Ok(()), // Book not found, possibly inconsistent state
        };
        
        // Update the book
        book.timestamp = timestamp;
        
        if is_buy {
            book.cancel_bid(order_id, price, cancelled_shares);
        } else {
            book.cancel_ask(order_id, price, cancelled_shares);
        }
        
        // Update the order details
        if let Some(details) = self.orders.get_mut(&order_id) {
            if details.size <= cancelled_shares {
                // Order fully cancelled, remove it
                self.orders.remove(&order_id);
            } else {
                // Order partially cancelled, update size
                details.size -= cancelled_shares;
            }
        }
        
        Ok(())
    }
    
    /// Process an Order Delete message
    fn process_order_delete(&mut self, timestamp: u64, order_delete: &OrderDeleteMessage) -> Result<()> {
        let order_id = order_delete.order_reference_number;
        
        // Find the order details
        let order_details = match self.orders.get(&order_id) {
            Some(details) => details,
            None => return Ok(()), // Order not found, possibly from before we started listening
        };
        
        let symbol = &order_details.stock;
        let price = order_details.price;
        let size = order_details.size;
        let is_buy = order_details.buy_sell_indicator == BuySellIndicator::Buy;
        
        // Get the book for this stock
        let book = match self.books.get_mut(symbol) {
            Some(book) => book,
            None => return Ok(()), // Book not found, possibly inconsistent state
        };
        
        // Update the book
        book.timestamp = timestamp;
        
        if is_buy {
            book.delete_bid(order_id, price, size);
        } else {
            book.delete_ask(order_id, price, size);
        }
        
        // Remove the order from our map
        self.orders.remove(&order_id);
        
        self.orders_deleted += 1;
        
        Ok(())
    }
    
    /// Process an Order Replace message
    fn process_order_replace(&mut self, timestamp: u64, order_replace: &OrderReplaceMessage) -> Result<()> {
        let old_order_id = order_replace.original_order_reference_number;
        let new_order_id = order_replace.new_order_reference_number;
        let new_price = order_replace.price;
        let new_size = order_replace.shares;
        
        // Find the old order details
        let old_order = match self.orders.get(&old_order_id) {
            Some(details) => details.clone(),
            None => return Ok(()), // Order not found, possibly from before we started listening
        };
        
        let symbol = old_order.stock.clone();
        let old_price = old_order.price;
        let old_size = old_order.size;
        let is_buy = old_order.buy_sell_indicator == BuySellIndicator::Buy;
        
        // Get the book for this stock
        let book = match self.books.get_mut(&symbol) {
            Some(book) => book,
            None => return Ok(()), // Book not found, possibly inconsistent state
        };
        
        // Update the book
        book.timestamp = timestamp;
        
        // Delete the old order
        if is_buy {
            book.delete_bid(old_order_id, old_price, old_size);
        } else {
            book.delete_ask(old_order_id, old_price, old_size);
        }
        
        // Add the new order
        if is_buy {
            book.add_bid(new_order_id, new_price, new_size);
        } else {
            book.add_ask(new_order_id, new_price, new_size);
        }
        
        // Remove the old order and add the new one to our map
        self.orders.remove(&old_order_id);
        self.orders.insert(new_order_id, OrderDetails {
            order_reference_number: new_order_id,
            stock: symbol,
            price: new_price,
            size: new_size,
            buy_sell_indicator: old_order.buy_sell_indicator,
        });
        
        Ok(())
    }
    
    /// Get the current order book for a stock
    pub fn get_order_book(&self, symbol: &str) -> Option<OrderBook> {
        self.books.get(symbol).map(|book| {
            let bids = book.get_bid_levels();
            let asks = book.get_ask_levels();
            
            OrderBook {
                symbol: symbol.to_string(),
                timestamp: book.timestamp,
                bids,
                asks,
            }
        })
    }
    
    /// Get statistics about the order book
    pub fn get_statistics(&self) -> HashMap<String, u64> {
        let mut stats = HashMap::new();
        stats.insert("messages_processed".to_string(), self.messages_processed);
        stats.insert("orders_added".to_string(), self.orders_added);
        stats.insert("orders_executed".to_string(), self.orders_executed);
        stats.insert("orders_deleted".to_string(), self.orders_deleted);
        stats.insert("active_orders".to_string(), self.orders.len() as u64);
        stats.insert("active_books".to_string(), self.books.len() as u64);
        
        // Get some statistics about book depth
        let mut max_bid_depth = 0;
        let mut max_ask_depth = 0;
        let mut total_bid_depth = 0;
        let mut total_ask_depth = 0;
        
        for book in self.books.values() {
            let bid_depth = book.bids.len();
            let ask_depth = book.asks.len();
            
            max_bid_depth = max_bid_depth.max(bid_depth);
            max_ask_depth = max_ask_depth.max(ask_depth);
            total_bid_depth += bid_depth;
            total_ask_depth += ask_depth;
        }
        
        stats.insert("max_bid_depth".to_string(), max_bid_depth as u64);
        stats.insert("max_ask_depth".to_string(), max_ask_depth as u64);
        stats.insert("total_bid_depth".to_string(), total_bid_depth as u64);
        stats.insert("total_ask_depth".to_string(), total_ask_depth as u64);
        
        if !self.books.is_empty() {
            stats.insert("avg_bid_depth".to_string(), total_bid_depth as u64 / self.books.len() as u64);
            stats.insert("avg_ask_depth".to_string(), total_ask_depth as u64 / self.books.len() as u64);
        }
        
        stats
    }
    
    /// Get all active symbols
    pub fn get_symbols(&self) -> Vec<String> {
        self.books.keys().cloned().collect()
    }
    
    /// Get all order books
    pub fn get_all_books(&self) -> &HashMap<String, StockOrderBook> {
        &self.books
    }
    
    /// Reset statistics
    pub fn reset_statistics(&mut self) {
        self.messages_processed = 0;
        self.orders_added = 0;
        self.orders_executed = 0;
        self.orders_deleted = 0;
    }
}

impl StockOrderBook {
    /// Create a new stock order book
    fn new(symbol: String) -> Self {
        Self {
            symbol,
            timestamp: 0,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
        }
    }
    
    /// Add a bid order
    fn add_bid(&mut self, order_id: u64, price: u64, size: u32) {
        let level = self.bids.entry(price).or_insert_with(PriceLevel::default);
        level.size += size;
        level.order_count += 1;
        level.orders.insert(order_id, size);
    }
    
    /// Add an ask order
    fn add_ask(&mut self, order_id: u64, price: u64, size: u32) {
        let level = self.asks.entry(price).or_insert_with(PriceLevel::default);
        level.size += size;
        level.order_count += 1;
        level.orders.insert(order_id, size);
    }
    
    /// Execute a bid order
    fn execute_bid(&mut self, order_id: u64, price: u64, size: u32) {
        if let Some(level) = self.bids.get_mut(&price) {
            if let Some(order_size) = level.orders.get_mut(&order_id) {
                // Reduce the order size
                let new_size = if *order_size > size { *order_size - size } else { 0 };
                
                // Update the level size
                level.size = level.size.saturating_sub(size);
                
                if new_size == 0 {
                    // Order fully executed, remove it
                    level.orders.remove(&order_id);
                    level.order_count = level.order_count.saturating_sub(1);
                    
                    // If level is empty, remove it
                    if level.order_count == 0 {
                        self.bids.remove(&price);
                    }
                } else {
                    // Order partially executed, update size
                    *order_size = new_size;
                }
            }
        }
    }
    
    /// Execute an ask order
    fn execute_ask(&mut self, order_id: u64, price: u64, size: u32) {
        if let Some(level) = self.asks.get_mut(&price) {
            if let Some(order_size) = level.orders.get_mut(&order_id) {
                // Reduce the order size
                let new_size = if *order_size > size { *order_size - size } else { 0 };
                
                // Update the level size
                level.size = level.size.saturating_sub(size);
                
                if new_size == 0 {
                    // Order fully executed, remove it
                    level.orders.remove(&order_id);
                    level.order_count = level.order_count.saturating_sub(1);
                    
                    // If level is empty, remove it
                    if level.order_count == 0 {
                        self.asks.remove(&price);
                    }
                } else {
                    // Order partially executed, update size
                    *order_size = new_size;
                }
            }
        }
    }
    
    /// Cancel a bid order
    fn cancel_bid(&mut self, order_id: u64, price: u64, size: u32) {
        if let Some(level) = self.bids.get_mut(&price) {
            if let Some(order_size) = level.orders.get_mut(&order_id) {
                // Reduce the order size
                let new_size = if *order_size > size { *order_size - size } else { 0 };
                
                // Update the level size
                level.size = level.size.saturating_sub(size);
                
                if new_size == 0 {
                    // Order fully cancelled, remove it
                    level.orders.remove(&order_id);
                    level.order_count = level.order_count.saturating_sub(1);
                    
                    // If level is empty, remove it
                    if level.order_count == 0 {
                        self.bids.remove(&price);
                    }
                } else {
                    // Order partially cancelled, update size
                    *order_size = new_size;
                }
            }
        }
    }
    
    /// Cancel an ask order
    fn cancel_ask(&mut self, order_id: u64, price: u64, size: u32) {
        if let Some(level) = self.asks.get_mut(&price) {
            if let Some(order_size) = level.orders.get_mut(&order_id) {
                // Reduce the order size
                let new_size = if *order_size > size { *order_size - size } else { 0 };
                
                // Update the level size
                level.size = level.size.saturating_sub(size);
                
                if new_size == 0 {
                    // Order fully cancelled, remove it
                    level.orders.remove(&order_id);
                    level.order_count = level.order_count.saturating_sub(1);
                    
                    // If level is empty, remove it
                    if level.order_count == 0 {
                        self.asks.remove(&price);
                    }
                } else {
                    // Order partially cancelled, update size
                    *order_size = new_size;
                }
            }
        }
    }
    
    /// Delete a bid order
    fn delete_bid(&mut self, order_id: u64, price: u64, size: u32) {
        if let Some(level) = self.bids.get_mut(&price) {
            if level.orders.remove(&order_id).is_some() {
                // Update the level
                level.size = level.size.saturating_sub(size);
                level.order_count = level.order_count.saturating_sub(1);
                
                // If level is empty, remove it
                if level.order_count == 0 {
                    self.bids.remove(&price);
                }
            }
        }
    }
    
    /// Delete an ask order
    fn delete_ask(&mut self, order_id: u64, price: u64, size: u32) {
        if let Some(level) = self.asks.get_mut(&price) {
            if level.orders.remove(&order_id).is_some() {
                // Update the level
                level.size = level.size.saturating_sub(size);
                level.order_count = level.order_count.saturating_sub(1);
                
                // If level is empty, remove it
                if level.order_count == 0 {
                    self.asks.remove(&price);
                }
            }
        }
    }
    
    /// Get bid levels for API return
    fn get_bid_levels(&self) -> Vec<OrderBookEntry> {
        // For bids, we want the highest prices first
        self.bids.iter().rev()
            .map(|(price, level)| OrderBookEntry {
                price: price_to_float(*price),
                size: level.size,
                order_count: level.order_count,
            })
            .collect()
    }
    
    /// Get ask levels for API return
    fn get_ask_levels(&self) -> Vec<OrderBookEntry> {
        // For asks, we want the lowest prices first
        self.asks.iter()
            .map(|(price, level)| OrderBookEntry {
                price: price_to_float(*price),
                size: level.size,
                order_count: level.order_count,
            })
            .collect()
    }
}
