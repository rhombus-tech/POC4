/*!
 * Order book reconstruction from NASDAQ ITCH messages
 * 
 * Efficiently maintains a full-depth limit order book based on
 * real-time ITCH message processing, optimized for TEE environments.
 */

use crate::error::Result;
use crate::itch::types::*;
use std::collections::{HashMap, BTreeMap};
use super::types::*;

/// Reconstructs and maintains a limit order book from ITCH messages
pub struct OrderBookReconstructor {
    /// Map of stock symbols to order books
    books: HashMap<String, StockOrderBook>,
    
    /// Map of order IDs to order details
    orders: HashMap<u64, OrderDetails>,
    
    /// Total number of messages processed
    messages_processed: u64,
    
    /// Total number of orders added
    orders_added: u64,
    
    /// Counts of messages by type
    message_type_counts: HashMap<MessageType, u64>,
    
    /// Timing metrics for each message type (in nanoseconds)
    message_type_timing: HashMap<MessageType, (u64, u64)>, // (total_time, count)
    
    /// Timestamp of last processed message
    last_message_timestamp: u64,
    
    /// Average latency between messages in microseconds
    avg_message_latency: f64,
    
    /// Number of messages used for latency calculation
    message_count_for_latency: u64,
    
    /// Maximum spread observed across all books
    max_spread: i64,
    
    /// Minimum spread observed across all books
    min_spread: i64,
    
    /// Volatility metrics by symbol (standard deviation of midpoint price changes)
    volatility_metrics: HashMap<String, (f64, u64, f64)>, // (sum_squared_changes, count, last_midpoint)
    
    /// Maps stock symbol to its latest NOII data
    noii_data: HashMap<String, NOIIData>,
    
    /// Maps stock symbol to its latest RPII data
    rpii_data: HashMap<String, RPIIData>,
    
    /// Maps stock symbol to its latest LULD Auction Collar data
    luld_data: HashMap<String, LULDData>,
    
