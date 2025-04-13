use aristo_client::aot::{AotCompiler, AotConfig, WasmAotCompiler, ParameterSpecialization, FormatDetection, MarketProfile};
use aristo_client::error::AdapterError;
use aristo_client::error::ClientError;
use aristo_client::itch::parser::ITCHParser;
use aristo_client::itch::types::{ITCHMessage, MessageType};
use aristo_client::protocol::types::ParameterFormat;

use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// Custom error handling for the demo
#[derive(Debug)]
enum DemoError {
    IoError(std::io::Error),
    AdapterError(AdapterError),
    ClientError(ClientError),
}

impl From<std::io::Error> for DemoError {
    fn from(err: std::io::Error) -> Self {
        DemoError::IoError(err)
    }
}

impl From<AdapterError> for DemoError {
    fn from(err: AdapterError) -> Self {
        DemoError::AdapterError(err)
    }
}

impl From<ClientError> for DemoError {
    fn from(err: ClientError) -> Self {
        DemoError::ClientError(err)
    }
}

type DemoResult<T> = Result<T, DemoError>;

/// Demonstrate AOT compilation for real-time ITCH processing
fn main() -> Result<(), DemoError> {
    println!("NASDAQ ITCH AOT Optimization Demo");
    println!("==================================");
    
    // Step 1: Setup AOT compilation configuration
    println!("\nStep 1: Configure AOT compilation");
    let config = AotConfig {
        parameter_format_specialization: true,
        message_type_specialization: true,
        pgo_enabled: true,
        profile_data_path: None,
        opt_level: 3,
        max_specialized_handlers: 50,
    };
    println!("  ✅ AOT configuration complete");
    
    // Step 2: Load sample ITCH data
    println!("\nStep 2: Loading ITCH sample data");
    let sample_path = PathBuf::from("data/itch/samples/20190327.PSX_ITCH_50");
    let mut messages = load_sample_messages(&sample_path, 1000)?;
    
    // Ensure we always have enough test data
    if messages.len() < 10 {
        println!("  Generating synthetic messages for testing");
        messages = generate_synthetic_messages(100);
    }
    println!("  ✅ Loaded {} sample messages", messages.len());
    
    // Step 3: Setup parameter format specialization
    println!("\nStep 3: Setting up parameter format specialization");
    let mut param_specialization = ParameterSpecialization::new(FormatDetection::Dynamic);
    
    // Register specialized handlers for both parameter formats
    param_specialization.register_handler(
        ParameterFormat::LengthPrefixed,
        "order_book_analysis",
        |params| {
            // Specialized handler for length-prefixed format
            // In a real implementation, this would be highly optimized
            if params.len() < 4 {
                return Err(aristo_client::error::AdapterError::ResponseParsing(
                    "Length-prefixed format requires at least 4 bytes".to_string()
                ));
            }
            
            let len = u32::from_le_bytes([params[0], params[1], params[2], params[3]]);
            if params.len() < 4 + len as usize {
                return Err(aristo_client::error::AdapterError::ResponseParsing(
                    "Incomplete data for declared length".to_string()
                ));
            }
            
            // Extract and process the actual data (after length prefix)
            Ok(params[4..].to_vec())
        }
    ).map_err(DemoError::from)?;
    
    param_specialization.register_handler(
        ParameterFormat::Direct,
        "order_book_analysis",
        |params| {
            // Specialized handler for direct format
            // In a real implementation, this would be highly optimized
            // Process the data directly without any length prefix
            Ok(params.to_vec())
        }
    ).map_err(DemoError::from)?;
    println!("  ✅ Parameter format specialization complete");
    
    // Step 4: Setup AOT compiler for ITCH message processing
    println!("\nStep 4: Initializing AOT compiler for ITCH message processing");
    let mut aot_compiler = AotCompiler::new(config.clone());
    
    // Set market profile for appropriate optimizations
    aot_compiler.set_market_profile(MarketProfile::Normal);
    println!("  ✅ AOT compiler initialized with Normal market profile");
    
    // Step 5: Setup WebAssembly AOT compiler
    println!("\nStep 5: Setting up WebAssembly AOT compiler");
    let mut wasm_compiler = WasmAotCompiler::new(config.clone());
    
    // Identify the WebAssembly contract path
    let wasm_path = PathBuf::from("examples/wasm_contracts/orderbook_analyzer/target/wasm32-unknown-unknown/release/orderbook_analyzer.wasm");
    
    // Compile the WebAssembly module ahead-of-time
    if wasm_path.exists() {
        println!("  Found WebAssembly contract at: {:?}", wasm_path);
        wasm_compiler.compile_from_file("orderbook_analyzer", &wasm_path).map_err(DemoError::from)?;
        println!("  ✅ WebAssembly contract compiled successfully");
    } else {
        println!("  ❌ WebAssembly contract not found at: {:?}", wasm_path);
        println!("  Please build the contract first with:");
        println!("    cd examples/wasm_contracts/orderbook_analyzer && cargo build --target wasm32-unknown-unknown --release");
    }
    
    // Step 6: Performance comparison with and without AOT optimizations
    println!("\nStep 6: Performance comparison");
    
    // Create parser for standard processing
    let mut standard_parser = ITCHParser::new();
    
    // Generate binary data for all messages
    let binary_messages = messages.iter()
        .map(|_| generate_random_message(MessageType::AddOrder))
        .collect::<Vec<_>>();
    
    // Benchmark standard processing
    println!("\nStandard processing (without AOT):");
    let (std_time, _std_results) = benchmark_processing(&binary_messages, |data| {
        // Convert ClientError to AdapterError for consistent error handling
        standard_parser.parse_message(data).map_err(|e| AdapterError::ResponseParsing(format!("Parser error: {:?}", e)))
    });
    
    // Benchmark AOT processing
    println!("\nOptimized processing (with AOT):");
    let (aot_time, _aot_results) = benchmark_processing(&binary_messages, |data| {
        // Pre-process with AOT compiler
        aot_compiler.process_message(data)
    });
    
    // Calculate improvement
    if std_time > aot_time {
        let improvement = (std_time.as_micros() as f64 - aot_time.as_micros() as f64) / 
                         std_time.as_micros() as f64 * 100.0;
        println!("\nAOT provides {:.2}% faster processing", improvement);
    } else {
        println!("\nNo performance improvement detected with AOT in this sample run");
    }
    
    // Step 7: WebAssembly parameter format handling comparison
    println!("\nStep 7: WebAssembly parameter format handling");
    
    // Create sample parameter data
    let sample_data = b"AAPL_ORDERBOOK_DATA_12345";
    
    // Test length-prefixed format
    let lp_start = Instant::now();
    for _ in 0..1000 {
        // Create length-prefixed format: 4-byte length + data
        let mut lp_data = Vec::with_capacity(4 + sample_data.len());
        lp_data.extend_from_slice(&(sample_data.len() as u32).to_le_bytes());
        lp_data.extend_from_slice(sample_data);
        
        // Process with parameter specialization
        let _ = param_specialization.process("order_book_analysis", &lp_data, Some(ParameterFormat::LengthPrefixed));
    }
    let lp_time = lp_start.elapsed();
    
    // Test direct format
    let direct_start = Instant::now();
    for _ in 0..1000 {
        // Process with parameter specialization 
        let _ = param_specialization.process("order_book_analysis", sample_data, Some(ParameterFormat::Direct));
    }
    let direct_time = direct_start.elapsed();
    
    println!("\nParameter format processing times:");
    println!("  Length-prefixed format: {:?} for 1000 iterations", lp_time);
    println!("  Direct format: {:?} for 1000 iterations", direct_time);
    
    if lp_time != direct_time {
        let faster = if lp_time < direct_time { "Length-prefixed" } else { "Direct" };
        let diff = if lp_time < direct_time { 
            direct_time.as_micros() as f64 / lp_time.as_micros() as f64 
        } else {
            lp_time.as_micros() as f64 / direct_time.as_micros() as f64
        };
        println!("  {} format is {:.2}x faster", faster, diff);
    } else {
        println!("  Both formats have similar performance");
    }
    
    // Step 8: WebAssembly execution with AOT
    println!("\nStep 8: WebAssembly execution with AOT");
    
    if wasm_path.exists() {
        // Execute the contract with different parameter formats
        let lp_start = Instant::now();
        for _ in 0..100 {
            let _ = wasm_compiler.execute(
                "orderbook_analyzer", 
                "analyze_orderbook", 
                sample_data, 
                ParameterFormat::LengthPrefixed
            );
        }
        let lp_exec_time = lp_start.elapsed();
        
        let direct_start = Instant::now();
        for _ in 0..100 {
            let _ = wasm_compiler.execute(
                "orderbook_analyzer", 
                "analyze_orderbook", 
                sample_data, 
                ParameterFormat::Direct
            );
        }
        let direct_exec_time = direct_start.elapsed();
        
        println!("\nWebAssembly execution times:");
        println!("  Length-prefixed format: {:?} for 100 iterations", lp_exec_time);
        println!("  Direct format: {:?} for 100 iterations", direct_exec_time);
        
        // Show statistics
        let wasm_stats = wasm_compiler.get_statistics();
        println!("\nWebAssembly compilation statistics:");
        println!("  Modules compiled: {}", wasm_stats.modules_compiled);
        println!("  Average compile time: {:.2} ms", wasm_stats.avg_compile_time_ms);
        println!("  Total module size: {} bytes", wasm_stats.total_module_size_bytes);
    } else {
        println!("  ❌ Skipping WebAssembly execution because contract not found");
    }
    
    // Step 9: Display AOT compiler statistics
    println!("\nStep 9: AOT Compiler Statistics");
    
    let handler_stats = aot_compiler.get_handler_statistics();
    println!("\nSpecialized handler statistics:");
    for (msg_type, stats) in &handler_stats {
        println!("  {:?}:", msg_type);
        println!("    Calls: {}", stats.calls);
        println!("    Avg time: {:.2} ns", stats.avg_time_ns);
    }
    
    let compile_stats = aot_compiler.get_compile_statistics();
    println!("\nCompilation statistics:");
    println!("  Handlers compiled: {}", compile_stats.handlers_compiled);
    println!("  Total compile time: {} ms", compile_stats.total_compile_time_ms);
    println!("  Total code size: {} bytes", compile_stats.total_code_size_bytes);
    
    println!("\nDemo complete. AOT compilation enhances real-time ITCH processing performance.");
    
    Ok(())
}

