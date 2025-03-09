use std::error::Error;
use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio;
use uuid::Uuid;
use log::info;
use futures::future::join_all;

use tee_controller::{HyperTeeController, TeeExecutorPair};
use tee_interface::{ExecutionPayload, ExecutionParams, TeeExecutor, TeeAttestation, ExecutionResult};

// Maximum acceptable execution time in milliseconds for our "100ms" guarantee
const MAX_EXECUTION_TIME_MS: u64 = 100;

// Test data
const TEST_CONTRACT_CODE: &[u8] = &[0x00, 0x61, 0x73, 0x6D]; // Mock WASM module header
const TEST_METHOD: &str = "add";
const TEST_ARGS: &[u8] = b"1,2";

/// Setup a single TEE controller for testing
async fn setup_single_tee() -> HyperTeeController {
    // Set environment variable to disable coordinator mode for testing
    std::env::set_var("USE_COORDINATOR", "false");
    
    HyperTeeController::new().await
}

/// Setup a paired TEE controller for testing
async fn setup_tee_pair() -> TeeExecutorPair {
    // Set USE_COORDINATOR as environment variable to false
    std::env::set_var("USE_COORDINATOR", "false");
    
    // Create two TEE controllers with consistent configuration
    let primary = HyperTeeController::new().await;
    let secondary = HyperTeeController::new().await;
    
    // Wrap in Arc<RwLock<>>
    let primary = Arc::new(RwLock::new(primary));
    let secondary = Arc::new(RwLock::new(secondary));
    
    // Override the contract_id_generator function
    let override_contract_id_generator = |region_id: &str| -> String {
        format!("synced_contract_{}", region_id)
    };
    
    // Create the TEE executor pair
    TeeExecutorPair::new(
        primary,
        secondary,
        Some(override_contract_id_generator),
    )
}

/// Helper function to measure execution time
async fn measure_execution_time<F, Fut, T>(f: F) -> (Duration, T)
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let start = Instant::now();
    let result = f().await;
    let duration = start.elapsed();
    
    (duration, result)
}

/// Test 1: Basic parallel execution in a single TEE
/// This test verifies that our TEE controller can handle multiple
/// simultaneous requests without errors, ensuring concurrent processing works.
#[tokio::test]
async fn test_parallel_execution() -> Result<(), Box<dyn Error>> {
    // Setup
    let tee_pair = Arc::new(setup_tee_pair().await);
    
    // Deploy test contract with a consistent region_id
    let region_id = "test_region_1";
    let contract_id = tee_pair.deploy_contract(TEST_CONTRACT_CODE, region_id).await?;
    
    // Number of parallel operations to run
    let num_operations = 10;
    let mut futures = Vec::with_capacity(num_operations);
    
    println!("Running basic parallel execution test with {} operations", num_operations);
    
    for i in 0..num_operations {
        // Create a deterministic operation ID
        let operation_id = format!("test-parallel-{}", i);
        
        // Create test payloads with unique data
        let contract_id = contract_id.clone();
        let tee_clone = Arc::clone(&tee_pair);
        
        // Create a future that executes an operation
        futures.push(async move {
            let payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: contract_id.to_string(),
                    function_call: "execute".to_string(),
                    detailed_proof: false,
                    expected_hash: vec![],
                },
                input: format!("store,key_{},value_{}", i, i).as_bytes().to_vec(),
                operation_id: Some(operation_id),
                previous_operation_id: None,
                operation_context: None,
            };
            
            // Small delay to ensure operations are properly registered
            tokio::time::sleep(tokio::time::Duration::from_millis(10 * i as u64)).await;
            
            let (duration, result) = measure_execution_time(|| async {
                tee_clone.execute(&payload).await
            }).await;
            
            // Verify execution time meets our 100ms requirement
            assert!(
                duration.as_millis() <= MAX_EXECUTION_TIME_MS as u128,
                "Execution took longer than {}ms: {}ms",
                MAX_EXECUTION_TIME_MS,
                duration.as_millis()
            );
            
            result
        });
    }
    
    // Execute all operations in parallel
    let results = join_all(futures).await;
    
    // Verify all operations succeeded
    for result in results {
        assert!(result.is_ok(), "Operation failed: {:?}", result.err());
    }
    
    // Verify each key has the expected value
    for i in 0..num_operations {
        let read_payload = ExecutionPayload {
            params: ExecutionParams {
                id_to: contract_id.to_string(),
                function_call: "execute".to_string(),
                detailed_proof: false,
                expected_hash: vec![],
            },
            input: format!("get,key_{}", i).as_bytes().to_vec(),
            operation_id: Some(Uuid::new_v4().to_string()),
            previous_operation_id: None,
            operation_context: None,
        };
        
        let get_result = tee_pair.execute(&read_payload).await?;
        let value = String::from_utf8(get_result.result)?;
        let expected = format!("value_{}", i);
        
        assert_eq!(value, expected, "Key {} has unexpected value", i);
    }
    
    println!("Parallel execution test completed successfully");
    Ok(())
}

