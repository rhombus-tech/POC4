/*!
 * Real-time NASDAQ ITCH Feed Handler
 * 
 * Optimized for high-performance, low-latency handling of live market data feeds.
 * Implements zero-copy processing, configurable batching, and message prioritization.
 */

use crate::aot::{AotCompiler, AotConfig, MarketProfile};
use crate::error::{ClientError, Result};
use crate::itch::parser::ITCHParser;
use crate::itch::types::{ITCHMessage, MessageType};
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Reference-based ITCH message that doesn't own its data
#[derive(Debug)]
pub struct ITCHMessageRef<'a> {
    /// Message type
    pub message_type: MessageType,
    /// Timestamp in nanoseconds from midnight
    pub timestamp: u64,
    /// Reference to the original buffer containing the full message
    pub data_slice: &'a [u8],
}

/// Zero-copy extension to the ITCHParser
impl ITCHParser {
    /// Parse a message without copying the underlying data
    pub fn parse_message_zero_copy<'a>(&self, data: &'a [u8]) -> Result<ITCHMessageRef<'a>> {
        if data.is_empty() {
            return Err(ClientError::Other("Empty message data".to_string()));
        }
        
        // Extract message type from first byte
        let message_type = MessageType::from(data[0]);
        
        // Extract timestamp (bytes 1-8)
        let timestamp = if data.len() >= 9 {
            let mut timestamp_bytes = [0u8; 8];
            timestamp_bytes.copy_from_slice(&data[1..9]);
            u64::from_be_bytes(timestamp_bytes)
        } else {
            return Err(ClientError::Other("Message too short for timestamp".to_string()));
        };
        
        // Create a reference-based message pointing to the original buffer
        Ok(ITCHMessageRef {
            message_type,
            timestamp,
            data_slice: data,
        })
    }
}

/// Methods to extract fields on-demand without copying
impl<'a> ITCHMessageRef<'a> {
    /// Extract stock symbol without allocating new strings
    pub fn get_stock(&self) -> Option<&str> {
        match self.message_type {
            MessageType::AddOrder => {
                // In ITCH 5.0 Add Order messages, stock symbol is at offset 13 (8-byte length)
                if self.data_slice.len() < 21 {
                    return None;
                }
                
                let stock_bytes = &self.data_slice[13..21];
                // Convert to str and trim trailing spaces without allocation
                core::str::from_utf8(stock_bytes).ok().map(|s| s.trim_end_matches(' '))
            },
            MessageType::Trade => {
                // In ITCH 5.0 Trade messages, stock symbol is also at offset 13
                if self.data_slice.len() < 21 {
                    return None;
                }
                
                let stock_bytes = &self.data_slice[13..21];
                core::str::from_utf8(stock_bytes).ok().map(|s| s.trim_end_matches(' '))
            },
            // Handle other message types as needed
            _ => None,
        }
    }
    
    /// Get price from message without allocation
    pub fn get_price(&self) -> Option<u64> {
        match self.message_type {
            MessageType::AddOrder => {
                // In Add Order messages, price is at offset 21 (8 bytes)
                if self.data_slice.len() < 29 {
                    return None;
                }
                
                let mut price_bytes = [0u8; 8];
                price_bytes.copy_from_slice(&self.data_slice[21..29]);
                Some(u64::from_be_bytes(price_bytes))
            },
            // Handle other message types as needed
            _ => None,
        }
    }
    
    /// Convert to owned ITCHMessage when necessary
    pub fn to_owned(&self) -> Option<ITCHMessage> {
        let mut parser = ITCHParser::new();
        parser.parse_message(self.data_slice).ok()
    }
}

/// Priority levels for ITCH messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessagePriority {
    /// Critical market events (system events, trading halts)
    Critical = 0,
    /// High priority events (trades, executions)
    High = 1,
    /// Medium priority events (order additions, modifications)
    Medium = 2,
    /// Low priority events (administrative messages)
    Low = 3,
}