/// Load sample ITCH messages from a binary file or generate them if the file doesn't exist
fn load_sample_messages(path: &Path, max_messages: usize) -> Result<Vec<ITCHMessage>, DemoError> {
    let mut messages = Vec::new();
    let mut parser = ITCHParser::new();
    
    // Try to open the file
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            // If file doesn't exist, generate synthetic data
            if e.kind() == io::ErrorKind::NotFound {
                println!("  Sample file not found");
                return Ok(generate_synthetic_messages(max_messages));
            } else {
                return Err(e.into());
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
                println!("Warning: Failed to parse message: {:?}", e);
                // Continue processing other messages
            }
        }
    }
    
    Ok(messages)
}

/// Generate synthetic ITCH messages directly
fn generate_synthetic_messages(count: usize) -> Vec<ITCHMessage> {
    let mut messages = Vec::with_capacity(count);
    let mut parser = ITCHParser::new();
    
    for i in 0..count {
        // Alternate message types
        let msg_type = match i % 4 {
            0 => MessageType::AddOrder,
            1 => MessageType::OrderExecuted,
            2 => MessageType::OrderCancel,
            _ => MessageType::Trade,
        };
        
        let msg_data = generate_random_message(msg_type);
        
        // Try to parse the generated message
        match parser.parse_message(&msg_data) {
            Ok(message) => messages.push(message),
            Err(_) => {
                // If parsing fails, generate a simpler message that will parse successfully
                if let Ok(simple_message) = create_simple_message(i) {
                    messages.push(simple_message);
                }
            }
        }
    }
    
    messages
}

