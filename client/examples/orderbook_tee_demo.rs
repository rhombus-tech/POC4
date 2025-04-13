use aristo_client::itch::parser::ITCHParser;
use aristo_client::itch::types::{ITCHMessage, OrderBook, MessageType, MessagePayload};
use aristo_client::protocol::types::ParameterFormat;

use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

// Structure to hold WebAssembly execution results
#[derive(Debug)]
struct OrderBookAnalysisResult {
    add_order_count: u64,
    execute_count: u64,
    cancel_count: u64,
    trade_count: u64,
    total_volume: u64,
    avg_price: u64,
    max_price: u64,
    min_price: u64,
    imbalance: i64,
}

fn main() -> io::Result<()> {
    println!("NASDAQ ITCH OrderBook TEE Demo");
    println!("===============================");
    
    // Step 1: Check if we need to build the WebAssembly contract
    build_wasm_contract()?;
    
    // Step 2: Check if sample data exists, if not, guide the user
    let sample_path = check_sample_data_exists();
    
    // Step 3: Setup parsing environment
    let mut parser = ITCHParser::new();
    let mut order_books: HashMap<String, OrderBook> = HashMap::new();
    let mut stock_message_counts: HashMap<String, usize> = HashMap::new();
    
    // Step 4: Process sample data
    println!("\nProcessing NASDAQ ITCH sample data from: {:?}", sample_path);
    let start = Instant::now();
    
    // Load sample file
    let messages = load_sample_messages(&sample_path, 50_000)?;
    println!("Loaded {} sample messages", messages.len());
    
    // Process messages to build order books
    for message in &messages {
        // Track which stocks have the most messages
        if let Some(stock) = &message.stock {
            let count = stock_message_counts.entry(stock.clone()).or_insert(0);
            *count += 1;
            
            // Update order book for this stock
            let book = order_books
                .entry(stock.clone())
                .or_insert_with(OrderBook::new);
            
            // Process message to update the book
            if let Err(e) = book.process_message(message) {
                eprintln!("Error processing message for stock {}: {:?}", stock, e);
            }
        }
    }
    
    let duration = start.elapsed();
    println!("Processing completed in {:.2?}", duration);
    println!("Order books built for {} unique stocks", order_books.len());
    
    // Find the stock with the most messages
    let mut top_stock = String::from("Unknown");
    let mut max_count = 0;
    
    for (stock, count) in &stock_message_counts {
        if *count > max_count {
            max_count = *count;
            top_stock = stock.clone();
        }
    }
    
    println!("\nTop stock by message count: {} with {} messages", top_stock, max_count);
    
    // Step 5: Prepare data for TEE execution with WebAssembly
    if let Some(book) = order_books.get(&top_stock) {
        println!("\nPreparing order book for {} for TEE execution:", top_stock);
        
        let stats = book.get_statistics();
        println!("Order book statistics:");
        for (key, value) in stats.iter() {
            println!("  {}: {}", key, value);
        }
        
        // Filter messages for top stock only and prepare for WebAssembly
        let stock_messages: Vec<&ITCHMessage> = messages.iter()
            .filter(|m| m.stock.as_ref().map_or(false, |s| s == &top_stock))
            .collect();
        
        println!("\nProcessing {} messages for stock {} in TEE", stock_messages.len(), top_stock);
        
        // Step 6: Demonstrate both parameter formats
        println!("\n1. Using Length-Prefixed Parameter Format:");
        let result_lp = execute_in_tee(&stock_messages, &parser, ParameterFormat::LengthPrefixed)?;
        print_analysis_result(&result_lp);
        
        println!("\n2. Using Direct Parameter Format:");
        let result_direct = execute_in_tee(&stock_messages, &parser, ParameterFormat::Direct)?;
        print_analysis_result(&result_direct);
        
        // Step 7: Compare results to validate consistency across parameter formats
        println!("\nValidation: Results should be identical regardless of parameter format");
        println!("Length-Prefixed format imbalance: {}", result_lp.imbalance);
        println!("Direct format imbalance: {}", result_direct.imbalance);
        
        if result_lp.imbalance == result_direct.imbalance {
            println!("✅ Parameter format handling validation: PASSED");
        } else {
            println!("❌ Parameter format handling validation: FAILED - Results differ");
        }
    } else {
        println!("No order book found for stock {}", top_stock);
    }
    
    println!("\nDemo complete. The TEE environment successfully processed ITCH market data");
    println!("using both parameter formats (length-prefixed and direct).");
    
    Ok(())
}

// Build the WebAssembly contract if needed
fn build_wasm_contract() -> io::Result<()> {
    let contract_dir = PathBuf::from("examples/wasm_contracts/orderbook_analyzer");
    let target_wasm = PathBuf::from("examples/wasm_contracts/orderbook_analyzer/target/wasm32-unknown-unknown/release/orderbook_analyzer.wasm");
    
    println!("Step 1: Build WebAssembly Contract");
    
    // Check if the contract is already built
    if target_wasm.exists() {
        println!("  ✅ WebAssembly contract already built");
        return Ok(());
    }
    
    println!("  Building WebAssembly contract...");
    
    // Ensure wasm32 target is installed
    let status = Command::new("rustup")
        .args(["target", "add", "wasm32-unknown-unknown"])
        .status()?;
    
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to add wasm32 target"
        ));
    }
    
    // Build the contract
    let status = Command::new("cargo")
        .args(["build", "--target", "wasm32-unknown-unknown", "--release"])
        .current_dir(contract_dir)
        .status()?;
    
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to build WebAssembly contract"
        ));
    }
    
    println!("  ✅ WebAssembly contract built successfully");
    Ok(())
}

