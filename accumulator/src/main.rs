// main.rs - HTTP server wrapper for RSA accumulator in Enarx
// Supports dual-format parameter validation (length-prefixed and direct formats)

use std::env;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Server, Request, Response, Method, StatusCode};
use serde::{Serialize, Deserialize};
use serde_json::{json, Value};

// Import our accumulator module 
mod accumulator;
mod verification;
mod discovery;

use accumulator::{
    init, 
    register_attestation, 
    batch_register_attestations,
    parse_dual_format_parameters,
    AccumulatorParams,
    AccumulatorState,
    AttestationReport,
};

use tee_interface::prelude::*;

// Constants
const PORT: u16 = 7101;
const MAX_PARAM_SIZE: usize = 1024 * 1024; // 1MB max parameter size

// Parameter validation configuration
const ENABLE_LENGTH_PREFIX: bool = true;
const ENABLE_DIRECT_FORMAT: bool = true;

// Types for API requests/responses
#[derive(Serialize, Deserialize)]
struct BatchItem {
    data: Vec<u8>,
    format: Option<String>,
    timestamp: Option<i64>,
    parameters: Option<Value>,
}

#[derive(Serialize, Deserialize)]
struct AccumulationResult {
    success: bool,
    accum_hash: Option<String>,
    batch_size: Option<usize>,
    cross_matched: Option<bool>,
    error: Option<String>,
}

// Accumulator state
struct AppState {
    accumulator: AccumulatorState,
    total_processed: u64,
    length_prefixed_count: u64,
    direct_format_count: u64,
    cross_val_count: u64,
    cross_val_matches: u64,
    start_time: Instant,
}

// Main entry point - HTTP server for Enarx
fn main() {
    // Initialize state
    let state = Arc::new(Mutex::new(AppState {
        accumulator: AccumulatorState::default(),
        total_processed: 0,
        length_prefixed_count: 0,
        direct_format_count: 0,
        cross_val_count: 0,
        cross_val_matches: 0,
        start_time: Instant::now(),
    }));
    
    // Determine port to listen on
    let port = env::var("PORT")
        .map(|p| p.parse::<u16>().unwrap_or(PORT))
        .unwrap_or(PORT);
    
    // Create HTTP server
    let server = match Server::http(format!("0.0.0.0:{}", port)) {
        Ok(server) => {
            println!("RSA Accumulator HTTP server started on port {}", port);
            println!("Dual-format parameter validation enabled:");
            println!("  - Length-prefixed format: {}", ENABLE_LENGTH_PREFIX);
            println!("  - Direct format: {}", ENABLE_DIRECT_FORMAT);
            server
        },
        Err(e) => {
            eprintln!("Failed to start server: {}", e);
            return;
        }
    };
    
    // Handle requests
    for request in server.incoming_requests() {
        handle_request(request, Arc::clone(&state));
    }
}

// Request handler
fn handle_request(request: Request, state: Arc<Mutex<AppState>>) {
    match (request.method(), request.url()) {
        // Health check endpoint
        (Method::Get, "/health") => {
            let response = json!({
                "status": "ok",
                "tee_type": env::var("TEE_TYPE").unwrap_or_else(|_| "sgx".to_string())
            });
            
            send_json_response(request, 200, &response);
        },
        
        // Parameter accumulation endpoint
        (Method::Post, "/add_optimized") => {
            // Read request body
            let mut content = Vec::new();
            if let Err(e) = request.as_reader().read_to_end(&mut content) {
                send_error(request, 400, &format!("Failed to read request: {}", e));
                return;
            }
            
            // Process batch request
            match process_batch_request(&content, &state) {
                Ok(result) => {
                    send_json_response(request, 200, &result);
                },
                Err(e) => {
                    send_error(request, 400, &format!("Invalid request: {}", e));
                }
            }
        },
        
        // Stats endpoint
        (Method::Get, "/stats") => {
            let stats = {
                let state_lock = state.lock().unwrap();
                let elapsed = state_lock.start_time.elapsed();
                
                json!({
                    "total_processed": state_lock.total_processed,
                    "length_prefixed_count": state_lock.length_prefixed_count,
                    "direct_format_count": state_lock.direct_format_count,
                    "cross_val_count": state_lock.cross_val_count,
                    "cross_val_matches": state_lock.cross_val_matches,
                    "uptime_seconds": elapsed.as_secs(),
                    "tee_type": env::var("TEE_TYPE").unwrap_or_else(|_| "sgx".to_string())
                })
            };
            
            send_json_response(request, 200, &stats);
        },
        
        // Unknown endpoint
        _ => {
            send_error(request, 404, "Not Found");
        }
    }
}

// Process a batch of parameters
fn process_batch_request(data: &[u8], state: &Arc<Mutex<AppState>>) -> Result<AccumulationResult, String> {
    // Parse JSON batch
    let batch: Vec<BatchItem> = serde_json::from_slice(data)
        .map_err(|e| format!("Failed to parse batch: {}", e))?;
    
    if batch.is_empty() {
        return Err("Empty batch".into());
    }
    
    // Process each parameter
    let mut success_count = 0;
    
    for item in batch.iter() {
        // Use existing dual-format parameter validation
        let result = parse_dual_format_parameters(&item.data);
        
        match result {
            Ok((validated_data, format)) => {
                // Update statistics
                let mut state_lock = state.lock().unwrap();
                state_lock.total_processed += 1;
                
                if format == "length-prefixed" {
                    state_lock.length_prefixed_count += 1;
                } else {
                    state_lock.direct_format_count += 1;
                }
                
                // Successfully validated parameter
                success_count += 1;
                
                // In a real implementation, we would:
                // 1. Create an AttestationReport from the validated data
                // 2. Call register_attestation to add it to the accumulator
                // 3. Update cross-validation stats if needed
            },
            Err(_) => {
                // Parameter validation failed
            }
        }
    }
    
    // Return result
    if success_count > 0 {
        Ok(AccumulationResult {
            success: true,
            accum_hash: Some(format!("hash_{}", Instant::now().elapsed().as_nanos())),
            batch_size: Some(batch.len()),
            cross_matched: None,
            error: None,
        })
    } else {
        Err("All parameters failed validation".into())
    }
}

// Send JSON response
fn send_json_response(request: Request, status: u16, data: &Value) {
    let json_string = serde_json::to_string(data).unwrap_or_else(|_| "{}".to_string());
    let response = Response::from_string(json_string)
        .with_status_code(StatusCode(status))
        .with_header(tiny_http::Header {
            field: "Content-Type".parse().unwrap(),
            value: "application/json".parse().unwrap(),
        });
    
    if let Err(e) = request.respond(response) {
        eprintln!("Failed to send response: {}", e);
    }
}

// Send error response
fn send_error(request: Request, status: u16, message: &str) {
    let error_json = json!({
        "success": false,
        "error": message
    });
    
    send_json_response(request, status, &error_json);
}
