/*!
 * NASDAQ ITCH Real-Time Feed Handler Demo
 * 
 * This demo showcases the high-performance real-time ITCH feed handling capabilities
 * with zero-copy processing, batch optimization, and priority-based message handling.
 */

use aristo_client::aot::{AotCompiler, AotConfig};
use aristo_client::error::Result;
use aristo_client::itch::parser::ITCHParser;
use aristo_client::itch::realtime::{
    BatchConfig, BatchProcessor, MessageMemoryPool, MessagePriority, PriorityBatchProcessor
};
use aristo_client::itch::types::{ITCHMessage, MessageType};
use aristo_client::protocol::types::ParameterFormat;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

fn main() -> Result<()> {
    println!("NASDAQ ITCH Real-Time Feed Handler Demo");
    println!("=======================================");
    
    // STEP 1: Load sample data from file or generate synthetic data
    println!("\nStep 1: Loading sample ITCH data");
    let sample_data = load_sample_data("data/itch/samples/sample.itch")?;
    println!("  Loaded {} bytes of sample data", sample_data.len());
    
    // STEP 2: Initialize memory pool for zero-allocation processing
    println!("\nStep 2: Setting up zero-allocation memory pool");
    let memory_pool = MessageMemoryPool::new(1024, 5000); // 5000 buffers of 1KB each
    println!("  Memory pool initialized with {} KB capacity", 
             memory_pool.get_stats().max_concurrent_use);
    
    // STEP 3: Setup batch processor
    println!("\nStep 3: Configuring batch processor");
    let batch_config = BatchConfig {
        max_batch_size: 1000,
        max_batch_delay_us: 100, // 100 microseconds
        dynamic_sizing: true,
        min_batch_size: 100,
        max_dynamic_batch_size: 5000,
    };
    let max_batch_size = batch_config.max_batch_size; // Save before moving
    let mut batch_processor = BatchProcessor::new(batch_config);
    println!("  Batch processor configured with {} max batch size", 
             max_batch_size);
    
    // STEP 4: Setup priority-based processor
    println!("\nStep 4: Configuring priority-based processor");
    let mut priority_processor = PriorityBatchProcessor::new();
    println!("  Priority processor initialized");
    
    // STEP 5: Configure AOT compiler for direct processing
    println!("\nStep 5: Configuring AOT compiler for direct processing");
    let mut aot_compiler = AotCompiler::new(AotConfig {
        parameter_format_specialization: true,
        message_type_specialization: true,
        pgo_enabled: true,
        profile_data_path: None,
        opt_level: 3,
        max_specialized_handlers: 50,
    });
    println!("  AOT compiler configured with parameter format specialization");
    
    // STEP 6: Simulate packet stream from real-time feed
    println!("\nStep 6: Simulating real-time feed (standard vs. optimized batch processing)");
    simulate_realtime_feed(&sample_data, &mut batch_processor)?;
    
    // STEP 7: Demonstrate parameter format handling (as in our WebAssembly contracts)
    println!("\nStep 7: Demonstrating parameter format handling");
    demonstrate_parameter_formats(&mut aot_compiler)?;
    
    // STEP 8: Demonstrate priority-based processing
    println!("\nStep 8: Demonstrating priority-based processing");
    simulate_priority_processing(&sample_data, &mut priority_processor)?;
    
    // STEP 9: Demonstrate zero-copy parsing
    println!("\nStep 9: Demonstrating zero-copy vs. standard parsing");
    benchmark_zero_copy_parsing(&sample_data)?;
    
    println!("\nDemo complete. Real-time feed handler optimizations demonstrated successfully.");
    println!("These optimizations enable processing high-volume market data with minimal latency.");
    
    Ok(())
}

/// Load sample ITCH data from file or generate synthetic data
fn load_sample_data(file_path: &str) -> Result<Vec<u8>> {
    let path = Path::new(file_path);
    
    if path.exists() {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;
        Ok(data)
    } else {
        // Generate synthetic data
        println!("  Sample file not found, generating synthetic data");
        let data = generate_synthetic_feed_data(100_000); // 100KB of synthetic data
        Ok(data)
    }
}

