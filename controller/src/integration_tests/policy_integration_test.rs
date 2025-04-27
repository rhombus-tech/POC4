use tee_interface::prelude::{ExecutionPayload, ExecutionParams};
use tee_interface::TeeExecutor;
use tee_interface::TeeError;
use crate::hyper_integration::HyperTeeController;
use crate::policy::{Policy, Transaction, PolicyRule, PolicyViolation, CircuitBreakerLevel, CircuitBreaker, TriggerCondition, RecoveryCondition, CircuitBreakerAction};

use chrono::Utc;
use uuid::Uuid;

// Helper function to create a test execution payload
fn create_test_payload(_amount: u64, _contract_type: Option<String>) -> ExecutionPayload {
    ExecutionPayload {
        operation_id: Some("1".to_string()),
        previous_operation_id: None,
        operation_context: None,
        input: b"store,key1,value1".to_vec(),
        params: ExecutionParams {
            expected_hash: vec![],
            detailed_proof: false,
            function_call: "transfer".to_string(),
            id_to: "test-contract".to_string(),
        },
    }
}

// Helper function to create a Transaction for policy checking
fn create_test_transaction(amount: u64, contract_id: Option<String>) -> Transaction {
    Transaction {
        id: Uuid::new_v4().to_string(),
        sender: "sender".to_string(),
        recipient: Some("test-contract".to_string()),
        amount,
        contract_id,
        function: Some("transfer".to_string()),
        parameters: None,
        region_id: "test-region".to_string(),
        timestamp: Utc::now(),
    }
}

#[tokio::test]
async fn test_valid_transaction_passes_policy() {
    // Initialize controller with policy manager
    // Set the environment variable for the test
    std::env::set_var("REGION_ID", "test-region");
    let mut controller = HyperTeeController::new().await;
    controller.initialize_policy_manager().await.unwrap();
    
    // Create a valid transaction (low amount)
    let transaction = create_test_transaction(100, Some("financial".to_string()));
    
    // Check policy compliance
    let _result = controller.check_policy_compliance(&transaction).await.unwrap();
    
    // Execute a payload
    let payload = create_test_payload(100, Some("financial".to_string()));
    let execution_result = controller.execute(&payload).await.unwrap();
    
    // Verify execution was successful
    assert!(execution_result.result.len() > 0);
}

#[tokio::test]
async fn test_high_value_transaction_triggers_circuit_breaker() {
    // Initialize controller with policy manager
    // Set the environment variable for the test
    std::env::set_var("REGION_ID", "test-region");
    let mut controller = HyperTeeController::new().await;
    controller.initialize_policy_manager().await.unwrap();
    
    // Manually set a transaction limit policy
    let mut policy = Policy::new("test-policy", "1.0", Some("test-region"));
    policy.add_rule(PolicyRule::TransactionLimit { 
        max_amount: 1000, 
        time_window_seconds: 60 
    });

    // Add a circuit breaker definition to the policy
    policy.add_circuit_breaker(CircuitBreaker {
        id: "high-value".to_string(),
        level: CircuitBreakerLevel::Critical,
        trigger_conditions: vec![
            TriggerCondition::Manual {
                authorized_roles: vec!["admin".to_string()]
            },
        ],
        recovery_conditions: Some(vec![
            RecoveryCondition::ManualApproval {
                required_approvals: 1,
            },
        ]),
        actions: vec![
            CircuitBreakerAction::BlockAllTransactions,
        ],
    });

    let policy_manager = controller.get_policy_manager().await.unwrap();
    {
        policy_manager.add_policy(policy).await;
    }
    
    // Create a high value transaction
    let transaction = Transaction::new_test(
        "user1", 
        Some("user2"), 
        1500, 
        Some("contract1"), 
        Some("transfer"), 
        "test-region"
    );
    
    // This should trigger a policy violation
    let result = controller.check_policy_compliance(&transaction).await;
    assert!(result.is_err());
    
    // Optional: check that it's the right type of violation
    match result {
        Err(PolicyViolation::TransactionLimitExceeded(_)) => {
            println!("Expected policy violation occurred");
        }
        Err(e) => panic!("Unexpected policy violation: {:?}", e),
        Ok(_) => panic!("Transaction should have been rejected"),
    }
}