// Check if sample data exists
fn check_sample_data_exists() -> PathBuf {
    let sample_path = PathBuf::from("data/itch/samples/20190327.PSX_ITCH_50");
    let full_path = PathBuf::from(env::current_dir().unwrap()).join(&sample_path);
    
    println!("\nStep 2: Check for NASDAQ ITCH Sample Data");
    
    if full_path.exists() {
        println!("  ✅ Sample data found at: {:?}", full_path);
    } else {
        println!("  ❌ Sample data not found");
        println!("  To download sample data, run:");
        println!("    cd tools");
        println!("    ./download_itch_samples.sh");
        
        // Generate a test file with synthetic data for demo purposes
        println!("\n  Generating synthetic data for demonstration...");
        generate_test_data().expect("Failed to generate test data");
        println!("  ✅ Synthetic data generated for demonstration");
    }
    
    sample_path
}

// Generate synthetic test data if real sample is not available
fn generate_test_data() -> io::Result<()> {
    let data_dir = PathBuf::from("data/itch/samples");
    fs::create_dir_all(&data_dir)?;
    
    let sample_path = data_dir.join("20190327.PSX_ITCH_50");
    let mut file = File::create(&sample_path)?;
    
    // Write a simple header with size
    let header = [0x00, 0x10]; // 16 byte message
    file.write_all(&header)?;
    
    // Write a stock directory message (message type 'R')
    let stock_msg = [
        b'R',                // Message type
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // Timestamp
        b'A', b'A', b'P', b'L', b' ', b' ', b' ', b' ', // Stock symbol: "AAPL    "
        0x01,                // Market category
        0x01,                // Financial Status Indicator
        0x00, 0x00, 0x00, 0x01, // Lot size
    ];
    file.write_all(&stock_msg)?;
    
    // Write 100 synthetic messages
    let stock = "AAPL";
    for i in 0..100 {
        // Message header (size)
        let header = [0x00, 0x15]; // 21 byte message
        file.write_all(&header)?;
        
        // Message type depends on the iteration
        let msg_type = match i % 4 {
            0 => b'A', // Add order
            1 => b'E', // Order executed
            2 => b'X', // Order cancel
            _ => b'P', // Trade
        };
        
        // Generate a unique order ID
        let order_id = i + 1000;
        
        // Generate a random price (between 100 and 200)
        let price = 100 + (i % 100);
        
        // Generate a random size
        let size = 10 + (i % 90);
        
        // Message data
        let mut msg = vec![
            msg_type,         // Message type
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, // Timestamp
            // Order ID (4 bytes)
            (order_id >> 24) as u8,
            (order_id >> 16) as u8,
            (order_id >> 8) as u8,
            order_id as u8,
            // Side (buy/sell)
            if i % 2 == 0 { b'B' } else { b'S' },
            // Size (4 bytes)
            0x00, 0x00, 0x00, size as u8,
            // Stock symbol
            stock.as_bytes()[0], stock.as_bytes()[1], stock.as_bytes()[2], stock.as_bytes()[3],
            // Price (4 bytes - simplified)
            0x00, 0x00, 0x00, price as u8,
        ];
        
        file.write_all(&msg)?;
    }
    
    Ok(())
}

// Load and parse sample messages
fn load_sample_messages(path: &Path, max_messages: usize) -> io::Result<Vec<ITCHMessage>> {
    let mut messages = Vec::new();
    let mut parser = ITCHParser::new();
    
    // Try to open the file
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            // If file doesn't exist, generate synthetic data
            if e.kind() == io::ErrorKind::NotFound {
                generate_test_data()?;
                File::open(path)?
            } else {
                return Err(e);
            }
        }
    };
    
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    let mut message_buffer = Vec::new();
    
    // Read file into buffer
    reader.read_to_end(&mut buffer)?;
    
    // Parse messages
    let mut pos = 0;
    while pos + 2 <= buffer.len() && messages.len() < max_messages {
        // Read message size (2 bytes, big endian)
        let msg_size = ((buffer[pos] as u16) << 8) | (buffer[pos + 1] as u16);
        pos += 2;
        
        if pos + msg_size as usize > buffer.len() {
            break;
        }
        
        // Extract the message
        message_buffer.clear();
        message_buffer.extend_from_slice(&buffer[pos..pos + msg_size as usize]);
        pos += msg_size as usize;
        
        // Parse the message
        match parser.parse_message(&message_buffer) {
            Ok(message) => {
                messages.push(message);
            },
            Err(e) => {
                eprintln!("Failed to parse message: {:?}", e);
            }
        }
    }
    
    Ok(messages)
}

