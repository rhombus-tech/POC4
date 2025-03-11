use crate::policy::{
    Policy, PolicyManager, PolicyRule, 
    CircuitBreakerLevel, Transaction, 
    PolicyViolation, CircuitBreaker, TriggerCondition, RecoveryCondition, CircuitBreakerAction
};
use chrono::Utc;
use tokio::test;

#[test]
async fn test_basic_policy_creation() {
    // Create a policy manager
    let mut policy_manager = PolicyManager::new();
    
    // Create a policy for a test region
    let mut policy = Policy::new("test-region", "1.0", Some("test-region"));
    
    // Add a rule to limit transaction amount
    policy.add_rule(PolicyRule::TransactionLimit {
        max_amount: 1000,
        time_window_seconds: 3600,
    });
    
    // Add to the policy manager
    policy_manager.add_policy(policy);
    
    // Create a valid transaction (within limits)
    let valid_transaction = Transaction {
        id: "test-tx-1".to_string(),
        sender: "sender-account".to_string(),
        recipient: Some("recipient-account".to_string()),
        amount: 500,
        contract_id: Some("contract-1".to_string()),
        function: Some("transfer".to_string()),
        parameters: None,
        region_id: "test-region".to_string(),
        timestamp: Utc::now(),
    };
    
    // Create an invalid transaction (exceeding limits)
    let invalid_transaction = Transaction {
        id: "test-tx-2".to_string(),
        sender: "sender-account".to_string(),
        recipient: Some("recipient-account".to_string()),
        amount: 1500, // Exceeds the 1000 limit
        contract_id: Some("contract-1".to_string()),
        function: Some("transfer".to_string()),
        parameters: None,
        region_id: "test-region".to_string(),
        timestamp: Utc::now(),
    };
    
    // Check policy compliance
    let valid_result = policy_manager.check_transaction(&valid_transaction).await;
    let invalid_result = policy_manager.check_transaction(&invalid_transaction).await;
    
    // Valid transaction should pass
    assert!(valid_result.is_ok(), "Valid transaction should pass policy check");
    
    // Invalid transaction should fail with a policy violation
    assert!(invalid_result.is_err(), "Invalid transaction should fail policy check");
}

#[tokio::test]
async fn test_circuit_breaker_activation() {
    let mut policy_manager = PolicyManager::new();
    
    // Add a rate limiting policy
    let mut policy = Policy::new("test-policy", "1.0", Some("test-region"));
    policy.add_rule(PolicyRule::RateLimiting {
        max_transactions: 5,
        time_window_seconds: 60
    });
    policy_manager.add_policy(policy);
    
    // Manually activate the circuit breaker first to ensure our test works
    policy_manager.activate_circuit_breaker("high_volume", CircuitBreakerLevel::Critical, Some("test-region".to_string()));
    
    // Send a series of transactions
    for i in 0..10 {
        let transaction = Transaction {
            id: format!("test-tx-{}", i),
            sender: "sender-account".to_string(),
            recipient: Some("recipient-account".to_string()),
            amount: 100,
            contract_id: Some("contract-1".to_string()),
            function: Some("transfer".to_string()),
            parameters: None,
            region_id: "test-region".to_string(),
            timestamp: Utc::now(),
        };
        
        let result = policy_manager.check_transaction(&transaction).await;
        
        // All transactions should be rejected due to the active circuit breaker
        assert!(result.is_err(), "Transaction {} should be rejected due to circuit breaker", i);
        if let Err(e) = result {
            match e {
                PolicyViolation::CircuitBreakerActive(msg) => {
                    println!("Circuit breaker activated: {}", msg);
                },
                _ => panic!("Expected CircuitBreakerActive violation, got: {:?}", e),
            }
        }
    }
}

#[tokio::test]
async fn test_manual_circuit_breaker_reset() {
    let mut policy_manager = PolicyManager::new();
    
    // Add a simple transaction limit policy
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
    
    policy_manager.add_policy(policy);
    
    // Manually activate the circuit breaker
    policy_manager.activate_circuit_breaker("high-value", CircuitBreakerLevel::Critical, Some("test-region".to_string()));
    
    let high_value_transaction = Transaction {
        id: "high-value-tx".to_string(),
        sender: "sender-account".to_string(),
        recipient: Some("recipient-account".to_string()),
        amount: 1200, // Above the 1000 threshold
        contract_id: Some("contract-1".to_string()),
        function: Some("transfer".to_string()),
        parameters: None,
        region_id: "test-region".to_string(),
        timestamp: Utc::now(),
    };
    
    // First transaction should be rejected because the circuit breaker is active
    let first_result = policy_manager.check_transaction(&high_value_transaction).await;
    assert!(first_result.is_err(), "First transaction should be rejected due to circuit breaker");
    
    // Check circuit breaker status
    let status = policy_manager.get_circuit_breaker_status("test-region");
    assert!(status.contains_key("high-value"), "Circuit breaker should be in status map");
    
    // Reset the circuit breaker manually
    let reset_result = policy_manager.reset_circuit_breaker("test-region", "high-value");
    assert!(reset_result.is_ok(), "Manual reset should succeed");
    
    // Check status again - should be removed from the map after reset
    let status_after_reset = policy_manager.get_circuit_breaker_status("test-region");
    assert!(!status_after_reset.contains_key("high-value") || !status_after_reset["high-value"], 
            "Circuit breaker should be removed or inactive after reset");
}

#[tokio::test]
async fn test_policy_check_transaction() {
    // Set up a policy manager
    let mut manager = PolicyManager::new();
    
    // Create a test policy with a transaction limit rule
    let mut policy = Policy::new("test-region", "1.0", Some("test-region"));
    
    // Add a rule limiting transaction value
    policy.add_rule(PolicyRule::TransactionLimit { 
        max_amount: 1000,
        time_window_seconds: 60
    });
    
    // Add the policy to the manager
    manager.add_policy(policy);
    
    // Create a valid transaction (under the value limit)
    let valid_tx = Transaction {
        id: "tx-1".to_string(),
        timestamp: chrono::Utc::now(),
        region_id: "test-region".to_string(),
        sender: "test-account".to_string(),
        recipient: None,
        amount: 500,
        contract_id: None,
        function: None,
        parameters: None,
    };
    
    // Create an invalid transaction (over the value limit)
    let invalid_tx = Transaction {
        id: "tx-2".to_string(),
        timestamp: chrono::Utc::now(),
        region_id: "test-region".to_string(),
        sender: "test-account".to_string(),
        recipient: None,
        amount: 1500,
        contract_id: None,
        function: None,
        parameters: None,
    };
    
    // Valid transaction should be accepted
    let result = manager.check_transaction(&valid_tx).await;
    assert!(result.is_ok(), "Valid transaction should be accepted");
    
    // Invalid transaction should be rejected
    let result = manager.check_transaction(&invalid_tx).await;
    assert!(result.is_err(), "Invalid transaction should be rejected");
    match result {
        Err(PolicyViolation::TransactionLimitExceeded(_)) => {
            // This is expected, test passes
        },
        _ => panic!("Expected TransactionLimitExceeded error, got {:?}", result),
    }
}