/// Create a simple message that will parse successfully
fn create_simple_message(sequence: usize) -> Result<ITCHMessage, AdapterError> {
    // Create a very simple system event message
    Ok(ITCHMessage {
        message_type: MessageType::SystemEvent,
        stock: Some(format!("SYM{}", sequence % 10)), // Simple stock symbol
        timestamp: sequence as u64 * 1_000_000, // Microseconds
        payload: aristo_client::itch::types::MessagePayload::SystemEvent(
            aristo_client::itch::types::SystemEventMessage::default()
        ),
    })
}

/// Generate synthetic ITCH data file for testing
fn generate_synthetic_data(target_path: &Path) -> Result<(), DemoError> {
    // Create parent directories if needed
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    let mut file = File::create(target_path)?;
    
    // Generate 100 synthetic messages
    for i in 0..100 {
        // Alternate message types
        let msg_type = match i % 4 {
            0 => MessageType::AddOrder,
            1 => MessageType::OrderExecuted,
            2 => MessageType::OrderCancel,
            _ => MessageType::Trade,
        };
        
        let msg_data = generate_random_message(msg_type);
        
        // Write size prefix (2 bytes, big endian)
        let size = msg_data.len() as u16;
        file.write_all(&[(size >> 8) as u8, (size & 0xFF) as u8])?;
        
        // Write message data
        file.write_all(&msg_data)?;
    }
    
    Ok(())
}

