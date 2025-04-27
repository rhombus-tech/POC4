use std::env;
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio;
use tee_controller::HyperTeeController;
use tee_interface::{ExecutionPayload, ExecutionParams, TeeExecutor};

// Configuration for the benchmark
struct BenchmarkConfig {
    coordinator_url: String,
    num_operations: usize,
    concurrency_level: usize,
    operation_type: String,
    region_id: String,
}

impl BenchmarkConfig {
    fn from_env() -> Self {
        Self {
            coordinator_url: env::var("COORDINATOR_URL")
                .unwrap_or_else(|_| "http://localhost:8080".to_string()),
            num_operations: env::var("NUM_OPERATIONS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1000),
            concurrency_level: env::var("CONCURRENCY_LEVEL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            operation_type: env::var("OPERATION_TYPE")
                .unwrap_or_else(|_| "mixed".to_string()),
            region_id: env::var("REGION_ID")
                .unwrap_or_else(|_| "default".to_string()),
        }
    }
}

// Performance metrics
#[derive(Debug)]
struct BenchmarkResults {
    total_operations: usize,
    successful_operations: usize,
    failed_operations: usize,
    total_duration_ms: u128,
    average_latency_ms: f64,
    throughput_ops_per_sec: f64,
    p50_latency_ms: f64,
    p95_latency_ms: f64,
    p99_latency_ms: f64,
}

async fn run_benchmark() -> Result<(), Box<dyn Error>> {
    let config = BenchmarkConfig::from_env();
    
    println!("Starting TEE benchmark with configuration:");
    println!("  Coordinator URL: {}", config.coordinator_url);
    println!("  Number of operations: {}", config.num_operations);
    println!("  Concurrency level: {}", config.concurrency_level);
    println!("  Operation type: {}", config.operation_type);
    println!("  Region ID: {}", config.region_id);
    
    // Set up environment variables
    env::set_var("COORDINATOR_URL", &config.coordinator_url);
    env::set_var("USE_COORDINATOR", "true");
    
    // Create the controller
    let controller = Arc::new(HyperTeeController::new().await);
    
    // Initialize and register with the coordinator
    println!("Registering with coordinator...");
    controller.initialize_coordinator().await
        .map_err(|e| format!("Failed to initialize: {}", e))?;
    
    // Deploy a test contract
    println!("Deploying test contract...");
    let contract_wasm = b"mock contract binary".to_vec();
    let contract_id = controller.deploy_contract(&contract_wasm, &config.region_id).await
        .map_err(|e| format!("Failed to deploy contract: {}", e))?;
    
    println!("Contract deployed with ID: {}", contract_id);
    
    // Prepare for benchmark
    println!("Preparing benchmark data...");
    let mut latencies = Vec::with_capacity(config.num_operations);
    let mut success_count = 0;
    let mut failure_count = 0;
    
    // Start the benchmark
    println!("Starting benchmark...");
    let start_time = Instant::now();
    
    // Create batches of operations
    let batch_size = config.num_operations / config.concurrency_level;
    let mut handles = Vec::new();
    
    for batch in 0..config.concurrency_level {
        let controller_clone = controller.clone();
        let contract_id_clone = contract_id.clone();
        let region_id_clone = config.region_id.clone();
        let operation_type_clone = config.operation_type.clone();
        
        let handle = tokio::spawn(async move {
            let mut batch_latencies = Vec::with_capacity(batch_size);
            let mut batch_success = 0;
            let mut batch_failure = 0;
            
            for i in 0..(batch_size) {
                let operation_index = batch * batch_size + i;
                let key = format!("key_{}", operation_index);
                let value = format!("value_{}", operation_index);
                
                // Determine operation type
                let (function, input) = match operation_type_clone.as_str() {
                    "write" => ("execute", format!("store,{},{}", key, value).into_bytes()),
                    "read" => ("execute", format!("get,{}", key).into_bytes()),
                    "compute" => ("execute", format!("add,10,{}", operation_index % 100).into_bytes()),
                    _ => {
                        // Mixed operations
                        match operation_index % 3 {
                            0 => ("execute", format!("store,{},{}", key, value).into_bytes()),
                            1 => ("execute", format!("get,{}", key).into_bytes()),
                            _ => ("execute", format!("add,10,{}", operation_index % 100).into_bytes()),
                        }
                    }
                };
                
                // Create payload
                let payload = ExecutionPayload {
                    input,
                    params: ExecutionParams {
                        id_to: contract_id_clone.clone(),
                        function_call: function.to_string(),
                        detailed_proof: false,
                        expected_hash: Vec::new(),
                    },
                    operation_id: None,
                    previous_operation_id: None,
                    operation_context: None,
                    region_id: None,
                    target_tee: None,
                    tee_type: None,
                    allow_fallback: Some(true),
                };
                
                // Execute and measure latency
                let op_start = Instant::now();
                let result = controller_clone.execute(&payload).await;
                let latency = op_start.elapsed().as_millis();
                
                // Record result
                match result {
                    Ok(_) => {
                        batch_success += 1;
                        batch_latencies.push(latency);
                    }
                    Err(_) => {
                        batch_failure += 1;
                        batch_latencies.push(latency);
                    }
                }
                
                // Small delay to prevent overwhelming the system
                if operation_index % 10 == 0 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
            
            (batch_latencies, batch_success, batch_failure)
        });
        
        handles.push(handle);
    }
    
    // Wait for all batches to complete
    for handle in handles {
        let (batch_latencies, batch_success, batch_failure) = handle.await?;
        latencies.extend(batch_latencies);
        success_count += batch_success;
        failure_count += batch_failure;
    }
    
    let total_duration = start_time.elapsed();
    
    // Calculate results
    let total_operations = success_count + failure_count;
    
    // Sort latencies for percentile calculations
    latencies.sort();
    
    let average_latency = if !latencies.is_empty() {
        latencies.iter().sum::<u128>() as f64 / latencies.len() as f64
    } else {
        0.0
    };
    
    let p50_index = (latencies.len() as f64 * 0.5) as usize;
    let p95_index = (latencies.len() as f64 * 0.95) as usize;
    let p99_index = (latencies.len() as f64 * 0.99) as usize;
    
    let p50_latency = if !latencies.is_empty() && p50_index < latencies.len() {
        latencies[p50_index] as f64
    } else {
        0.0
    };
    
    let p95_latency = if !latencies.is_empty() && p95_index < latencies.len() {
        latencies[p95_index] as f64
    } else {
        0.0
    };
    
    let p99_latency = if !latencies.is_empty() && p99_index < latencies.len() {
        latencies[p99_index] as f64
    } else {
        0.0
    };
    
    let throughput = if total_duration.as_secs_f64() > 0.0 {
        total_operations as f64 / total_duration.as_secs_f64()
    } else {
        0.0
    };
    
    let results = BenchmarkResults {
        total_operations,
        successful_operations: success_count,
        failed_operations: failure_count,
        total_duration_ms: total_duration.as_millis(),
        average_latency_ms: average_latency,
        throughput_ops_per_sec: throughput,
        p50_latency_ms: p50_latency,
        p95_latency_ms: p95_latency,
        p99_latency_ms: p99_latency,
    };
    
    // Print results
    println!("\nBenchmark Results:");
    println!("  Total operations: {}", results.total_operations);
    println!("  Successful operations: {}", results.successful_operations);
    println!("  Failed operations: {}", results.failed_operations);
    println!("  Total duration: {} ms", results.total_duration_ms);
    println!("  Average latency: {:.2} ms", results.average_latency_ms);
    println!("  Throughput: {:.2} ops/sec", results.throughput_ops_per_sec);
    println!("  P50 latency: {:.2} ms", results.p50_latency_ms);
    println!("  P95 latency: {:.2} ms", results.p95_latency_ms);
    println!("  P99 latency: {:.2} ms", results.p99_latency_ms);
    
    // Check if we meet our 100ms SLA
    if results.p99_latency_ms > 100.0 {
        println!("\n⚠️ WARNING: P99 latency exceeds our 100ms SLA target!");
    } else {
        println!("\n✅ Performance meets our 100ms SLA target!");
    }
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    run_benchmark().await
}