impl<'a> ITCHMessageRef<'a> {
    /// Determine message priority based on message type
    pub fn get_priority(&self) -> MessagePriority {
        match self.message_type {
            MessageType::SystemEvent => MessagePriority::Critical,
            MessageType::TradingAction => MessagePriority::Critical,
            MessageType::Trade | MessageType::OrderExecuted => MessagePriority::High,
            MessageType::AddOrder | MessageType::OrderReplace => MessagePriority::Medium,
            _ => MessagePriority::Low,
        }
    }
}

/// Pre-allocated memory pool for zero-allocation processing
pub struct MessageMemoryPool {
    /// Pre-allocated buffers for message processing
    buffers: Vec<Vec<u8>>,
    /// Indices of available buffers
    available: VecDeque<usize>,
    /// Size of each buffer
    buffer_size: usize,
    /// Statistics
    stats: MemoryPoolStats,
}

/// Statistics for memory pool monitoring
#[derive(Debug, Default, Clone)]
pub struct MemoryPoolStats {
    /// Total number of buffer acquisitions
    pub total_acquisitions: usize,
    /// Number of failed acquisitions (pool exhausted)
    pub failed_acquisitions: usize,
    /// Maximum number of buffers in use simultaneously
    pub max_concurrent_use: usize,
}

impl MessageMemoryPool {
    /// Create a new memory pool with pre-allocated buffers
    pub fn new(buffer_size: usize, pool_size: usize) -> Self {
        let mut buffers = Vec::with_capacity(pool_size);
        let mut available = VecDeque::with_capacity(pool_size);
        
        for i in 0..pool_size {
            buffers.push(vec![0u8; buffer_size]);
            available.push_back(i);
        }
        
        Self { 
            buffers, 
            available, 
            buffer_size,
            stats: MemoryPoolStats::default(),
        }
    }
    
    /// Acquire a buffer from the pool
    pub fn acquire(&mut self) -> Option<&mut [u8]> {
        self.stats.total_acquisitions += 1;
        
        let idx = match self.available.pop_front() {
            Some(idx) => idx,
            None => {
                self.stats.failed_acquisitions += 1;
                return None;
            }
        };
        
        let current_use = self.buffers.len() - self.available.len();
        if current_use > self.stats.max_concurrent_use {
            self.stats.max_concurrent_use = current_use;
        }
        
        Some(&mut self.buffers[idx][..])
    }
    
    /// Release a buffer back to the pool
    pub fn release(&mut self, buffer: &[u8]) {
        // Find buffer index from pointer and mark as available
        let buffer_ptr = buffer.as_ptr() as usize;
        for (idx, buf) in self.buffers.iter().enumerate() {
            if buf.as_ptr() as usize == buffer_ptr {
                self.available.push_back(idx);
                break;
            }
        }
    }
    
    /// Get current statistics
    pub fn get_stats(&self) -> MemoryPoolStats {
        self.stats.clone()
    }
    
    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = MemoryPoolStats::default();
    }
}

/// Configuration for message batch processing
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of messages to process in a single batch
    pub max_batch_size: usize,
    /// Maximum time to wait before processing incomplete batch (microseconds)
    pub max_batch_delay_us: u64,
    /// Whether to dynamically adjust batch size based on market conditions
    pub dynamic_sizing: bool,
    /// Minimum batch size when using dynamic sizing
    pub min_batch_size: usize,
    /// Maximum batch size when using dynamic sizing
    pub max_dynamic_batch_size: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 1000,
            max_batch_delay_us: 100, // 100 microseconds
            dynamic_sizing: true,
            min_batch_size: 100,
            max_dynamic_batch_size: 5000,
        }
    }
}

/// Batch processor for efficient message handling
pub struct BatchProcessor {
    /// Configuration for batch processing
    config: BatchConfig,
    /// Message buffer for current batch
    message_buffer: Vec<Vec<u8>>,
    /// Timestamp of the last processed batch
    last_batch_time: Instant,
    /// AOT compiler for optimized processing
    aot_compiler: AotCompiler,
    /// Parser for zero-copy message handling
    parser: ITCHParser,
    /// Statistics about processed batches
    stats: BatchProcessorStats,
}

