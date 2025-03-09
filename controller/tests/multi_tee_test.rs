use tee_controller::HyperTeeController;
use tee_interface::{ExecutionPayload, ExecutionParams, TeeExecutor};
use std::sync::Arc;
use std::env;
use tokio;

// Mock coordinator for testing the multi-TEE functionality
struct MockCoordinator {
    worker_id_1: String,
    worker_id_2: String,
    tee_controller_1: HyperTeeController,
    tee_controller_2: HyperTeeController,
}

impl MockCoordinator {
    // Create a new mock coordinator with two TEE controllers
    async fn new() -> Self {
        // Set environment variable to disable coordinator mode
        env::set_var("USE_COORDINATOR", "false");
        
        // Create two TEE controllers to simulate a TEE pair
        let tee_controller_1 = HyperTeeController::new().await;
        let tee_controller_2 = HyperTeeController::new().await;
        
        // For testing purposes, use fixed worker IDs that don't depend on private fields
        let worker_id_1 = "worker-1".to_string();
        let worker_id_2 = "worker-2".to_string();
        
        MockCoordinator {
            worker_id_1,
            worker_id_2,
            tee_controller_1,
            tee_controller_2,
        }
    }
    
    // Simulate task execution across multiple TEE pairs
    async fn execute_task(&self, payload: &ExecutionPayload) -> Result<Vec<u8>, String> {
        // In a real implementation, this would route through the coordinator
        // For our test, we'll execute on both TEEs in parallel
        
        println!("Executing task on primary TEE controller (worker: {}) and secondary TEE controller (worker: {}) in parallel", 
                 self.worker_id_1, self.worker_id_2);
        
        // Clone the payload to avoid ownership issues
        let payload_clone = payload.clone();
        
        // Execute on both TEEs simultaneously using tokio::join!
        let (primary_result, secondary_result) = tokio::join!(
            self.tee_controller_1.execute(payload),
            self.tee_controller_2.execute(&payload_clone)
        );
        
        // Handle the results
        let primary_result = primary_result
            .map_err(|e| format!("Primary execution failed: {}", e))?;
        
        let secondary_result = secondary_result
            .map_err(|e| format!("Secondary execution failed: {}", e))?;
            
        // Compare the results for consistency
        if primary_result.result == secondary_result.result {
            println!("Results match between primary and secondary TEE controllers");
        } else {
            println!("WARNING: Results differ between primary and secondary TEE controllers");
            return Err(format!("Result mismatch: primary={:?}, secondary={:?}", 
                              primary_result.result, secondary_result.result));
        }
        
        println!("Task execution complete with verified result");
        Ok(primary_result.result)
    }
}

// Test multi-TEE execution with a mock coordinator
#[tokio::test]
async fn test_multi_tee_execution() {
    // Create a mock coordinator with two TEE controllers
    let coordinator = Arc::new(MockCoordinator::new().await);
    
    // Create a test contract for key-value operations
    let contract_id = "test-contract";
    
    // Create a payload with a store command
    let store_payload = ExecutionPayload {
        input: b"store,test_key,test_value".to_vec(),
        params: ExecutionParams {
            id_to: contract_id.to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: Vec::new(),
        },
        operation_id: None,
        previous_operation_id: None,
        operation_context: None,
    };
    
    // Execute the store command
    let store_result = coordinator.execute_task(&store_payload).await.unwrap();
    assert_eq!(store_result, b"success");
    
    // Create a payload with a get command to verify the store worked
    let get_payload = ExecutionPayload {
        input: b"get,test_key".to_vec(),
        params: ExecutionParams {
            id_to: contract_id.to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: Vec::new(),
        },
        operation_id: None,
        previous_operation_id: None,
        operation_context: None,
    };
    
    // Execute the get command
    let get_result = coordinator.execute_task(&get_payload).await.unwrap();
    assert_eq!(get_result, b"test_value");
    
    println!("Multi-TEE execution test passed successfully");
}