#[tokio::test]
async fn test_circuit_breaker_manual_reset() {
    // Test that we can manually reset circuit breakers
    // Set the environment variable for the test
    std::env::set_var("REGION_ID", "test-region");
    let mut controller = HyperTeeController::new().await;
    controller.initialize_policy_manager().await.unwrap();
    
    // Create and add a policy with transaction limits
    let mut policy = Policy::new("test-policy", "1.0", Some("test-region"));
    policy.add_rule(PolicyRule::TransactionLimit { 
        max_amount: 1000, 
        time_window_seconds: 60 
    });
    
    // Add a circuit breaker definition to the policy
    policy.add_circuit_breaker(CircuitBreaker {
        id: "high-value".to_string(),
        level: CircuitBreakerLevel::Critical,
        trigger_conditions: vec![
            TriggerCondition::Manual {
                authorized_roles: vec!["admin".to_string()]
            },
        ],
        recovery_conditions: Some(vec![
            RecoveryCondition::ManualApproval {
                required_approvals: 1,
            },
        ]),
        actions: vec![
            CircuitBreakerAction::BlockAllTransactions,
        ],
    });
    
    let policy_manager = controller.get_policy_manager().await.unwrap();
    {
        policy_manager.add_policy(policy).await;
        
        // Manually activate a circuit breaker in this region
        policy_manager.activate_circuit_breaker(
            "high-value", 
            CircuitBreakerLevel::Critical, 
            Some("test-region".to_string())
        ).await;
    }
     
    // Check that the circuit breaker is active
    let status = controller.get_circuit_breaker_status("test-region").await;
    assert!(status.is_ok(), "Should be able to get circuit breaker status");
    let status_map = status.unwrap();
    assert!(status_map.contains_key("high-value"), "Circuit breaker should be active");
     
    // Create a transaction - should be rejected
    let transaction = Transaction::new_test(
        "user1", 
        Some("user2"), 
        500, 
        Some("contract1"), 
        Some("transfer"), 
        "test-region"
    );
    let result = controller.check_policy_compliance(&transaction).await;
    assert!(result.is_err(), "Transaction should be rejected due to circuit breaker");
     
    // Reset the circuit breaker
    let reset_result = controller.reset_circuit_breaker("test-region", "high-value").await;
    assert!(reset_result.is_ok(), "Circuit breaker reset should succeed");
     
    // Check that it's reset (either removed or marked as inactive)
    let new_status = controller.get_circuit_breaker_status("test-region").await;
    assert!(new_status.is_ok(), "Should be able to get circuit breaker status");
    let new_status_map = new_status.unwrap();
    assert!(!new_status_map.contains_key("high-value") || !new_status_map["high-value"], 
           "Circuit breaker should be reset");
     
    // Try another transaction - should pass
    let transaction = Transaction::new_test(
        "user1", 
        Some("user2"), 
        500, 
        Some("contract1"), 
        Some("transfer"), 
        "test-region"
    );
    let result = controller.check_policy_compliance(&transaction).await;
    assert!(result.is_ok(), "Transaction should pass after circuit breaker reset");
}

#[tokio::test]
async fn test_policy_disabled_integration() {
    // Initialize controller without policy manager to test backward compatibility
    let controller = HyperTeeController::new().await;
    
    // Create a payload that would normally violate policy
    let payload = create_test_payload(5000, Some("financial".to_string()));
    
    // Execute should still work since policy manager is not initialized
    let execution_result = controller.execute(&payload).await.unwrap();
    
    // Verify execution was successful
    assert!(execution_result.result.len() > 0);
}
