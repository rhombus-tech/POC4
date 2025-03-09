use tee_controller::HyperTeeController;
use tee_interface::{ExecutionPayload, ExecutionParams, TeeExecutor};
use std::env;
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set up environment variables for testing
    env::set_var("USE_COORDINATOR", "true");
    env::set_var("COORDINATOR_URL", "http://localhost:8080");
    
    println!("Creating HyperTeeController with coordinator integration...");
    let controller = HyperTeeController::new().await;
    
    // Initialize and register with coordinator
    println!("Initializing coordinator...");
    controller.initialize_coordinator().await
        .map_err(|e| format!("Failed to initialize coordinator: {}", e))?;
    
    // Get available workers
    println!("Getting available workers...");
    let workers = controller.get_available_workers().await
        .map_err(|e| format!("Failed to get workers: {}", e))?;
    println!("Available workers: {:?}", workers);
    
    if !workers.is_empty() {
        // Register a TEE pair with the first available worker
        println!("Registering TEE pair with worker: {}", workers[0]);
        controller.register_tee_pair("default", &workers[0]).await
            .map_err(|e| format!("Failed to register TEE pair: {}", e))?;
            
        // Create a test payload
        let payload = ExecutionPayload {
            input: b"get,test_key".to_vec(),
            params: ExecutionParams {
                id_to: "test-contract".to_string(),
                function_call: "execute".to_string(),
                detailed_proof: false,
                expected_hash: Vec::new(),
            },
            operation_id: None,
            previous_operation_id: None,
            operation_context: None,
        };
        
        // Execute the payload
        println!("Executing payload...");
        let result = controller.execute(&payload).await
            .map_err(|e| format!("Execution failed: {}", e))?;
            
        println!("Execution result: {:?}", result);
    } else {
        println!("No workers available for testing");
    }
    
    println!("Test completed successfully");
    Ok(())
}