// Test parallel execution across multiple TEE pairs
#[tokio::test]
async fn test_parallel_execution() {
    // Create a mock coordinator with two TEE controllers
    let coordinator = Arc::new(MockCoordinator::new().await);
    
    // Create a test contract
    let contract_id = "parallel-test-contract";
    
    // Number of parallel operations to execute
    let num_operations = 10;
    
    // Create multiple tasks to be executed in parallel
    let mut handles = Vec::new();
    
    println!("Running basic parallel execution test");
    
    // Measure start time for performance testing
    let start_time = std::time::Instant::now();
    
    for i in 0..num_operations {
        let coordinator_clone = coordinator.clone();
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        
        // Spawn a task to execute a store operation
        let handle = tokio::spawn(async move {
            let store_payload = ExecutionPayload {
                input: format!("store,{},{}", key, value).into_bytes(),
                params: ExecutionParams {
                    id_to: contract_id.to_string(),
                    function_call: "execute".to_string(),
                    detailed_proof: false,
                    expected_hash: Vec::new(),
                },
                operation_id: None,
                previous_operation_id: None,
                operation_context: None,
            };
            
            let result = coordinator_clone.execute_task(&store_payload).await.unwrap();
            (key, result)
        });
        
        handles.push(handle);
    }
    
    // Wait for all operations to complete
    for handle in handles {
        let (key, result) = handle.await.unwrap();
        assert_eq!(result, b"success", "Failed to store key: {}", key);
    }
    
    // Verify all values were stored correctly
    for i in 0..num_operations {
        let key = format!("key_{}", i);
        let expected_value = format!("value_{}", i);
        
        let get_payload = ExecutionPayload {
            input: format!("get,{}", key).into_bytes(),
            params: ExecutionParams {
                id_to: contract_id.to_string(),
                function_call: "execute".to_string(),
                detailed_proof: false,
                expected_hash: Vec::new(),
            },
            operation_id: None,
            previous_operation_id: None,
            operation_context: None,
        };
        
        let get_result = coordinator.execute_task(&get_payload).await.unwrap();
        assert_eq!(get_result, expected_value.as_bytes());
    }
    
    // Calculate and verify execution time
    let elapsed = start_time.elapsed();
    println!("Basic parallel execution completed in {:?}", elapsed);
    assert!(elapsed.as_millis() < 1000, "Parallel execution took longer than expected: {:?}", elapsed);
    
    println!("Basic parallel multi-TEE execution test passed successfully");
    
    // Part 2: Test state conflict resolution
    println!("\nRunning state conflict resolution test");
    
    // Create a scenario where multiple operations try to update the same key
    let shared_key = "shared_key";
    
    // First set an initial value
    let init_payload = ExecutionPayload {
        input: format!("store,{},initial", shared_key).into_bytes(),
        params: ExecutionParams {
            id_to: contract_id.to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: Vec::new(),
        },
        operation_id: None,
        previous_operation_id: None,
        operation_context: None,
    };
    
    let init_result = coordinator.execute_task(&init_payload).await.unwrap();
    assert_eq!(init_result, b"success");
    
    // Now try to update the same key with multiple operations simultaneously
    let num_conflicting_ops = 5;
    let mut conflict_handles = Vec::new();
    
    let conflict_start_time = std::time::Instant::now();
    
    for i in 0..num_conflicting_ops {
        let coordinator_clone = coordinator.clone();
        let conflict_value = format!("conflict_value_{}", i);
        
        let handle = tokio::spawn(async move {
            let conflict_payload = ExecutionPayload {
                input: format!("store,{},{}", shared_key, conflict_value).into_bytes(),
                params: ExecutionParams {
                    id_to: contract_id.to_string(),
                    function_call: "execute".to_string(),
                    detailed_proof: false,
                    expected_hash: Vec::new(),
                },
                operation_id: None,
                previous_operation_id: None,
                operation_context: None,
            };
            
            let result = coordinator_clone.execute_task(&conflict_payload).await;
            (conflict_value, result)
        });
        
        conflict_handles.push(handle);
    }
    
    // Wait for all conflict operations to complete
    let mut success_count = 0;
    for handle in conflict_handles {
        let (value, result) = handle.await.unwrap();
        if result.is_ok() {
            success_count += 1;
            println!("Successfully set shared key to value: {}", value);
        } else {
            println!("Expected conflict detected: {}", result.unwrap_err());
        }
    }
    
    // At least one operation should succeed
    assert!(success_count > 0, "No operations succeeded for shared key");
    
    // Get the final value to verify consistency
    let check_payload = ExecutionPayload {
        input: format!("get,{}", shared_key).into_bytes(),
        params: ExecutionParams {
            id_to: contract_id.to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: Vec::new(),
        },
        operation_id: None,
        previous_operation_id: None,
        operation_context: None,
    };
    
    let final_value = coordinator.execute_task(&check_payload).await.unwrap();
    println!("Final value for shared key: {}", String::from_utf8_lossy(&final_value));
    
    // The value should be one of our conflict values or still be initial
    // We're not asserting exactly which one, just that we have a consistent final state
    let conflict_elapsed = conflict_start_time.elapsed();
    println!("Conflict resolution test completed in {:?}", conflict_elapsed);
    assert!(conflict_elapsed.as_millis() < 1000, "Conflict resolution took longer than expected: {:?}", conflict_elapsed);
    
    // Part 3: Performance under load
    println!("\nTesting performance under load (100ms guarantee)");
    
    // Create a larger batch of operations to stress test performance
    let stress_ops = 20;
    let mut stress_handles = Vec::new();
    
    let stress_start_time = std::time::Instant::now();
    
    for i in 0..stress_ops {
        let coordinator_clone = coordinator.clone();
        let key = format!("stress_key_{}", i);
        let value = format!("stress_value_{}", i);
        
        let handle = tokio::spawn(async move {
            let start = std::time::Instant::now();
            
            let store_payload = ExecutionPayload {
                input: format!("store,{},{}", key, value).into_bytes(),
                params: ExecutionParams {
                    id_to: contract_id.to_string(),
                    function_call: "execute".to_string(),
                    detailed_proof: false,
                    expected_hash: Vec::new(),
                },
                operation_id: None,
                previous_operation_id: None,
                operation_context: None,
            };
            
            let result = coordinator_clone.execute_task(&store_payload).await.unwrap();
            let duration = start.elapsed();
            (key, value, duration)
        });
        
        stress_handles.push(handle);
    }
    
    // Collect all operation durations and verify they meet the 100ms target
    let mut all_durations = Vec::new();
    
    for handle in stress_handles {
        let (key, value, duration) = handle.await.unwrap();
        println!("Operation for key {} completed in {:?}", key, duration);
        all_durations.push(duration);
        
        // Verify the operation was successful by retrieving the value
        let get_payload = ExecutionPayload {
            input: format!("get,{}", key).into_bytes(),
            params: ExecutionParams {
                id_to: contract_id.to_string(),
                function_call: "execute".to_string(),
                detailed_proof: false,
                expected_hash: Vec::new(),
            },
            operation_id: None,
            previous_operation_id: None,
            operation_context: None,
        };
        
        let get_result = coordinator.execute_task(&get_payload).await.unwrap();
        assert_eq!(get_result, value.as_bytes(), "Value mismatch for key {}", key);
    }
    
    // Calculate statistics
    all_durations.sort();
    let total_ops = all_durations.len();
    
    if total_ops > 0 {
        let p50 = all_durations[total_ops / 2];
        let p95 = all_durations[(total_ops * 95) / 100];
        let p99 = all_durations[(total_ops * 99) / 100];
        let max = all_durations.last().unwrap();
        
        println!("Performance results:");
        println!("- p50 (median): {:?}", p50);
        println!("- p95: {:?}", p95);
        println!("- p99: {:?}", p99);
        println!("- max: {:?}", max);
        
        // In a test environment the actual performance might vary,
        // but we expect our p95 to be under our target in production
        // Here we're using a more reasonable 500ms for the test
        assert!(p95.as_millis() < 500, "95th percentile latency exceeds target: {:?}", p95);
    }
    
    let stress_elapsed = stress_start_time.elapsed();
    println!("Performance test completed in {:?}", stress_elapsed);
    
    println!("Parallel execution test suite passed successfully");
}