/// Generate synthetic ITCH feed data for demo
fn generate_synthetic_feed_data(size_bytes: usize) -> Vec<u8> {
    let mut data = Vec::with_capacity(size_bytes);
    let mut offset = 0;
    
    let itch_parser = ITCHParser::new();
    
    while offset < size_bytes {
        // Generate message with alternating types
        // Only use message types that we have proper constructors for
        let msg_type = match (offset / 100) % 3 {
            0 => MessageType::AddOrder,
            1 => MessageType::OrderExecuted,
            _ => MessageType::OrderCancel,
        };
        
        // Create synthetic message
        let msg_data = generate_message_data(msg_type, offset as u64);
        
        // Ensure message is at least 1 byte and no larger than packet size
        if !msg_data.is_empty() && msg_data.len() < 1400 {
            // Add message length (2 bytes, big endian)
            let msg_len = msg_data.len() as u16;
            data.push((msg_len >> 8) as u8);
            data.push((msg_len & 0xFF) as u8);
            
            // Add message data only if valid
            data.extend_from_slice(&msg_data);
        }
        
        offset += 2 + msg_data.len();
    }
    
    data
}

/// Generate synthetic message data for specific type
fn generate_message_data(msg_type: MessageType, sequence: u64) -> Vec<u8> {
    let mut data = Vec::new();
    
    // Add message type byte
    data.push(msg_type as u8 as u8);
    
    // Add timestamp (8 bytes, big endian)
    let timestamp = sequence * 1_000_000; // microsecond increments
    data.extend_from_slice(&timestamp.to_be_bytes());
    
    // Add message-specific fields
    match msg_type {
        MessageType::AddOrder => {
            // Order reference number (8 bytes)
            data.extend_from_slice(&sequence.to_be_bytes());
            
            // Buy/Sell indicator (1 byte)
            data.push(if sequence % 2 == 0 { b'B' } else { b'S' });
            
            // Shares (4 bytes)
            let shares = (sequence % 1000 + 100) as u32;
            data.extend_from_slice(&shares.to_be_bytes());
            
            // Stock (8 bytes, fixed length)
            let stock = format!("SYM{:<5}", sequence % 100);
            data.extend_from_slice(stock.as_bytes());
            
            // Price (8 bytes)
            let price = (sequence % 10000 + 10000) as u64;
            data.extend_from_slice(&price.to_be_bytes());
        },
        MessageType::OrderExecuted => {
            // Order reference number (8 bytes)
            data.extend_from_slice(&sequence.to_be_bytes());
            
            // Executed shares (4 bytes)
            let shares = (sequence % 100 + 10) as u32;
            data.extend_from_slice(&shares.to_be_bytes());
            
            // Match number (8 bytes)
            data.extend_from_slice(&(sequence + 1000).to_be_bytes());
        },
        MessageType::OrderCancel => {
            // Order reference number (8 bytes)
            data.extend_from_slice(&sequence.to_be_bytes());
            
            // Canceled shares (4 bytes)
            let shares = (sequence % 50 + 5) as u32;
            data.extend_from_slice(&shares.to_be_bytes());
        },
        MessageType::SystemEvent => {
            // Event code (1 byte)
            data.push(b'O'); // Start of messages
        },
        _ => {
            // For other types, just add some padding
            // Ensure we don't have empty data (at least 16 bytes of padding)
            for i in 0..16 {
                data.push(((i as u64 + sequence) % 255) as u8);
            }
        }
    }
    
    // Ensure the message is not empty and has at least the minimum required fields
    if data.len() < 9 { // Message type (1) + timestamp (8)
        // Add padding to ensure minimum length
        while data.len() < 9 {
            data.push(0);
        }
    }
    
    data
}

