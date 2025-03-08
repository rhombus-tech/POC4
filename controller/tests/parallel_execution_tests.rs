use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tokio::time;
use uuid::Uuid;
use futures::future::join_all;

use tee_controller::{HyperTeeController, TeeExecutorPair};
use tee_interface::{ExecutionPayload, ExecutionParams, TeeExecutor, TeeAttestation};

// Maximum acceptable execution time in milliseconds for our "100ms" guarantee
const MAX_EXECUTION_TIME_MS: u64 = 100;

// Test data
const TEST_CONTRACT_CODE: &[u8] = &[0x00, 0x61, 0x73, 0x6D]; // Mock WASM module header
const TEST_METHOD: &str = "add";
const TEST_ARGS: &[u8] = b"1,2";

/// Setup a single TEE controller for testing
async fn setup_single_tee() -> HyperTeeController {
    HyperTeeController::new().await
}

/// Setup a paired TEE controller for testing
async fn setup_tee_pair() -> TeeExecutorPair {
    let primary = Arc::new(RwLock::new(HyperTeeController::new().await));
    let secondary = Arc::new(RwLock::new(HyperTeeController::new().await));
    
    TeeExecutorPair::new(primary, secondary)
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

/// Create a test execution payload
fn create_test_payload(contract_id: &str) -> ExecutionPayload {
    ExecutionPayload {
        params: ExecutionParams {
            id_to: contract_id.to_string(),
            function_call: TEST_METHOD.to_string(),
            detailed_proof: true,
            expected_hash: vec![],
        },
        input: TEST_ARGS.to_vec(),
        operation_id: Some(Uuid::new_v4().to_string()),
        previous_operation_id: None,
        operation_context: None,
    }
}

/// Test 1: Basic single TEE parallel execution
/// This test verifies that a single TEE can handle multiple parallel operations
/// without state conflicts and within our 100ms performance guarantee.
#[tokio::test]
async fn test_single_tee_parallel_execution() -> Result<(), Box<dyn Error>> {
    // Setup
    let tee = Arc::new(setup_single_tee().await);
    
    // Deploy test contract
    let contract_id = tee.deploy_contract(TEST_CONTRACT_CODE, "region_id").await?;
    
    // Create multiple parallel operations (non-conflicting)
    let num_operations = 10;
    let mut futures = Vec::with_capacity(num_operations);
    
    for i in 0..num_operations {
        // Create a unique but deterministic operation ID for each operation
        let operation_id = format!("test-parallel-{}", i);
        
        let contract_id = contract_id.clone();
        let tee_clone = tee.clone();
        
        futures.push(async move {
            let mut payload = create_test_payload(&contract_id);
            payload.operation_id = Some(operation_id);
            
            // Small delay to ensure operations are properly registered
            time::sleep(time::Duration::from_millis(10 * i as u64)).await;
            
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
                            time::sleep(time::Duration::from_millis(50)).await;
                            
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
    
    // Verify all operations succeeded
    for result in results {
        assert!(result.is_ok(), "Operation failed: {:?}", result.err());
    }
    
    Ok(())
}

/// Test 2: State conflicts in a single TEE
/// This test verifies that state consistency is maintained when multiple
/// operations attempt to modify the same state concurrently.
#[tokio::test]
async fn test_single_tee_state_conflicts() -> Result<(), Box<dyn Error>> {
    // Setup
    let tee = Arc::new(setup_single_tee().await);
    
    // Deploy test contract
    let contract_id = tee.deploy_contract(TEST_CONTRACT_CODE, "region_id").await?;
    
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
                    function_call: "store_state".to_string(),
                    detailed_proof: true,
                    expected_hash: vec![],
                },
                // All operations try to write to the same key with different values
                input: format!("same_key,value_{}", i).as_bytes().to_vec(),
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
            function_call: "get_state".to_string(),
            detailed_proof: false,
            expected_hash: vec![],
        },
        input: "same_key".as_bytes().to_vec(),
        operation_id: Some(Uuid::new_v4().to_string()),
        previous_operation_id: None,
        operation_context: None,
    };
    
    let get_result = tee.execute(&read_payload).await?;
    
    // Verify that the final state has one of our expected values
    let final_value = String::from_utf8(get_result.result)?;
    assert!(
        final_value.starts_with("value_"),
        "Final state value should be one of our test values, got: {}",
        final_value
    );
    
    Ok(())
}

/// Test 3: TEE pair parallel execution
/// This test verifies that a pair of TEEs can handle parallel operations
/// with cross-checking of results for regulatory compliance.
#[tokio::test]
async fn test_tee_pair_parallel_execution() -> Result<(), Box<dyn Error>> {
    // Setup
    let tee_pair = Arc::new(setup_tee_pair().await);
    
    // Deploy test contract
    let contract_id = tee_pair.deploy_contract(TEST_CONTRACT_CODE, "default").await?;
    
    // Create multiple parallel operations
    let num_operations = 10;
    let mut futures = Vec::with_capacity(num_operations);
    
    for _ in 0..num_operations {
        let contract_id = contract_id.clone();
        let tee_pair_clone = tee_pair.clone();
        
        futures.push(async move {
            let payload = create_test_payload(&contract_id);
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
            
            // Verify result contains attestations from both TEEs
            if let Ok(execution_result) = &result {
                assert!(
                    execution_result.attestations.len() >= 2,
                    "Expected attestations from both TEEs, got: {}",
                    execution_result.attestations.len()
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
    
    // Deploy test contract
    let contract_id = tee_pair.deploy_contract(TEST_CONTRACT_CODE, "default").await?;
    
    // Create a mix of different operations
    let num_operations = 50;
    let mut futures = Vec::with_capacity(num_operations);
    
    for i in 0..num_operations {
        let operation_type = i % 3; // 0 = add, 1 = store_state, 2 = get_state
        
        let contract_id = contract_id.clone();
        let tee_pair_clone = tee_pair.clone();
        
        futures.push(async move {
            let (function_call, input) = match operation_type {
                0 => {
                    // Simple add uses the "add" method
                    ("add", format!("{},{}", i, i+1).as_bytes().to_vec())
                },
                1 => {
                    // Using the standard wasm execute interface
                    ("execute", format!("store_state,key_{},value_{}", i, i*10).as_bytes().to_vec())
                },
                _ => {
                    // Using the standard wasm execute interface
                    ("execute", format!("get_state,key_{}", i / 2).as_bytes().to_vec())
                }
            };
            
            let payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: contract_id,
                    function_call: function_call.to_string(),
                    detailed_proof: true,
                    expected_hash: vec![],
                },
                input,
                operation_id: Some(Uuid::new_v4().to_string()),
                previous_operation_id: None,
                operation_context: None,
            };
            
            let (duration, result) = measure_execution_time(|| async {
                tee_pair_clone.execute(&payload).await
            }).await;
            
            // Verify execution time
            assert!(
                duration.as_millis() <= MAX_EXECUTION_TIME_MS as u128,
                "High concurrency execution took longer than {}ms: {}ms for operation type {}",
                MAX_EXECUTION_TIME_MS,
                duration.as_millis(),
                operation_type
            );
            
            (operation_type, duration, result)
        });
    }
    
    // Execute all operations in parallel
    let results = join_all(futures).await;
    
    // Analyze results
    let mut add_count = 0;
    let mut store_count = 0;
    let mut get_count = 0;
    let mut total_duration = Duration::new(0, 0);
    
    for (op_type, duration, result) in results {
        // Count by operation type
        match op_type {
            0 => {
                add_count += 1;
                assert!(result.is_ok(), "Add operation failed: {:?}", result.err());
            },
            1 => {
                store_count += 1;
                assert!(result.is_ok(), "Store operation failed: {:?}", result.err());
            },
            _ => {
                get_count += 1;
                // Get operations may return empty results for keys that weren't written yet
                if let Err(e) = &result {
                    println!("Note: Get operation may have failed legitimately: {:?}", e);
                }
            }
        }
        
        total_duration += duration;
    }
    
    // Calculate average execution time
    let avg_ms = total_duration.as_millis() / num_operations as u128;
    println!("Average execution time: {}ms", avg_ms);
    println!("Operation counts - Add: {}, Store: {}, Get: {}", add_count, store_count, get_count);
    
    // Ensure average is well under our 100ms target
    assert!(
        avg_ms <= MAX_EXECUTION_TIME_MS as u128,
        "Average execution time exceeded target: {}ms",
        avg_ms
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
            let tee_pair_clone = tee_pair.clone();
            
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
                                
                                let check_result = tee_pair_clone.execute(&check_payload).await;
                                
                                if let Ok(res) = &check_result {
                                    if res.operation_status == Some("completed".to_string()) {
                                        return (duration, check_result);
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
                            
                            return (duration, tee_pair_clone.execute(&final_check).await);
                        }
                    }
                    result // Return the original result if not a pending operation
                } else {
                    result // Return the original result if it's an error
                };
                
                // Verify execution time
                assert!(
                    duration.as_millis() <= MAX_EXECUTION_TIME_MS as u128,
                    "Execution took longer than {}ms: {}ms for contract {} operation {}",
                    MAX_EXECUTION_TIME_MS,
                    duration.as_millis(),
                    contract_type,
                    op_idx
                );
                
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
    // Setup
    let tee = Arc::new(setup_tee_pair().await);
    
    // Read the contract WASM files
    let simple_multiply_path = "../target/wasm32-unknown-unknown/release/simple_multiply.wasm";
    let token_transfer_path = "../target/wasm32-unknown-unknown/release/token_transfer.wasm";
    let key_value_store_path = "../target/wasm32-unknown-unknown/release/key_value_store.wasm";
    let data_oracle_path = "../target/wasm32-unknown-unknown/release/data_oracle.wasm";
    
    // Get code for each contract
    use std::fs::read;
    let simple_multiply_code = read(simple_multiply_path).expect("Failed to read simple_multiply contract");
    let token_transfer_code = read(token_transfer_path).expect("Failed to read token_transfer contract");
    let key_value_store_code = read(key_value_store_path).expect("Failed to read key_value_store contract");
    let data_oracle_code = read(data_oracle_path).expect("Failed to read data_oracle contract");
    
    // Deploy all contracts
    let simple_multiply_id = tee.deploy_contract(&simple_multiply_code, "simple_multiply").await?;
    let token_transfer_id = tee.deploy_contract(&token_transfer_code, "token_transfer").await?;
    let key_value_store_id = tee.deploy_contract(&key_value_store_code, "key_value_store").await?;
    let data_oracle_id = tee.deploy_contract(&data_oracle_code, "data_oracle").await?;
    
    println!("Deployed contracts: simple_multiply={}, token_transfer={}, key_value_store={}, data_oracle={}", 
             simple_multiply_id, token_transfer_id, key_value_store_id, data_oracle_id);
    
    // Create test payloads for each contract using the standard execute interface
    
    // Store all the contract IDs for future use
    let contract_ids = vec![
        simple_multiply_id.clone(), 
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
        let tee_clone = tee.clone();
        
        futures.push(async move {
            let payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: contract_id,
                    function_call: "execute".to_string(),
                    detailed_proof: true,
                    expected_hash: vec![],
                },
                input,
                operation_id: Some(Uuid::new_v4().to_string()),
                previous_operation_id: None,
                operation_context: None,
            };
            
            tee_clone.execute(&payload).await
        });
    }
    
    let results = join_all(futures).await;
    
    // Verify all operations succeeded
    for (i, result) in results.iter().enumerate() {
        assert!(result.is_ok(), "Operation {} failed: {:?}", i, result.as_ref().err());
        println!("Operation {} result: {:?}", i, result);
    }
    
    // Now verify the results by querying each contract
    
    // Store all the query input parameters
    let query_inputs = vec![
        "multiply,5,10".as_bytes().to_vec(),
        "execute,balance,userB".as_bytes().to_vec(),
        "get,test_key".as_bytes().to_vec(),
        "query_price,BTC".as_bytes().to_vec(),
    ];
    
    // Expected results for each contract
    let expected_results = vec![
        "50".to_string(),
        "50".to_string(),
        "test_value".to_string(),
        "50000".to_string(),
    ];
    
    // Execute all queries in parallel
    let mut query_futures = Vec::with_capacity(4);
    
    for i in 0..4 {
        let contract_id = contract_ids[i].clone();
        let input = query_inputs[i].clone();
        let tee_clone = tee.clone();
        
        query_futures.push(async move {
            let query_payload = ExecutionPayload {
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
            
            tee_clone.execute(&query_payload).await
        });
    }
    
    let query_results = join_all(query_futures).await;
    
    // Verify query results
    for (i, result) in query_results.iter().enumerate() {
        assert!(result.is_ok(), "Query {} failed: {:?}", i, result.as_ref().err());
        println!("Query {} result: {:?}", i, result);
        
        // Check the result matches expected
        let output = String::from_utf8(result.as_ref().unwrap().result.clone()).unwrap();
        assert_eq!(output, expected_results[i], "Unexpected result for query {}", i);
    }
    
    // Also test concurrent execution by running multiple operations on the same contract
    let num_concurrent_ops = 10;
    let mut concurrent_futures = Vec::with_capacity(num_concurrent_ops);
    
    for i in 0..num_concurrent_ops {
        let key = format!("concurrent_key_{}", i);
        let value = format!("concurrent_value_{}", i);
        let contract_id = key_value_store_id.clone();
        let tee_clone = tee.clone();
        
        concurrent_futures.push(async move {
            let payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: contract_id,
                    function_call: "execute".to_string(),
                    detailed_proof: true,
                    expected_hash: vec![],
                },
                input: format!("store,{},{}", key, value).as_bytes().to_vec(),
                operation_id: Some(Uuid::new_v4().to_string()),
                previous_operation_id: None,
                operation_context: None,
            };
            
            tee_clone.execute(&payload).await
        });
    }
    
    let concurrent_results = join_all(concurrent_futures).await;
    
    // Verify all concurrent operations succeeded
    for (i, result) in concurrent_results.iter().enumerate() {
        assert!(result.is_ok(), "Concurrent operation {} failed: {:?}", i, result.as_ref().err());
    }
    
    // Verify concurrent operation results
    let mut verify_futures = Vec::with_capacity(num_concurrent_ops);
    
    for i in 0..num_concurrent_ops {
        let key = format!("concurrent_key_{}", i);
        let expected_value = format!("concurrent_value_{}", i);
        let contract_id = key_value_store_id.clone();
        let tee_clone = tee.clone();
        
        verify_futures.push(async move {
            let query_payload = ExecutionPayload {
                params: ExecutionParams {
                    id_to: contract_id,
                    function_call: "execute".to_string(),
                    detailed_proof: false,
                    expected_hash: vec![],
                },
                input: format!("get,{}", key).as_bytes().to_vec(),
                operation_id: Some(Uuid::new_v4().to_string()),
                previous_operation_id: None,
                operation_context: None,
            };
            
            let result = tee_clone.execute(&query_payload).await?;
            let output = String::from_utf8(result.result)?;
            assert_eq!(output, expected_value, "Key {} has incorrect value", key);
            Ok::<_, Box<dyn Error>>(output)
        });
    }
    
    let verify_results = join_all(verify_futures).await;
    
    // Ensure all verification succeeded
    for (i, result) in verify_results.iter().enumerate() {
        assert!(result.is_ok(), "Verification {} failed: {:?}", i, result.as_ref().err());
    }
    
    Ok(())
}
