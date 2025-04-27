use std::env;
use std::error::Error;
use std::time::Duration;
use tokio;
use tee_controller::HyperTeeController;
use tee_interface::{ExecutionPayload, ExecutionParams, TeeExecutor};

// The multi_tee_integration tool demonstrates how to use multiple TEE pairs
// with the coordinator to execute contracts in a distributed environment.

async fn run_primary_controller(coordinator_url: &str) -> Result<(), Box<dyn Error>> {
    // Set up environment variables for the primary controller
    env::set_var("COORDINATOR_URL", coordinator_url);
    env::set_var("USE_COORDINATOR", "true");
    
    // Create the primary controller
    println!("Starting primary controller...");
    let controller = HyperTeeController::new().await;
    
    // Initialize and register with the coordinator
    println!("Registering primary controller with coordinator...");
    controller.initialize_coordinator().await
        .map_err(|e| format!("Failed to initialize: {}", e))?;
    
    // Wait for secondary controller to register
    println!("Waiting for secondary controller to register...");
    let mut attempts = 0;
    let mut workers = Vec::new();
    
    while attempts < 30 {
        match controller.get_available_workers().await {
            Ok(available_workers) => {
                if available_workers.len() > 1 {
                    // Found at least one other worker besides ourselves
                    workers = available_workers;
                    break;
                }
            }
            Err(e) => {
                println!("Error getting workers: {}, retrying...", e);
            }
        }
        
        tokio::time::sleep(Duration::from_secs(1)).await;
        attempts += 1;
    }
    
    if workers.len() <= 1 {
        return Err("Timed out waiting for secondary controller to register".into());
    }
    
    println!("Available workers: {:?}", workers);
    
    // Just take the first available worker as secondary
    if workers.is_empty() {
        return Err("No workers available to pair with".into());
    }
    
    let secondary_worker_id = workers[0].clone();
    println!("Selected secondary worker: {}", secondary_worker_id);
    
    // Register a TEE pair with the secondary worker
    println!("Registering TEE pair with secondary worker: {}", secondary_worker_id);
    controller.register_tee_pair("default", &secondary_worker_id).await
        .map_err(|e| format!("Failed to register TEE pair: {}", e))?;
    
    // Deploy a test contract
    println!("Deploying test contract...");
    let contract_wasm = b"mock contract binary".to_vec();
    let contract_id = controller.deploy_contract(&contract_wasm, "default").await
        .map_err(|e| format!("Failed to deploy contract: {}", e))?;
    
    println!("Contract deployed with ID: {}", contract_id);
    
    // Execute operations on the contract through the TEE pair
    // Store a value
    let store_payload = ExecutionPayload {
        input: b"store,hello,world".to_vec(),
        params: ExecutionParams {
            id_to: contract_id.clone(),
            function_call: "execute".to_string(),
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
    
    println!("Executing store operation...");
    let store_result = controller.execute(&store_payload).await
        .map_err(|e| format!("Failed to execute store: {}", e))?;
    
    println!("Store result: {:?}", String::from_utf8_lossy(&store_result.result));
    
    // Get the stored value
    let get_payload = ExecutionPayload {
        input: b"get,hello".to_vec(),
        params: ExecutionParams {
            id_to: contract_id,
            function_call: "execute".to_string(),
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
    
    println!("Executing get operation...");
    let get_result = controller.execute(&get_payload).await
        .map_err(|e| format!("Failed to execute get: {}", e))?;
    
    println!("Get result: {:?}", String::from_utf8_lossy(&get_result.result));
    
    // Verify the result
    assert_eq!(&get_result.result, b"world", "Value was not stored correctly");
    
    println!("Integration test completed successfully!");
    Ok(())
}

async fn run_secondary_controller(coordinator_url: &str) -> Result<(), Box<dyn Error>> {
    // Set up environment variables for the secondary controller
    env::set_var("COORDINATOR_URL", coordinator_url);
    env::set_var("USE_COORDINATOR", "true");
    
    // Create the secondary controller
    println!("Starting secondary controller...");
    let controller = HyperTeeController::new().await;
    
    // Initialize and register with the coordinator
    println!("Registering secondary controller with coordinator...");
    controller.initialize_coordinator().await
        .map_err(|e| format!("Failed to initialize: {}", e))?;
    
    // Secondary controller just waits to be paired and receive tasks
    println!("Secondary controller initialized");
    println!("Waiting for tasks from coordinator...");
    
    // Keep the controller alive
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Default coordinator URL
    let coordinator_url = env::var("COORDINATOR_URL")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    
    // Determine whether to run as primary or secondary
    let run_mode = env::var("RUN_MODE")
        .unwrap_or_else(|_| "primary".to_string());
    
    match run_mode.as_str() {
        "primary" => run_primary_controller(&coordinator_url).await,
        "secondary" => run_secondary_controller(&coordinator_url).await,
        _ => Err("Invalid RUN_MODE. Use 'primary' or 'secondary'".into()),
    }
}