/// Simulate processing a real-time feed by breaking data into packets
fn simulate_realtime_feed(data: &[u8], processor: &mut BatchProcessor) -> Result<()> {
    const PACKET_SIZE: usize = 1500; // Typical Ethernet MTU
    const NUM_PACKETS: usize = 100;  // Process 100 packets for demo
    
    // Split data into packets
    let mut packets = Vec::new();
    let mut offset = 0;
    
    // Create packets from the data
    while offset < data.len() && packets.len() < NUM_PACKETS {
        let packet_end = (offset + PACKET_SIZE).min(data.len());
        // Make sure we have enough data to be a valid packet (at least a few bytes)
        if packet_end - offset > 10 {
            packets.push(&data[offset..packet_end]);
        }
        offset = packet_end;
    }
    
    println!("  Created {} packets from sample data", packets.len());
    
    // Simulate standard processing without batching
    let start_time = Instant::now();
    let mut message_count = 0;
    let mut parser = ITCHParser::new();
    
    for packet in &packets {
        let mut packet_offset = 0;
        
        while packet_offset + 2 <= packet.len() {
            // Extract message length
            let msg_len = ((packet[packet_offset] as u16) << 8) | (packet[packet_offset + 1] as u16);
            packet_offset += 2;
            
            // Skip invalid messages
            if msg_len == 0 || packet_offset + msg_len as usize > packet.len() {
                break;
            }
            
            // Process individual message
            let message_data = &packet[packet_offset..packet_offset + msg_len as usize];
            if !message_data.is_empty() {
                // Only try to parse non-empty messages
                match parser.parse_message(message_data) {
                    Ok(_) => message_count += 1,
                    Err(_) => {} // Skip invalid messages silently
                }
            }
            
            packet_offset += msg_len as usize;
        }
    }
    
    let standard_time = start_time.elapsed();
    println!("  Standard processing: {} messages in {:?}",
             message_count, 
             standard_time);
    
    // Only calculate per-message time if we processed messages
    if message_count > 0 {
        println!("  Average time per message: {:?}",
                 standard_time / message_count as u32);
    } else {
        println!("  No valid messages processed in standard mode");
    }
    
    // Simulate optimized processing with batching
    let start_time = Instant::now();
    
    for packet in &packets {
        // Wrap in a match to handle any errors gracefully
        match processor.process_incoming_packet(packet) {
            Ok(_) => {},
            Err(e) => {
                println!("  Warning: Error processing packet: {}", e);
                // Continue with next packet
            }
        }
    }
    
    let optimized_time = start_time.elapsed();
    let stats = processor.get_stats();
    
    println!("  Optimized batch processing: {} messages in {:?}",
             stats.total_messages,
             optimized_time);
             
    // Only calculate per-message time if we processed messages
    if stats.total_messages > 0 {
        println!("  Average time per message: {:?}",
                 optimized_time / stats.total_messages as u32);
    } else {
        println!("  No valid messages processed in batch mode");
    }
    
    println!("  Batch statistics:");
    println!("    Total batches: {}", stats.total_batches);
    println!("    Average batch size: {:.1}", stats.avg_batch_size);
    println!("    Maximum batch size: {}", stats.max_batch_size);
    println!("    Average processing time: {:.2} μs", stats.avg_processing_time_us);
    println!("    Batch size adjustments: {}", stats.batch_size_adjustments);
    
    // Calculate performance improvement if both methods processed messages
    if message_count > 0 && stats.total_messages > 0 {
        let speedup = standard_time.as_nanos() as f64 / optimized_time.as_nanos() as f64;
        println!("  Batch processing is {:.2}x faster than standard processing", speedup);
    } else {
        println!("  Could not calculate speedup - insufficient valid messages");
    }
    
    Ok(())
}

/// Demonstrate parameter format handling for WebAssembly contract integration
fn demonstrate_parameter_formats(compiler: &mut AotCompiler) -> Result<()> {
    // Create sample market data
    let mut order_book_data = Vec::new();
    
    // Generate some dummy order book data
    for i in 0..100 {
        order_book_data.push((i % 255) as u8);
    }
    
    println!("  Testing with {} bytes of order book data", order_book_data.len());
    
    // Create a format-aware processor for testing parameter formats
    let mut processor = FormatAwareProcessor::new();
    
    // Test with length-prefixed format
    let start_time = Instant::now();
    for _ in 0..1000 {
        // Create length-prefixed format (4-byte length + data)
        let mut lp_data = Vec::with_capacity(4 + order_book_data.len());
        lp_data.extend_from_slice(&(order_book_data.len() as u32).to_le_bytes());
        lp_data.extend_from_slice(&order_book_data);
        
        // Process with parameter format hint
        let _ = processor.process_message_with_format(&lp_data, Some(ParameterFormat::LengthPrefixed))?;
    }
    let lp_time = start_time.elapsed();
    
    // Test with direct format
    let start_time = Instant::now();
    for _ in 0..1000 {
        // Process the data directly without any length prefix
        let _ = processor.process_message_with_format(&order_book_data, Some(ParameterFormat::Direct))?;
    }
    let direct_time = start_time.elapsed();
    
    // Test with auto-detection
    let start_time = Instant::now();
    for _ in 0..1000 {
        // Let the processor auto-detect the format
        let _ = processor.process_message_with_format(&order_book_data, None)?;
    }
    let auto_time = start_time.elapsed();
    
    println!("  Parameter format processing times (1000 iterations):");
    println!("    Length-prefixed format: {:?}", lp_time);
    println!("    Direct format: {:?}", direct_time);
    println!("    Auto-detection: {:?}", auto_time);
    
    // Calculate the fastest format
    if lp_time < direct_time && lp_time < auto_time {
        println!("    Length-prefixed format is fastest");
    } else if direct_time < lp_time && direct_time < auto_time {
        println!("    Direct format is fastest");
    } else {
        println!("    Auto-detection is fastest");
    }
    
    let specialized_speedup = auto_time.as_nanos() as f64 / 
                             direct_time.min(lp_time).as_nanos() as f64;
    println!("    Format specialization provides {:.2}x speedup vs. auto-detection", 
             specialized_speedup);
    
    Ok(())
}