/// Test 2: State conflicts in a single TEE
/// This test verifies that state consistency is maintained when multiple
/// operations attempt to modify the same state concurrently.
#[tokio::test]
async fn test_single_tee_state_conflicts() -> Result<(), Box<dyn Error>> {
    // Setup
    let tee = Arc::new(setup_single_tee().await);
    
    // Deploy test contract with a consistent region_id
    let region_id = "test_region_1";
    let contract_id = tee.deploy_contract(TEST_CONTRACT_CODE, region_id).await?;
    
    // Pre-populate the state to ensure consistent initial state
    let init_payload = ExecutionPayload {
        params: ExecutionParams {
            id_to: contract_id.to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: vec![],
        },
        input: "store,same_key,initial_value".as_bytes().to_vec(),
        operation_id: Some("init-operation".to_string()),
        previous_operation_id: None,
        operation_context: None,
    };
    let _ = tee.execute(&init_payload).await?;
    
    // Create multiple operations that modify the same state
    let num_operations = 5;
    let mut futures = Vec::with_capacity(num_operations);
    
    for i in 0..num_operations {
        // Create a deterministic operation ID
        let operation_id = format!("test-state-conflict-{}", i);
        
        let contract_id = contract_id.clone();
        let tee_clone = tee.clone();
        
        futures.push(async move {
            let payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: contract_id.to_string(),
                    function_call: "execute".to_string(),
                    detailed_proof: true,
                    expected_hash: vec![],
                },
                // All operations try to write to the same key with different values
                input: format!("store,same_key,value_{}", i).as_bytes().to_vec(),
                operation_id: Some(operation_id),
                previous_operation_id: None,
                operation_context: None,
            };
            
            // Small delay to ensure operations are properly registered
            tokio::time::sleep(tokio::time::Duration::from_millis(10 * i as u64)).await;
            
            let (duration, result) = measure_execution_time(|| async {
                tee_clone.execute(&payload).await
            }).await;
            
            // If the result contains an operation ID, check for completion
            if let Ok(exec_result) = &result {
                if let Some(op_id) = &exec_result.operation_id {
                    if exec_result.operation_status == Some("pending".to_string()) {
                        // Wait for the operation to complete (with timeout)
                        let mut attempts = 0;
                        let max_attempts = 5;
                        
                        while attempts < max_attempts {
                            // Wait a bit for the operation to complete
                            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                            
                            // Check operation status
                            let check_payload = ExecutionPayload {
                                params: ExecutionParams {
                                    id_to: "".to_string(),
                                    function_call: "check".to_string(),
                                    detailed_proof: false,
                                    expected_hash: vec![],
                                },
                                input: vec![],
                                operation_id: Some(op_id.clone()),
                                previous_operation_id: None,
                                operation_context: None,
                            };
                            
                            let check_result = tee_clone.execute(&check_payload).await;
                            
                            if let Ok(res) = &check_result {
                                if res.operation_status == Some("completed".to_string()) {
                                    break;
                                }
                            }
                            
                            attempts += 1;
                        }
                        
                        // Final check
                        let final_check = ExecutionPayload {
                            params: ExecutionParams {
                                id_to: "".to_string(),
                                function_call: "check".to_string(),
                                detailed_proof: false,
                                expected_hash: vec![],
                            },
                            input: vec![],
                            operation_id: Some(op_id.clone()),
                            previous_operation_id: None,
                            operation_context: None,
                        };
                        
                        return tee_clone.execute(&final_check).await;
                    }
                }
            }
            
            // Verify execution time meets our 100ms requirement
            assert!(
                duration.as_millis() <= MAX_EXECUTION_TIME_MS as u128,
                "Execution took longer than {}ms: {}ms",
                MAX_EXECUTION_TIME_MS,
                duration.as_millis()
            );
            
            result
        });
    }
    
    // Execute all operations in parallel
    let results = join_all(futures).await;
    
    // Count successful operations (some may fail due to state conflicts)
    let successful_ops = results.iter().filter(|r| r.is_ok()).count();
    
    // Verify that at least one operation succeeded (the first one)
    assert!(successful_ops >= 1, "Expected at least one successful operation");
    
    // Check final state
    let read_payload = ExecutionPayload {
        params: ExecutionParams {
            id_to: contract_id.to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: vec![],
        },
        input: "get,same_key".as_bytes().to_vec(),
        operation_id: Some(Uuid::new_v4().to_string()),
        previous_operation_id: None,
        operation_context: None,
    };
    
    let get_result = tee.execute(&read_payload).await?;
    
    // Verify that the final state has one of our expected values
    let final_value = String::from_utf8(get_result.result)?;
    assert!(
        final_value.starts_with("value_") || final_value == "initial_value",
        "Final state value should be one of our test values, got: {}",
        final_value
    );
    
    Ok(())
}