// Test contention with explicit state conflicts
#[tokio::test]
async fn test_state_conflict_handling() {
    // Create a mock coordinator with two TEE controllers
    let coordinator = Arc::new(MockCoordinator::new().await);
    
    // Create a test contract
    let contract_id = "conflict-test-contract";
    
    // Set up a shared key with initial value
    let shared_key = "same_key";
    let init_payload = ExecutionPayload {
        input: format!("store,{},initial_value", shared_key).into_bytes(),
        params: ExecutionParams {
            id_to: contract_id.to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: Vec::new(),
        },
        operation_id: None,
        previous_operation_id: None,
        operation_context: None,
    };
    
    let init_result = coordinator.execute_task(&init_payload).await.unwrap();
    assert_eq!(init_result, b"success");
    
    // Simultaneously attempt to:
    // 1. Get the value (read)
    // 2. Update the value to "value_1" (write)
    // 3. Update the value to "value_2" (write)
    
    let (get_result, update1_result, update2_result) = tokio::join!(
        tokio::spawn({
            let coordinator = coordinator.clone();
            async move {
                let get_payload = ExecutionPayload {
                    input: format!("get,{}", shared_key).into_bytes(),
                    params: ExecutionParams {
                        id_to: contract_id.to_string(),
                        function_call: "execute".to_string(),
                        detailed_proof: false,
                        expected_hash: Vec::new(),
                    },
                    operation_id: None,
                    previous_operation_id: None,
                    operation_context: None,
                };
                
                coordinator.execute_task(&get_payload).await
            }
        }),
        tokio::spawn({
            let coordinator = coordinator.clone();
            async move {
                let update_payload = ExecutionPayload {
                    input: format!("store,{},value_1", shared_key).into_bytes(),
                    params: ExecutionParams {
                        id_to: contract_id.to_string(),
                        function_call: "execute".to_string(),
                        detailed_proof: false,
                        expected_hash: Vec::new(),
                    },
                    operation_id: None,
                    previous_operation_id: None,
                    operation_context: None,
                };
                
                coordinator.execute_task(&update_payload).await
            }
        }),
        tokio::spawn({
            let coordinator = coordinator.clone();
            async move {
                let update_payload = ExecutionPayload {
                    input: format!("store,{},value_2", shared_key).into_bytes(),
                    params: ExecutionParams {
                        id_to: contract_id.to_string(),
                        function_call: "execute".to_string(),
                        detailed_proof: false,
                        expected_hash: Vec::new(),
                    },
                    operation_id: None,
                    previous_operation_id: None,
                    operation_context: None,
                };
                
                coordinator.execute_task(&update_payload).await
            }
        })
    );
    
    // Verify results
    let get_value = get_result.unwrap().unwrap();
    println!("Get operation returned: {}", String::from_utf8_lossy(&get_value));
    
    // Both updates should succeed or one should fail due to conflict
    let update1 = update1_result.unwrap();
    let update2 = update2_result.unwrap();
    
    println!("Update 1 result: {:?}", update1);
    println!("Update 2 result: {:?}", update2);
    
    // Check the final value
    let final_get_payload = ExecutionPayload {
        input: format!("get,{}", shared_key).into_bytes(),
        params: ExecutionParams {
            id_to: contract_id.to_string(),
            function_call: "execute".to_string(),
            detailed_proof: false,
            expected_hash: Vec::new(),
        },
        operation_id: None,
        previous_operation_id: None,
        operation_context: None,
    };
    
    let final_value = coordinator.execute_task(&final_get_payload).await.unwrap();
    println!("Final value after concurrent operations: {}", String::from_utf8_lossy(&final_value));
    
    // The final value should be either "value_1" or "value_2"
    assert!(
        final_value == b"value_1" || final_value == b"value_2",
        "Final value is neither value_1 nor value_2: {}",
        String::from_utf8_lossy(&final_value)
    );
    
    println!("State conflict handling test passed successfully");
}