/// Demonstrate priority-based processing for critical market events
fn simulate_priority_processing(data: &[u8], processor: &mut PriorityBatchProcessor) -> Result<()> {
    const PACKET_SIZE: usize = 1500; // Typical Ethernet MTU
    const NUM_PACKETS: usize = 50;   // Process 50 packets for demo
    
    // Extract messages from data
    let mut messages = Vec::new();
    let mut offset = 0;
    
    while offset + 2 <= data.len() && messages.len() < 1000 {
        // Extract message length
        let msg_len = ((data[offset] as u16) << 8) | (data[offset + 1] as u16);
        offset += 2;
        
        if offset + msg_len as usize > data.len() {
            break;
        }
        
        // Store message
        messages.push(data[offset..offset + msg_len as usize].to_vec());
        offset += msg_len as usize;
    }
    
    println!("  Extracted {} messages for priority processing", messages.len());
    
    // Inject some critical and high-priority messages
    // Make every 50th message a system event (critical)
    for i in (0..messages.len()).step_by(50) {
        if i < messages.len() {
            let mut system_event = Vec::new();
            system_event.push(MessageType::SystemEvent as u8 as u8);
            system_event.extend_from_slice(&(i as u64).to_be_bytes()); // timestamp
            system_event.push(b'A'); // Emergency halt event
            
            messages[i] = system_event;
        }
    }
    
    // Make every 10th message a trade (high priority)
    for i in (0..messages.len()).step_by(10) {
        if i < messages.len() && i % 50 != 0 { // Don't overwrite system events
            let mut trade_msg = Vec::new();
            trade_msg.push(MessageType::Trade as u8 as u8);
            trade_msg.extend_from_slice(&(i as u64).to_be_bytes()); // timestamp
            
            // Add some trade data
            for _ in 0..20 {
                trade_msg.push(0); // Dummy data
            }
            
            messages[i] = trade_msg;
        }
    }
    
    // Feed messages to priority processor
    let start_time = Instant::now();
    
    for message in &messages {
        processor.enqueue_message(message.clone())?;
    }
    
    // Process all messages
    let mut total_processed = 0;
    while total_processed < messages.len() {
        let processed = processor.process_next_batch(100)?;
        total_processed += processed;
        
        if processed == 0 {
            break; // No more messages to process
        }
    }
    
    let processing_time = start_time.elapsed();
    let stats = processor.get_stats();
    
    println!("  Priority processing complete in {:?}", processing_time);
    println!("  Messages processed by priority:");
    println!("    Critical: {} messages", stats.critical_messages);
    println!("    High: {} messages", stats.high_messages);
    println!("    Medium: {} messages", stats.medium_messages);
    println!("    Low: {} messages", stats.low_messages);
    
    // Calculate percentage of critical messages that were processed
    let critical_pct = (stats.critical_messages as f64 / 
                       (messages.len() / 50) as f64) * 100.0;
    println!("  {}% of critical messages were processed", critical_pct);
    
    Ok(())
}