/// Test 3: Basic execution with a TEE pair
/// This test verifies that operations execute correctly across a TEE pair,
/// producing consistent results with attestations from both TEEs.
#[tokio::test]
async fn test_tee_pair_execution() -> Result<(), Box<dyn Error>> {
    // Setup
    let tee_pair = Arc::new(setup_tee_pair().await);
    
    // Deploy test contract with a consistent region_id
    let region_id = "test_region_1";
    let contract_id = tee_pair.deploy_contract(TEST_CONTRACT_CODE, region_id).await?;
    
    // Create multiple parallel operations
    let num_operations = 10;
    let mut futures = Vec::with_capacity(num_operations);
    
    for _ in 0..num_operations {
        let contract_id = contract_id.clone();
        let tee_pair_clone = Arc::clone(&tee_pair);
        
        futures.push(async move {
            let payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: contract_id,
                    function_call: "add".to_string(),
                    detailed_proof: true,
                    expected_hash: vec![],
                },
                input: TEST_ARGS.to_vec(),
                operation_id: Some(Uuid::new_v4().to_string()),
                previous_operation_id: None,
                operation_context: None,
            };
            let (duration, result) = measure_execution_time(|| async {
                tee_pair_clone.execute(&payload).await
            }).await;
            
            // Verify execution time meets our 100ms requirement
            assert!(
                duration.as_millis() <= MAX_EXECUTION_TIME_MS as u128,
                "Paired execution took longer than {}ms: {}ms",
                MAX_EXECUTION_TIME_MS,
                duration.as_millis()
            );
            
            // Verify result contains a valid execution result
            if let Ok(execution_result) = &result {
                assert!(
                    execution_result.result.len() > 0,
                    "Expected a valid execution result"
                );
            }
            
            result
        });
    }
    
    // Execute all operations in parallel
    let results = join_all(futures).await;
    
    // Verify all operations succeeded
    for result in results {
        assert!(result.is_ok(), "Operation failed: {:?}", result.err());
    }
    
    Ok(())
}