/// Statistics for batch processor monitoring
#[derive(Debug, Default, Clone)]
pub struct BatchProcessorStats {
    /// Total number of batches processed
    pub total_batches: usize,
    /// Total number of messages processed
    pub total_messages: usize,
    /// Average batch size
    pub avg_batch_size: f64,
    /// Maximum batch size encountered
    pub max_batch_size: usize,
    /// Average processing time per batch (microseconds)
    pub avg_processing_time_us: f64,
    /// Total number of dynamic batch size adjustments
    pub batch_size_adjustments: usize,
}

impl BatchProcessor {
    /// Create a new batch processor with the given configuration
    pub fn new(config: BatchConfig) -> Self {
        Self {
            config,
            message_buffer: Vec::with_capacity(5000), // Pre-allocate for efficiency
            last_batch_time: Instant::now(),
            aot_compiler: AotCompiler::new(AotConfig {
                parameter_format_specialization: true,
                message_type_specialization: true,
                pgo_enabled: true,
                profile_data_path: None,
                opt_level: 3,
                max_specialized_handlers: 50,
            }),
            parser: ITCHParser::new(),
            stats: BatchProcessorStats::default(),
        }
    }
    
    /// Process an incoming packet of ITCH data
    pub fn process_incoming_packet(&mut self, packet: &[u8]) -> Result<()> {
        // Extract messages from packet and add to batch
        let messages = self.extract_messages(packet)?;
        self.message_buffer.extend(messages);
        
        // Process batch if it's full or too old
        if self.should_process_batch() {
            self.process_batch()?;
        }
        
        Ok(())
    }
    
    /// Extract individual messages from a packet
    fn extract_messages(&self, packet: &[u8]) -> Result<Vec<Vec<u8>>> {
        let mut messages = Vec::new();
        let mut offset = 0;
        
        while offset + 2 <= packet.len() {
            // Read message length (2 bytes, big endian)
            let msg_len = ((packet[offset] as u16) << 8) | (packet[offset + 1] as u16);
            offset += 2;
            
            if offset + msg_len as usize > packet.len() {
                return Err(ClientError::Other(format!(
                    "Incomplete message: expected {} bytes, only {} available",
                    msg_len, packet.len() - offset
                )));
            }
            
            // Extract the message
            let message = packet[offset..offset + msg_len as usize].to_vec();
            messages.push(message);
            
            offset += msg_len as usize;
        }
        
        Ok(messages)
    }
    
    /// Determine if it's time to process the current batch
    fn should_process_batch(&self) -> bool {
        self.message_buffer.len() >= self.config.max_batch_size ||
        (self.message_buffer.len() > 0 && 
         self.last_batch_time.elapsed().as_micros() >= self.config.max_batch_delay_us as u128)
    }
    
    /// Process the current batch of messages
    fn process_batch(&mut self) -> Result<()> {
        if self.message_buffer.is_empty() {
            return Ok(());
        }
        
        let start_time = Instant::now();
        let batch_size = self.message_buffer.len();
        
        // Use AOT compiler to optimize batch processing
        self.aot_compiler.set_market_profile(MarketProfile::Normal); // TODO: Detect market profile dynamically
        
        // Process each message through our zero-copy parser and optimized processor
        for message_data in &self.message_buffer {
            // Use pre-compiled AOT handlers for this message type
            let _ = self.aot_compiler.process_message(message_data)?;
        }
        
        // Update statistics
        let processing_time = start_time.elapsed();
        self.update_stats(batch_size, processing_time);
        
        // Clear buffer and update time
        self.message_buffer.clear();
        self.last_batch_time = Instant::now();
        
        // Adjust batch size if dynamic sizing is enabled
        if self.config.dynamic_sizing {
            self.adjust_batch_size_for_current_load(processing_time);
        }
        
        Ok(())
    }
    