/// Benchmark zero-copy parsing versus standard parsing
fn benchmark_zero_copy_parsing(data: &[u8]) -> Result<()> {
    let parser = ITCHParser::new();
    let iterations = 1000;
    
    // Extract a single message for testing
    let mut offset = 0;
    let mut test_message = Vec::new();
    
    if offset + 2 <= data.len() {
        let msg_len = ((data[offset] as u16) << 8) | (data[offset + 1] as u16);
        offset += 2;
        
        if offset + msg_len as usize <= data.len() {
            test_message = data[offset..offset + msg_len as usize].to_vec();
        }
    }
    
    if test_message.is_empty() {
        println!("  Could not extract test message from data");
        return Ok(());
    }
    
    println!("  Benchmarking with a {} byte test message", test_message.len());
    
    // Benchmark standard parsing (with memory allocation)
    let start_time = Instant::now();
    for _ in 0..iterations {
        let mut parser_instance = ITCHParser::new();
        let _ = parser_instance.parse_message(&test_message)?;
    }
    let standard_time = start_time.elapsed();
    
    println!("  Standard parsing: {:?} for {} iterations ({:?} per parse)",
             standard_time, 
             iterations,
             standard_time / iterations as u32);
    
    // Benchmark zero-copy parsing
    let start_time = Instant::now();
    for _ in 0..iterations {
        let message_ref = parser.parse_message_zero_copy(&test_message)?;
        
        // Access some fields to simulate real usage
        let _ = message_ref.message_type;
        let _ = message_ref.get_stock();
        let _ = message_ref.get_price();
    }
    let zero_copy_time = start_time.elapsed();
    
    println!("  Zero-copy parsing: {:?} for {} iterations ({:?} per parse)",
             zero_copy_time, 
             iterations,
             zero_copy_time / iterations as u32);
    
    // Calculate speedup
    let speedup = standard_time.as_nanos() as f64 / zero_copy_time.as_nanos() as f64;
    println!("  Zero-copy parsing is {:.2}x faster than standard parsing", speedup);
    
    Ok(())
}

/// Demo message processor that simulates different parameter formats
pub struct FormatAwareProcessor {
    parser: ITCHParser,
}

impl FormatAwareProcessor {
    /// Create a new format-aware processor
    pub fn new() -> Self {
        Self {
            parser: ITCHParser::new(),
        }
    }
    
    /// Process a message with explicit parameter format
    pub fn process_message_with_format(&mut self, data: &[u8], format: Option<ParameterFormat>) -> Result<ITCHMessage> {
        // This simulates how we'd process different parameter formats in real implementation
        match format {
            Some(ParameterFormat::LengthPrefixed) => {
                // Handle length-prefixed format (4-byte length + data)
                if data.len() < 4 {
                    return Err(aristo_client::error::ClientError::Other(
                        "Length-prefixed format requires at least 4 bytes".to_string()
                    ));
                }
                
                let len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                if data.len() < 4 + len as usize {
                    return Err(aristo_client::error::ClientError::Other(
                        format!("Incomplete data for declared length {}", len)
                    ));
                }
                
                // Process the actual data after the length prefix
                match self.parser.parse_message(&data[4..]) {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(aristo_client::error::ClientError::Other(format!("Failed to parse message: {}", e)))
                }
            },
            Some(ParameterFormat::Direct) => {
                // Process raw data directly without any length prefix
                match self.parser.parse_message(data) {
                    Ok(msg) => Ok(msg),
                    Err(e) => Err(aristo_client::error::ClientError::Other(format!("Failed to parse message: {}", e)))
                }
            },
            Some(ParameterFormat::Empty) => {
                // Return an empty message for empty parameters
                Ok(ITCHMessage {
                    message_type: MessageType::SystemEvent,
                    stock: None,
                    timestamp: 0,
                    payload: aristo_client::itch::types::MessagePayload::SystemEvent(
                        aristo_client::itch::types::SystemEventMessage::default()
                    ),
                })
            },
            None => {
                // Auto-detect format (additional overhead)
                if data.len() >= 4 {
                    let potential_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
                    if potential_len > 0 && potential_len <= 1024 && data.len() >= 4 + potential_len as usize {
                        // Looks like length-prefixed format
                        return self.process_message_with_format(data, Some(ParameterFormat::LengthPrefixed));
                    }
                }
                
                // Default to direct format
                self.process_message_with_format(data, Some(ParameterFormat::Direct))
            }
        }
    }
}