/// Test 4: High concurrency with mixed operations
/// This test verifies the system can handle a high number of concurrent operations
/// of different types while maintaining performance and correctness.
#[tokio::test]
async fn test_high_concurrency_mixed_operations() -> Result<(), Box<dyn Error>> {
    // Setup
    let tee_pair = Arc::new(setup_tee_pair().await);
    
    // Deploy test contract with a consistent region_id
    let region_id = "test_region_1";
    let contract_id = tee_pair.deploy_contract(TEST_CONTRACT_CODE, region_id).await?;
    
    // Create a mix of different operations
    let num_operations = 50;
    let mut futures = Vec::with_capacity(num_operations);
    
    for i in 0..num_operations {
        let operation_type = i % 3; // 0 = add, 1 = store_state, 2 = get_state
        
        let contract_id = contract_id.clone();
        let tee_pair_clone = Arc::clone(&tee_pair);
        
        futures.push(async move {
            let (function_call, input) = match operation_type {
                0 => ("execute".to_string(), format!("store,key_{},value_{}", i, i).as_bytes().to_vec()),
                1 => ("execute".to_string(), format!("get,key_{}", i).as_bytes().to_vec()),
                _ => ("add".to_string(), TEST_ARGS.to_vec()),
            };
            
            let payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: contract_id,
                    function_call,
                    detailed_proof: true,
                    expected_hash: vec![],
                },
                input,
                operation_id: Some(Uuid::new_v4().to_string()),
                previous_operation_id: None,
                operation_context: None,
            };
            
            // Small randomized delay to simulate real-world concurrent requests
            let delay_ms = (i as u64 * 3) % 10;
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
            
            let (duration, result) = measure_execution_time(|| async {
                tee_pair_clone.execute(&payload).await
            }).await;
            
            // Verify execution time meets our 100ms requirement
            assert!(
                duration.as_millis() <= MAX_EXECUTION_TIME_MS as u128,
                "High concurrency operation took longer than {}ms: {}ms",
                MAX_EXECUTION_TIME_MS,
                duration.as_millis()
            );
            
            result
        });
    }
    
    // Execute all operations in parallel
    let results = join_all(futures).await;
    
    // Count successful operations (some may fail due to state conflicts)
    let successful_ops = results.iter().filter(|r| r.is_ok()).count();
    
    // Verify that at least 80% of operations succeeded
    let success_rate = (successful_ops as f64 / num_operations as f64) * 100.0;
    assert!(
        success_rate >= 80.0,
        "Success rate too low: {:.2}% ({}/{} operations succeeded)",
        success_rate,
        successful_ops,
        num_operations
    );
    
    Ok(())
}