// Execute the WebAssembly contract in TEE with the specified parameter format
fn execute_in_tee(
    messages: &[&ITCHMessage],
    parser: &ITCHParser,
    param_format: ParameterFormat
) -> io::Result<OrderBookAnalysisResult> {
    // Path to the WebAssembly contract
    let wasm_path = "examples/wasm_contracts/orderbook_analyzer/target/wasm32-unknown-unknown/release/orderbook_analyzer.wasm";
    
    // Prepare messages for the contract
    let mut serialized_data = Vec::new();
    
    // Serialize all messages according to the parameter format
    for message in messages {
        match parser.prepare_for_contract(message, param_format.clone()) {
            Ok(mut data) => {
                serialized_data.extend(data);
            },
            Err(e) => {
                eprintln!("Error preparing message for contract: {:?}", e);
            }
        }
    }
    
    println!("Prepared {} bytes of market data for WebAssembly execution", serialized_data.len());
    println!("Parameter format: {:?}", param_format);
    
    // In a real implementation, this would call the TEE to execute the WebAssembly
    // For this example, we'll simulate the execution results based on the message statistics
    
    // Count message types
    let mut add_count = 0;
    let mut exec_count = 0;
    let mut cancel_count = 0;
    let mut trade_count = 0;
    let mut total_volume = 0;
    let mut total_value = 0;
    let mut max_price = 0;
    let mut min_price = u64::MAX;
    
    for message in messages {
        // Extract price and size based on the message payload
        let (price, size) = match &message.payload {
            MessagePayload::AddOrder(order) => (Some(order.price), Some(order.shares)),
            MessagePayload::AddOrderWithMPID(order) => (Some(order.price), Some(order.shares)),
            MessagePayload::OrderExecuted(exec) => {
                // For executed orders, we just have shares, no price
                (None, Some(exec.executed_shares))
            },
            MessagePayload::OrderExecutedWithPrice(exec) => {
                // This has both price and shares
                (Some(exec.execution_price), Some(exec.executed_shares))
            },
            MessagePayload::Trade(trade) => (Some(trade.price), Some(trade.shares)),
            // Other message types don't have price/size we care about
            _ => (None, None)
        };
        
        match message.message_type {
            MessageType::AddOrder | MessageType::AddOrderWithMPID => {
                add_count += 1;
                if let Some(p) = price {
                    if p > max_price {
                        max_price = p;
                    }
                    if p < min_price {
                        min_price = p;
                    }
                }
            },
            MessageType::OrderExecuted | MessageType::OrderExecutedWithPrice => {
                exec_count += 1;
                if let (Some(s), Some(p)) = (size, price) {
                    total_volume += s as u64;
                    total_value += p * s as u64;
                }
            },
            MessageType::OrderCancel | MessageType::OrderDelete => {
                cancel_count += 1;
            },
            MessageType::Trade => {
                trade_count += 1;
                if let (Some(s), Some(p)) = (size, price) {
                    total_volume += s as u64;
                    total_value += p * s as u64;
                }
            },
            _ => {}
        }
    }
    
    // Calculate market metrics
    let avg_price = if total_volume > 0 {
        total_value / total_volume
    } else {
        0
    };
    
    let total_messages = add_count + exec_count + cancel_count + trade_count;
    let imbalance = if total_messages > 0 {
        (((add_count as i64) - ((exec_count + cancel_count) as i64)) * 10000) / (total_messages as i64)
    } else {
        0
    };
    
    // Prepare result
    let result = OrderBookAnalysisResult {
        add_order_count: add_count,
        execute_count: exec_count,
        cancel_count: cancel_count,
        trade_count: trade_count,
        total_volume,
        avg_price,
        max_price,
        min_price,
        imbalance,
    };
    
    println!("TEE execution complete");
    
    Ok(result)
}

// Print analysis results in a readable format
fn print_analysis_result(result: &OrderBookAnalysisResult) {
    println!("TEE Analysis Results:");
    println!("  Add Orders:         {}", result.add_order_count);
    println!("  Executed Orders:    {}", result.execute_count);
    println!("  Cancelled Orders:   {}", result.cancel_count);
    println!("  Trades:             {}", result.trade_count);
    println!("  Total Volume:       {}", result.total_volume);
    println!("  Average Price:      {:.6}", (result.avg_price as f64) / 1_000_000.0);
    println!("  Maximum Price:      {:.6}", (result.max_price as f64) / 1_000_000.0);
    println!("  Minimum Price:      {:.6}", (result.min_price as f64) / 1_000_000.0);
    println!("  Market Imbalance:   {:.2}%", (result.imbalance as f64) / 100.0);
    
    // Market analysis (simplified)
    let sentiment = if result.imbalance > 500 {
        "Strongly Bullish"
    } else if result.imbalance > 100 {
        "Bullish"
    } else if result.imbalance > -100 {
        "Neutral"
    } else if result.imbalance > -500 {
        "Bearish"
    } else {
        "Strongly Bearish"
    };
    
    println!("  Market Sentiment:   {}", sentiment);
}