    /// Update processor statistics
    fn update_stats(&mut self, batch_size: usize, processing_time: Duration) {
        self.stats.total_batches += 1;
        self.stats.total_messages += batch_size;
        
        if batch_size > self.stats.max_batch_size {
            self.stats.max_batch_size = batch_size;
        }
        
        // Update averages
        let processing_time_us = processing_time.as_micros() as f64;
        self.stats.avg_processing_time_us = (
            (self.stats.avg_processing_time_us * (self.stats.total_batches - 1) as f64) +
            processing_time_us
        ) / self.stats.total_batches as f64;
        
        self.stats.avg_batch_size = (
            (self.stats.avg_batch_size * (self.stats.total_batches - 1) as f64) +
            batch_size as f64
        ) / self.stats.total_batches as f64;
    }
    
    /// Dynamically adjust batch size based on processing performance
    fn adjust_batch_size_for_current_load(&mut self, processing_time: Duration) {
        // Target processing time is 50% of max_batch_delay
        let target_time_us = (self.config.max_batch_delay_us as f64) * 0.5;
        let actual_time_us = processing_time.as_micros() as f64;
        
        // Don't adjust for very small batches
        if self.message_buffer.capacity() < self.config.min_batch_size {
            return;
        }
        
        // Calculate adjustment factor
        let adjustment_factor = target_time_us / actual_time_us;
        
        if adjustment_factor > 1.2 || adjustment_factor < 0.8 {
            // Significant difference, adjust batch size
            let new_capacity = (
                self.message_buffer.capacity() as f64 * adjustment_factor
            ).round() as usize;
            
            // Apply bounds
            let bounded_capacity = new_capacity
                .max(self.config.min_batch_size)
                .min(self.config.max_dynamic_batch_size);
            
            // Only update if there's a significant change
            if (bounded_capacity as f64 / self.message_buffer.capacity() as f64 - 1.0).abs() > 0.1 {
                self.message_buffer.reserve(bounded_capacity - self.message_buffer.capacity());
                self.stats.batch_size_adjustments += 1;
            }
        }
    }
    
    /// Get current processor statistics
    pub fn get_stats(&self) -> BatchProcessorStats {
        self.stats.clone()
    }
    
    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = BatchProcessorStats::default();
    }
}

/// Priority-based batch processor for handling messages based on importance
pub struct PriorityBatchProcessor {
    /// Separate queues for different priority levels
    critical_queue: VecDeque<Vec<u8>>,
    high_queue: VecDeque<Vec<u8>>,
    medium_queue: VecDeque<Vec<u8>>,
    low_queue: VecDeque<Vec<u8>>,
    /// Parser for determining message priority
    parser: ITCHParser,
    /// AOT compiler for optimized processing
    aot_compiler: AotCompiler,
    /// Statistics
    stats: PriorityProcessorStats,
}

/// Statistics for priority-based processor
#[derive(Debug, Default, Clone)]
pub struct PriorityProcessorStats {
    /// Messages processed by priority level
    pub critical_messages: usize,
    pub high_messages: usize,
    pub medium_messages: usize,
    pub low_messages: usize,
    /// Current queue lengths
    pub critical_queue_len: usize,
    pub high_queue_len: usize,
    pub medium_queue_len: usize,
    pub low_queue_len: usize,
}

impl PriorityBatchProcessor {
    /// Create a new priority-based batch processor
    pub fn new() -> Self {
        Self {
            critical_queue: VecDeque::new(),
            high_queue: VecDeque::new(),
            medium_queue: VecDeque::new(),
            low_queue: VecDeque::new(),
            parser: ITCHParser::new(),
            aot_compiler: AotCompiler::new(AotConfig {
                parameter_format_specialization: true,
                message_type_specialization: true,
                pgo_enabled: true,
                profile_data_path: None,
                opt_level: 3,
                max_specialized_handlers: 50,
            }),
            stats: PriorityProcessorStats::default(),
        }
    }
    