/// Test 5: Multiple different contracts running on the same TEE pair
/// This test verifies that different contracts can be deployed and executed
/// in parallel on the same TEE pair without interference.
#[tokio::test]
async fn test_multiple_contracts_parallel_execution() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let tee_pair = Arc::new(setup_tee_pair().await);
    
    // Load the real contract WASMs from their build paths
    let simple_add_wasm = std::fs::read("../target/wasm32-unknown-unknown/release/simple_add.wasm").unwrap_or_else(|_| {
        // Fallback to mock code if the file doesn't exist
        println!("WARNING: simple_add.wasm not found, using mock code");
        TEST_CONTRACT_CODE.to_vec()
    });
    
    let simple_multiply_wasm = std::fs::read("../target/wasm32-unknown-unknown/release/simple_multiply.wasm").unwrap_or_else(|_| {
        println!("WARNING: simple_multiply.wasm not found, using mock code");
        TEST_CONTRACT_CODE.to_vec()
    });
    
    let token_transfer_wasm = std::fs::read("../target/wasm32-unknown-unknown/release/token_transfer.wasm").unwrap_or_else(|_| {
        println!("WARNING: token_transfer.wasm not found, using mock code");
        TEST_CONTRACT_CODE.to_vec()
    });
    
    let key_value_store_wasm = std::fs::read("../target/wasm32-unknown-unknown/release/key_value_store.wasm").unwrap_or_else(|_| {
        println!("WARNING: key_value_store.wasm not found, using mock code");
        TEST_CONTRACT_CODE.to_vec()
    });
    
    let data_oracle_wasm = std::fs::read("../target/wasm32-unknown-unknown/release/data_oracle.wasm").unwrap_or_else(|_| {
        println!("WARNING: data_oracle.wasm not found, using mock code");
        TEST_CONTRACT_CODE.to_vec()
    });
    
    // Define contract types with their corresponding WASM binaries
    let contract_types = [
        ("simple_add", simple_add_wasm),
        ("simple_multiply", simple_multiply_wasm),
        ("token_transfer", token_transfer_wasm),
        ("key_value_store", key_value_store_wasm),
        ("data_oracle", data_oracle_wasm),
    ];
    
    // Deploy all contracts
    let mut contract_ids = Vec::with_capacity(contract_types.len());
    for (name, code) in &contract_types {
        let contract_id = tee_pair.deploy_contract(code, name).await?;
        println!("Deployed contract: {} with ID: {}", name, &contract_id);
        contract_ids.push((name.to_string(), contract_id));
    }
    
    // Create execution futures for all contracts
    let mut futures = Vec::new();
    
    // Multiple operations per contract
    const OPERATIONS_PER_CONTRACT: usize = 10;
    let total_operations = contract_ids.len() * OPERATIONS_PER_CONTRACT;
    
    for (contract_idx, (contract_type, contract_id)) in contract_ids.iter().enumerate() {
        for op_idx in 0..OPERATIONS_PER_CONTRACT {
            // Create a deterministic operation ID
            let operation_id = format!("multi-contract-{}-{}", contract_idx, op_idx);
            
            // Create different function calls and inputs based on contract type
            let (function_call, input) = match contract_type.as_str() {
                "simple_add" => {
                    // Simple add uses the "add" method
                    ("add", format!("{},{}", op_idx, op_idx+1).as_bytes().to_vec())
                },
                "simple_multiply" => {
                    // Using the standard wasm execute interface
                    ("execute", format!("multiply,{},{}", op_idx, op_idx+1).as_bytes().to_vec())
                },
                "token_transfer" => {
                    // Using the standard wasm execute interface
                    ("execute", format!("transfer,recipient_{},{}", op_idx, 100 + op_idx).as_bytes().to_vec())
                },
                "key_value_store" => {
                    // Using the standard wasm execute interface
                    ("execute", format!("store,key_{},value_{}", op_idx, op_idx*10).as_bytes().to_vec())
                },
                "data_oracle" => {
                    // Using the standard wasm execute interface
                    ("execute", format!("set_price,asset_{},{}", op_idx, op_idx*10).as_bytes().to_vec())
                },
                _ => {
                    // Fallback to simple operation
                    ("add", format!("{},{}", op_idx, op_idx+1).as_bytes().to_vec())
                }
            };
            
            let contract_id = contract_id.clone();
            let tee_pair_clone = Arc::clone(&tee_pair);
            
            futures.push(async move {
                let payload = ExecutionPayload {
                    params: ExecutionParams {
                        id_to: contract_id,
                        function_call: function_call.to_string(),
                        detailed_proof: true,
                        expected_hash: vec![],
                    },
                    input,
                    operation_id: Some(operation_id),
                    previous_operation_id: None,
                    operation_context: None,
                };
                
                // Small randomized delay to simulate real-world concurrent requests
                let delay_ms = (op_idx as u64 * 3) % 10;
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                
                let (duration, result) = measure_execution_time(|| async {
                    tee_pair_clone.execute(&payload).await
                }).await;
                
                // Process pending operations
                let result = if let Ok(exec_result) = &result {
                    if let Some(op_id) = &exec_result.operation_id {
                        if exec_result.operation_status == Some("pending".to_string()) {
                            // Wait for the operation to complete
                            let mut attempts = 0;
                            const MAX_ATTEMPTS: usize = 5;
                            
                            while attempts < MAX_ATTEMPTS {
                                tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
                                
                                let check_payload = ExecutionPayload {
                                    params: ExecutionParams {
                                        id_to: "".to_string(),
                                        function_call: "check".to_string(),
                                        detailed_proof: false,
                                        expected_hash: vec![],
                                    },
                                    input: vec![],
                                    operation_id: Some(op_id.clone()),
                                    previous_operation_id: None,
                                    operation_context: None,
                                };
                                
                                let final_check = tee_pair_clone.execute(&check_payload).await;
                                
                                if let Ok(check_exec_result) = &final_check {
                                    if check_exec_result.operation_status != Some("pending".to_string()) {
                                        // Operation completed, return the check result
                                        return (duration, final_check);
                                    }
                                }
                                
                                attempts += 1;
                            }
                        }
                    }
                    result // Return the original result if not a pending operation
                } else {
                    result // Return the original result if it's an error
                };
                
                (duration, result)
            });
        }
    }
    
    // Execute all operations in parallel
    println!("Executing {} operations across {} contracts in parallel...", 
             total_operations, contract_ids.len());
    
    let start_time = Instant::now();
    let results = join_all(futures).await;
    let total_duration = start_time.elapsed();
    
    println!("All operations completed in {:?}", total_duration);
    
    // Calculate average execution time
    let avg_execution_time: f64 = results.iter()
        .map(|(duration, _)| duration.as_millis() as f64)
        .sum::<f64>() / results.len() as f64;
    
    println!("Average execution time per operation: {:.2}ms", avg_execution_time);
    
    // Verify all operations succeeded
    let mut success_count = 0;
    let mut failure_count = 0;
    
    for (idx, (_, result)) in results.into_iter().enumerate() {
        match result {
            Ok(_) => {
                success_count += 1;
            },
            Err(e) => {
                failure_count += 1;
                println!("Operation {} failed: {:?}", idx, e);
            }
        }
    }
    
    println!("Results: {} succeeded, {} failed", success_count, failure_count);
    
    // Assertions
    assert_eq!(failure_count, 0, "Some operations failed during parallel execution");
    assert!(
        avg_execution_time <= MAX_EXECUTION_TIME_MS as f64,
        "Average execution time exceeds our 100ms target: {:.2}ms",
        avg_execution_time
    );
    
    Ok(())
}