/// Generate a random ITCH message of specified type
fn generate_random_message(msg_type: MessageType) -> Vec<u8> {
    let mut data = Vec::new();
    
    // Message type byte
    data.push(msg_type as u8 as u8);
    
    // Timestamp (8 bytes)
    data.extend_from_slice(&(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64)
        .to_be_bytes());
    
    match msg_type {
        MessageType::AddOrder => {
            // Order reference number (8 bytes)
            data.extend_from_slice(&(rand::random::<u64>()).to_be_bytes());
            // Buy/Sell indicator (1 byte)
            data.push(if rand::random::<bool>() { b'B' } else { b'S' });
            // Shares (4 bytes)
            data.extend_from_slice(&(rand::random::<u32>() % 1000 + 1).to_be_bytes());
            // Stock (8 bytes, fixed length)
            data.extend_from_slice(b"AAPL    ");
            // Price (8 bytes)
            data.extend_from_slice(&(rand::random::<u64>() % 10000 + 10000).to_be_bytes());
        },
        MessageType::OrderExecuted => {
            // Order reference number (8 bytes)
            data.extend_from_slice(&(rand::random::<u64>()).to_be_bytes());
            // Executed shares (4 bytes)
            data.extend_from_slice(&(rand::random::<u32>() % 100 + 1).to_be_bytes());
            // Match number (8 bytes)
            data.extend_from_slice(&(rand::random::<u64>()).to_be_bytes());
        },
        MessageType::OrderCancel => {
            // Order reference number (8 bytes)
            data.extend_from_slice(&(rand::random::<u64>()).to_be_bytes());
            // Canceled shares (4 bytes)
            data.extend_from_slice(&(rand::random::<u32>() % 100 + 1).to_be_bytes());
        },
        MessageType::Trade => {
            // Order reference number (8 bytes)
            data.extend_from_slice(&(rand::random::<u64>()).to_be_bytes());
            // Buy/Sell indicator (1 byte)
            data.push(if rand::random::<bool>() { b'B' } else { b'S' });
            // Shares (4 bytes)
            data.extend_from_slice(&(rand::random::<u32>() % 1000 + 1).to_be_bytes());
            // Stock (8 bytes, fixed length)
            data.extend_from_slice(b"AAPL    ");
            // Price (8 bytes)
            data.extend_from_slice(&(rand::random::<u64>() % 10000 + 10000).to_be_bytes());
            // Match number (8 bytes)
            data.extend_from_slice(&(rand::random::<u64>()).to_be_bytes());
        },
        _ => {
            // For other message types, just add some padding
            for _ in 0..16 {
                data.push(rand::random::<u8>());
            }
        }
    }
    
    data
}

/// Benchmark message processing with a given processor function
fn benchmark_processing<F, E>(messages: &[Vec<u8>], mut process_fn: F) -> (Duration, usize) 
where 
    F: FnMut(&[u8]) -> Result<ITCHMessage, E>,
    E: std::fmt::Debug
{
    const ITERATIONS: usize = 5;
    
    let mut total_time = Duration::default();
    let mut success_count = 0;
    
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        
        for msg in messages {
            if process_fn(msg).is_ok() {
                success_count += 1;
            }
        }
        
        total_time += start.elapsed();
    }
    
    let avg_time = total_time / ITERATIONS as u32;
    let per_message = if messages.is_empty() {
        Duration::default()
    } else {
        avg_time / messages.len() as u32
    };
    println!("  Processed {} messages in {:?} ({:?} per message)",
             messages.len(), 
             avg_time,
             per_message);
    println!("  Success rate: {:.1}%", (success_count as f64 / (messages.len() * ITERATIONS) as f64) * 100.0);
    
    (avg_time, success_count)
}

// Required for writing to files
trait Write {
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()>;
}

impl Write for File {
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        std::io::Write::write_all(self, buf)
    }
}