    /// Statistics for monitoring
    pub orders_executed: u64,
    pub orders_deleted: u64,
    pub noii_messages: u64,
    pub rpii_messages: u64,
    pub luld_messages: u64,
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
        OrderBookReconstructor {
            books: HashMap::new(),
            orders: HashMap::new(),
            messages_processed: 0,
            orders_added: 0,
            // Initialize new statistics fields with default values
            message_type_counts: HashMap::new(),
            message_type_timing: HashMap::new(),
            last_message_timestamp: 0,
            avg_message_latency: 0.0,
            message_count_for_latency: 0,
            max_spread: 0,
            min_spread: i64::MAX,
            volatility_metrics: HashMap::new(),
            noii_data: HashMap::new(),
            rpii_data: HashMap::new(),
            luld_data: HashMap::new(),
            orders_executed: 0,
            orders_deleted: 0,
            noii_messages: 0,
            rpii_messages: 0,
            luld_messages: 0,
        }
    }
    
    /// Process a message and update order books accordingly
    pub fn process_message(&mut self, message: &ITCHMessage) -> Result<()> {
        // Record start time for performance measurement
        let start_time = std::time::Instant::now();
        
        // Update message count
        self.messages_processed += 1;
        
        // Update message type counter
        *self.message_type_counts.entry(message.message_type).or_insert(0) += 1;
        
        // Calculate message latency if we have a previous message
        if self.last_message_timestamp > 0 && message.timestamp > self.last_message_timestamp {
            let latency = (message.timestamp - self.last_message_timestamp) as f64;
            let prev_total = self.avg_message_latency * self.message_count_for_latency as f64;
            self.message_count_for_latency += 1;
            self.avg_message_latency = (prev_total + latency) / self.message_count_for_latency as f64;
        }
        self.last_message_timestamp = message.timestamp;
        
        // Process message based on type
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
            MessagePayload::NOII(noii) => {
                // Convert imbalance direction
                let imbalance_direction = match noii.imbalance_direction {
                    b'B' => ImbalanceDirection::Buy,
                    b'S' => ImbalanceDirection::Sell,
                    b'N' => ImbalanceDirection::NoImbalance,
                    b'O' => ImbalanceDirection::InsufficientOrders,
                    _ => ImbalanceDirection::NoImbalance,
                };
                
                // Convert cross type
                let cross_type = match noii.cross_type {
                    b'O' => CrossType::Opening,
                    b'C' => CrossType::Closing,
                    b'I' => CrossType::IPO,
                    b'H' => CrossType::Halt,
                    b'V' => CrossType::Volatility,
                    _ => CrossType::Other,
                };
                
                // Create NOII data entry
                let noii_data = NOIIData {
                    timestamp: message.timestamp,
                    paired_shares: noii.paired_shares as u64,
                    imbalance_shares: noii.imbalance_shares as u64,
                    imbalance_direction,
                    far_price: noii.far_price as u64,
                    near_price: noii.near_price as u64,
                    current_reference_price: noii.current_reference_price as u64,
                    cross_type,
                    price_variation_indicator: noii.price_variation_indicator as char,
                };
                
                // Store NOII data for this symbol
                self.noii_data.insert(noii.stock.clone(), noii_data);
                self.noii_messages += 1;
            },
            MessagePayload::RPII(rpii) => {
                // Convert interest flag
                let interest_flag = match rpii.interest_flag {
                    b'B' => InterestFlag::Buy,
                    b'S' => InterestFlag::Sell,
                    _ => InterestFlag::None,
                };
                
                // Create RPII data entry
                let rpii_data = RPIIData {
                    timestamp: message.timestamp,
                    interest_flag,
                };
                
                // Store RPII data for this symbol
                self.rpii_data.insert(rpii.stock.clone(), rpii_data);
                self.rpii_messages += 1;
            },
            MessagePayload::LULDAuctionCollar(luld) => {
                // Create LULD data entry
                let luld_data = LULDData {
                    timestamp: message.timestamp,
                    auction_collar_reference_price: luld.auction_collar_reference_price as u64,
                    upper_auction_collar_price: luld.upper_auction_collar_price as u64,
                    lower_auction_collar_price: luld.lower_auction_collar_price as u64,
                    auction_collar_extension: luld.auction_collar_extension,
                };
                
                // Store LULD data for this symbol
                self.luld_data.insert(luld.stock.clone(), luld_data);
                self.luld_messages += 1;
            },
            // Other message types don't affect the order book
            _ => {},
        }
        
        // Update market quality metrics after processing the message
        self.update_market_quality_metrics();
        
        // Record end time for performance measurement
        let end_time = std::time::Instant::now();
        let elapsed_time = end_time.duration_since(start_time).as_nanos() as u64;
        
        // Update timing metrics for this message type
        let message_type = message.message_type;
        let (total_time, count) = self.message_type_timing.entry(message_type).or_insert((0, 0));
        *total_time += elapsed_time;
        *count += 1;
        
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
    
    /// Get NOII data for a symbol
    pub fn get_noii_data(&self, symbol: &str) -> Option<&NOIIData> {
        self.noii_data.get(symbol)
    }
    
    /// Get RPII data for a symbol
    pub fn get_rpii_data(&self, symbol: &str) -> Option<&RPIIData> {
        self.rpii_data.get(symbol)
    }
    
    /// Get LULD Auction Collar data for a symbol
    pub fn get_luld_data(&self, symbol: &str) -> Option<&LULDData> {
        self.luld_data.get(symbol)
    }
    
    /// Update market quality metrics (spread, volatility) for all books
    fn update_market_quality_metrics(&mut self) {
        for (symbol, book) in &self.books {
            // Update spread metrics
            if let (Some(best_bid), Some(best_ask)) = (book.get_best_bid_price(), book.get_best_ask_price()) {
                let spread = best_ask as i64 - best_bid as i64;
                if spread > self.max_spread {
                    self.max_spread = spread;
                }
                if spread < self.min_spread {
                    self.min_spread = spread;
                }
                
                // Update volatility metrics using midpoint price
                let midpoint = (best_bid as f64 + best_ask as f64) / 2.0;
                let entry = self.volatility_metrics.entry(symbol.clone()).or_insert((0.0, 0, midpoint));
                
                // If we have a previous midpoint, calculate price change
                if entry.1 > 0 {
                    let price_change = midpoint - entry.2;
                    entry.0 += price_change * price_change; // Sum of squared changes
                }
                
                // Update the entry
                entry.1 += 1;
                entry.2 = midpoint;
            }
        }
    }
    
    /// Get statistics about the reconstructed order books
    pub fn get_statistics(&self) -> HashMap<String, u64> {
        let mut stats = HashMap::new();
        
        // General statistics
        stats.insert("messages_processed".to_string(), self.messages_processed);
        stats.insert("orders_added".to_string(), self.orders_added);
        stats.insert("active_orders".to_string(), self.orders.len() as u64);
        stats.insert("active_books".to_string(), self.books.len() as u64);
        
        // Message type statistics - NOII, RPII, LULD counts from type_counts map
        let noii_count = self.message_type_counts.get(&MessageType::NOII).copied().unwrap_or(0);
        stats.insert("noii_messages".to_string(), noii_count);
        stats.insert("noii_symbols".to_string(), self.noii_data.len() as u64);
        
        let rpii_count = self.message_type_counts.get(&MessageType::RPII).copied().unwrap_or(0);
        stats.insert("rpii_messages".to_string(), rpii_count);
        stats.insert("rpii_symbols".to_string(), self.rpii_data.len() as u64);
        
        let luld_count = self.message_type_counts.get(&MessageType::LULDAuctionCollar).copied().unwrap_or(0);
        stats.insert("luld_messages".to_string(), luld_count);
        stats.insert("luld_symbols".to_string(), self.luld_data.len() as u64);
        
        // Calculate average processing time for all messages
        let mut total_processing_time = 0;
        let mut total_messages = 0;
        for (_, (time, count)) in &self.message_type_timing {
            total_processing_time += time;
            total_messages += count;
        }
        if total_messages > 0 {
            stats.insert("avg_processing_time_ns".to_string(), total_processing_time / total_messages);
        }
        
        // Market quality metrics - convert to integers for compatibility
        stats.insert("max_spread".to_string(), self.max_spread as u64);
        if self.min_spread != i64::MAX {
            stats.insert("min_spread".to_string(), self.min_spread as u64);
        }
        
        // Order book statistics
        let mut total_bid_depth = 0;
        let mut total_ask_depth = 0;
        let mut max_bid_depth = 0;
        let mut max_ask_depth = 0;
        
        for book in self.books.values() {
            let bid_depth = book.bids.len() as u64;
            let ask_depth = book.asks.len() as u64;
            
            total_bid_depth += bid_depth;
            total_ask_depth += ask_depth;
            
            if bid_depth > max_bid_depth {
                max_bid_depth = bid_depth;
            }
            
            if ask_depth > max_ask_depth {
                max_ask_depth = ask_depth;
            }
        }
        
        let book_count = self.books.len() as u64;
        let avg_bid_depth = if book_count > 0 { total_bid_depth / book_count } else { 0 };
        let avg_ask_depth = if book_count > 0 { total_ask_depth / book_count } else { 0 };
        
        stats.insert("total_bid_depth".to_string(), total_bid_depth);
        stats.insert("total_ask_depth".to_string(), total_ask_depth);
        stats.insert("max_bid_depth".to_string(), max_bid_depth);
        stats.insert("max_ask_depth".to_string(), max_ask_depth);
        stats.insert("avg_bid_depth".to_string(), avg_bid_depth);
        stats.insert("avg_ask_depth".to_string(), avg_ask_depth);
        stats.insert("orders_executed".to_string(), self.orders_executed);
        stats.insert("orders_deleted".to_string(), self.orders_deleted);
        
        stats
    }
    
    /// Get all active symbols
    pub fn get_symbols(&self) -> Vec<String> {
        self.books.keys().cloned().collect()
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
    
    /// Get all order books
    pub fn get_all_books(&self) -> &HashMap<String, StockOrderBook> {
        &self.books
    }
    
    pub fn reset_statistics(&mut self) {
        self.messages_processed = 0;
        self.orders_added = 0;
        self.orders_executed = 0;
        self.orders_deleted = 0;
        self.noii_messages = 0;
        self.rpii_messages = 0;
        self.luld_messages = 0;
    }
}