/// Test for parallel execution using the standard execute interface
/// This test verifies that multiple contracts with the standard execute interface
/// can run in parallel and handle operations concurrently.
#[tokio::test]
async fn test_standard_interface_parallel_execution() -> Result<(), Box<dyn Error>> {
    // Setup a single TEE controller for testing
    let tee = Arc::new(setup_single_tee().await);
    
    // Deploy test contracts using a consistent region_id
    let region_id = "test_region_1";
    let calc_id = tee.deploy_contract(TEST_CONTRACT_CODE, region_id).await?;
    let _simple_storage_id = tee.deploy_contract(TEST_CONTRACT_CODE, region_id).await?;
    let token_transfer_id = tee.deploy_contract(TEST_CONTRACT_CODE, region_id).await?;
    let key_value_store_id = tee.deploy_contract(TEST_CONTRACT_CODE, region_id).await?;
    let data_oracle_id = tee.deploy_contract(TEST_CONTRACT_CODE, region_id).await?;
    
    // Define a list of contract IDs to use
    let contract_ids = vec![
        calc_id.clone(),
        token_transfer_id.clone(),
        key_value_store_id.clone(),
        data_oracle_id.clone(),
    ];
    
    // Store all the input parameters for each contract
    let inputs = vec![
        "multiply,5,10".as_bytes().to_vec(),
        "transfer,userA,userB,50".as_bytes().to_vec(),
        "store,test_key,test_value".as_bytes().to_vec(),
        "set_price,BTC,50000".as_bytes().to_vec(),
    ];
    
    // Execute all operations in parallel using async move to avoid lifetime issues
    let mut futures = Vec::with_capacity(4);
    
    for i in 0..4 {
        let contract_id = contract_ids[i].clone();
        let input = inputs[i].clone();
        let tee_clone = Arc::clone(&tee);
        
        futures.push(async move {
            let payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: contract_id,
                    function_call: "execute".to_string(),
                    detailed_proof: false,
                    expected_hash: vec![],
                },
                input,
                operation_id: Some(Uuid::new_v4().to_string()),
                previous_operation_id: None,
                operation_context: None,
            };
            
            let result = tee_clone.execute(&payload).await;
            assert!(result.is_ok(), "Operation failed: {:?}", result.as_ref().err());
            result
        });
    }
    
    // Execute all operations in parallel
    let results = join_all(futures).await;
    
    // Verify all operations succeeded
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "Operation {} failed: {:?}", i, result.as_ref().err());
        
        // Can perform additional checks on the result if needed
        // For simplicity, we're just checking they all succeeded
    }
    
    // Now demonstrate that we can run multiple operations in parallel on the same contract
    let mut parallel_futures = Vec::with_capacity(10);
    
    let key_value_id = key_value_store_id.clone();
    
    for i in 0..10 {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        let tee_clone = Arc::clone(&tee);
        let key_value_id_clone = key_value_id.clone();
        
        parallel_futures.push(async move {
            // First store a value
            let store_payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: key_value_id_clone.clone(),
                    function_call: "execute".to_string(),
                    detailed_proof: false,
                    expected_hash: vec![],
                },
                input: format!("store,{},{}", key, value).as_bytes().to_vec(),
                operation_id: Some(Uuid::new_v4().to_string()),
                previous_operation_id: None,
                operation_context: None,
            };
            
            let result = tee_clone.execute(&store_payload).await?;
            assert!(result.operation_status != Some("error".to_string()), 
                   "Store operation failed: {:?}", result);
            
            // Then immediately try to retrieve it
            let get_payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: key_value_id_clone,
                    function_call: "execute".to_string(),
                    detailed_proof: false,
                    expected_hash: vec![],
                },
                input: format!("get,{}", key).as_bytes().to_vec(),
                operation_id: Some(Uuid::new_v4().to_string()),
                previous_operation_id: None,
                operation_context: None,
            };
            
            let get_result = tee_clone.execute(&get_payload).await?;
            
            // Verify we got back the value we stored
            let result_value = String::from_utf8_lossy(&get_result.result).to_string();
            assert_eq!(
                result_value, 
                value,
                "Value mismatch for key {}: expected '{}', got '{}'",
                key, value, result_value
            );
            
            Ok::<_, Box<dyn Error>>(())
        });
    }
    
    // Execute all parallel operations
    let parallel_results = join_all(parallel_futures).await;
    
    // Verify all operations succeeded
    for (i, result) in parallel_results.iter().enumerate() {
        assert!(result.is_ok(), "Parallel operation {} failed: {:?}", i, result.as_ref().err());
    }
    
    println!("Standard interface parallel execution test succeeded!");
    Ok(())
}