    /// Enqueue a message based on its priority
    pub fn enqueue_message(&mut self, message: Vec<u8>) -> Result<()> {
        // Parse enough of the message to determine its type
        if message.is_empty() {
            return Err(ClientError::Other("Empty message".to_string()));
        }
        
        let message_ref = self.parser.parse_message_zero_copy(&message)?;
        let priority = message_ref.get_priority();
        
        // Place in appropriate queue based on priority
        match priority {
            MessagePriority::Critical => {
                self.critical_queue.push_back(message);
                self.stats.critical_messages += 1;
            },
            MessagePriority::High => {
                self.high_queue.push_back(message);
                self.stats.high_messages += 1;
            },
            MessagePriority::Medium => {
                self.medium_queue.push_back(message);
                self.stats.medium_messages += 1;
            },
            MessagePriority::Low => {
                self.low_queue.push_back(message);
                self.stats.low_messages += 1;
            },
        }
        
        // Update queue length stats
        self.update_queue_stats();
        
        Ok(())
    }
    
    /// Update queue length statistics
    fn update_queue_stats(&mut self) {
        self.stats.critical_queue_len = self.critical_queue.len();
        self.stats.high_queue_len = self.high_queue.len();
        self.stats.medium_queue_len = self.medium_queue.len();
        self.stats.low_queue_len = self.low_queue.len();
    }
    
    /// Process the next batch of messages based on priority
    pub fn process_next_batch(&mut self, max_batch_size: usize) -> Result<usize> {
        let mut processed = 0;
        let mut remaining = max_batch_size;
        
        // Process critical messages first, then high, medium, low
        // Use a separate method to avoid multiple mutable borrows of self
        let critical_processed = self.process_critical_queue(remaining)?;
        processed += critical_processed;
        remaining -= critical_processed;
        
        if remaining > 0 {
            let high_processed = self.process_high_queue(remaining)?;
            processed += high_processed;
            remaining -= high_processed;
        }
        
        if remaining > 0 {
            let medium_processed = self.process_medium_queue(remaining)?;
            processed += medium_processed;
            remaining -= medium_processed;
        }
        
        if remaining > 0 {
            let low_processed = self.process_low_queue(remaining)?;
            processed += low_processed;
        }
        
        // Update queue stats after processing
        self.update_queue_stats();
        
        Ok(processed)
    }
    
    /// Process messages from the critical queue
    fn process_critical_queue(&mut self, max_count: usize) -> Result<usize> {
        let mut queue = std::mem::take(&mut self.critical_queue);
        let processed = self.process_queue_internal(&mut queue, max_count)?;
        self.critical_queue = queue;
        Ok(processed)
    }
    
    /// Process messages from the high priority queue
    fn process_high_queue(&mut self, max_count: usize) -> Result<usize> {
        let mut queue = std::mem::take(&mut self.high_queue);
        let processed = self.process_queue_internal(&mut queue, max_count)?;
        self.high_queue = queue;
        Ok(processed)
    }
    
    /// Process messages from the medium priority queue
    fn process_medium_queue(&mut self, max_count: usize) -> Result<usize> {
        let mut queue = std::mem::take(&mut self.medium_queue);
        let processed = self.process_queue_internal(&mut queue, max_count)?;
        self.medium_queue = queue;
        Ok(processed)
    }
    
    /// Process messages from the low priority queue
    fn process_low_queue(&mut self, max_count: usize) -> Result<usize> {
        let mut queue = std::mem::take(&mut self.low_queue);
        let processed = self.process_queue_internal(&mut queue, max_count)?;
        self.low_queue = queue;
        Ok(processed)
    }
    
    /// Process messages from a specific queue
    fn process_queue_internal(&mut self, queue: &mut VecDeque<Vec<u8>>, max_count: usize) -> Result<usize> {
        let mut processed = 0;
        
        while processed < max_count && !queue.is_empty() {
            if let Some(message) = queue.pop_front() {
                // Process with AOT-optimized handler
                self.aot_compiler.process_message(&message)?;
                processed += 1;
            }
        }
        
        Ok(processed)
    }
    
    /// Get current statistics
    pub fn get_stats(&self) -> PriorityProcessorStats {
        self.stats.clone()
    }
    
    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = PriorityProcessorStats::default();
    }
}