/// NOII data for tracking imbalance information
#[derive(Clone, Debug)]
pub struct NOIIData {
    pub timestamp: u64,
    pub paired_shares: u64,
    pub imbalance_shares: u64,
    pub imbalance_direction: ImbalanceDirection,
    pub far_price: u64,
    pub near_price: u64,
    pub current_reference_price: u64,
    pub cross_type: CrossType,
    pub price_variation_indicator: char,
}

/// RPII data for tracking retail price improvement indicator
#[derive(Clone, Debug)]
pub struct RPIIData {
    pub timestamp: u64,
    pub interest_flag: InterestFlag,
}

/// LULD Auction Collar data
#[derive(Clone, Debug)]
pub struct LULDData {
    pub timestamp: u64,
    pub auction_collar_reference_price: u64,
    pub upper_auction_collar_price: u64,
    pub lower_auction_collar_price: u64,
    pub auction_collar_extension: u32,
}

/// Direction of order imbalance
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImbalanceDirection {
    Buy,
    Sell,
    NoImbalance,
    InsufficientOrders,
}

/// Cross type for NOII
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossType {
    Opening,
    Closing,
    IPO,
    Halt,
    Volatility,
    Other,
}

/// Interest flag for RPII
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterestFlag {
    Buy,
    Sell,
    None,
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
    
    /// Get the best (highest) bid price
    fn get_best_bid_price(&self) -> Option<u64> {
        // BTreeMap is sorted, so the last key is the highest price
        self.bids.keys().next_back().copied()
    }
    
    /// Get the best (lowest) ask price
    fn get_best_ask_price(&self) -> Option<u64> {
        // BTreeMap is sorted, so the first key is the lowest price
        self.asks.keys().next().copied()
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