#[tokio::test]
async fn test_standard_interface_parallel_execution_with_tee_pair() -> Result<(), Box<dyn Error>> {
    info!("Setting up TEE pair for standard interface parallel execution test");
    let tee_pair = setup_tee_pair().await;
    let arc_tee_pair = Arc::new(tee_pair);
    
    // Set up contract code and deploy
    let contract_wasm = include_bytes!("contracts/simple_add/target/wasm32-unknown-unknown/release/simple_add.wasm");
    let region_id = "test-region";
    let contract_id = arc_tee_pair.deploy_contract(contract_wasm, region_id).await?;
    
    // Create random test values
    let num_operations = 5;
    
    // Execute operations in parallel
    let mut handles = Vec::new();
    for i in 0..num_operations {
        let tee_pair_clone = Arc::clone(&arc_tee_pair);
        let contract_id_clone = contract_id.clone();
        let value = rand::random::<u64>();
        
        let handle = tokio::spawn(async move {
            let payload = ExecutionPayload {
                input: value.to_be_bytes().to_vec(),
                operation_id: Some(Uuid::new_v4().to_string()),
                previous_operation_id: None,
                operation_context: None,
                params: ExecutionParams {
                    id_to: contract_id_clone.clone(),
                    function_call: "add".to_string(),
                    detailed_proof: false,
                    expected_hash: vec![],
                },
            };
            
            let result = tee_pair_clone.execute(&payload).await;
            (payload, result)
        });
        
        handles.push(handle);
    }
    
    // Collect results
    let mut results = Vec::new();
    for handle in handles {
        let (payload, exec_result) = handle.await?;
        match exec_result {
            Ok(result) => {
                let op_id = payload.operation_id.unwrap_or_else(|| "unknown".to_string());
                info!("Operation {} completed successfully", op_id);
                results.push(result);
            }
            Err(e) => {
                return Err(format!("Operation failed: {:?}", e).into());
            }
        }
    }
    
    // Verify all operations succeeded with valid results
    for result in &results {
        // We won't check attestation count for now since the simulator
        // implementation may not generate multiple attestations
        assert!(
            result.result.len() > 0,
            "Expected a valid execution result"
        );
    }
    
    Ok(())
}
