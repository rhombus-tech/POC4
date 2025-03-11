use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use thiserror::Error;

/// Represents a policy that can be enforced by TEEs
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Policy {
    /// Unique identifier for the policy
    pub id: String,
    /// Version string for tracking policy updates
    pub version: String,
    /// Optional region ID if the policy is specific to a region
    pub region_id: Option<String>,
    /// Set of rules that comprise this policy
    pub rules: Vec<PolicyRule>,
    /// Circuit breakers associated with this policy
    pub circuit_breakers: Vec<CircuitBreaker>,
    /// When the policy was created or last updated
    pub updated_at: DateTime<Utc>,
    /// Who created or last updated the policy
    pub updated_by: String,
}

impl Policy {
    /// Create a new policy with default values
    pub fn new(id: &str, version: &str, region_id: Option<&str>) -> Self {
        Self {
            id: id.to_string(),
            version: version.to_string(),
            region_id: region_id.map(|s| s.to_string()),
            rules: Vec::new(),
            circuit_breakers: Vec::new(),
            updated_at: Utc::now(),
            updated_by: "system".to_string(),
        }
    }

    /// Add a rule to this policy
    pub fn add_rule(&mut self, rule: PolicyRule) -> &mut Self {
        self.rules.push(rule);
        self
    }

    /// Add a circuit breaker to this policy
    pub fn add_circuit_breaker(&mut self, breaker: CircuitBreaker) -> &mut Self {
        self.circuit_breakers.push(breaker);
        self
    }
}

/// Different types of policy rules that can be enforced
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum PolicyRule {
    /// Limit transaction amounts within a time window
    TransactionLimit {
        max_amount: u64,
        time_window_seconds: u64,
    },
    /// Limit transaction rates within a time window
    RateLimiting {
        max_transactions: u32,
        time_window_seconds: u64,
    },
    /// Restrict contract execution to an allowlist
    AllowedContracts {
        contract_ids: Vec<String>,
    },
    /// Prevent transactions from/to specific addresses
    BlockedAddresses {
        addresses: Vec<String>,
    },
    /// Control whether data must remain within a region
    DataLocalization {
        must_stay_in_region: bool,
    },
    /// Custom rule for extensibility
    Custom {
        rule_type: String,
        parameters: HashMap<String, String>,
    },
}

/// Circuit breaker configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CircuitBreaker {
    /// Unique identifier for this circuit breaker
    pub id: String,
    /// The severity level of this circuit breaker
    pub level: CircuitBreakerLevel,
    /// Conditions that trigger this circuit breaker
    pub trigger_conditions: Vec<TriggerCondition>,
    /// Optional conditions for automatic recovery
    pub recovery_conditions: Option<Vec<RecoveryCondition>>,
    /// Actions to take when triggered
    pub actions: Vec<CircuitBreakerAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum CircuitBreakerLevel {
    /// Still allows transactions but logs warnings
    Warning,
    /// Allows only certain types of transactions 
    Restricted,
    /// Blocks specific transaction types with errors
    Error,
    /// Halts all transactions
    Critical,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TriggerCondition {
    /// Trigger based on transaction volume
    TransactionVolume {
        threshold: u64,
        time_window_seconds: u64,
    },
    /// Trigger based on price deviation for an asset
    PriceDeviation {
        asset_id: String,
        threshold_percent: f64,
        time_window_seconds: u64,
    },
    /// Trigger based on error rate in transaction processing
    ErrorRate {
        threshold_percent: f64,
        time_window_seconds: u64,
    },
    /// Trigger if attestation failures occur
    AttestationFailure {
        minimum_nodes: u32,
    },
    /// Manual trigger by authorized roles
    Manual {
        authorized_roles: Vec<String>,
    },
    /// Custom trigger for extensibility
    Custom {
        condition_type: String,
        parameters: HashMap<String, String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RecoveryCondition {
    /// Recover after elapsed time
    TimeElapsed {
        seconds: u64,
    },
    /// Recover after manual approval
    ManualApproval {
        required_approvals: u32,
    },
    /// Recover when metrics return to normal
    MetricsNormalized {
        time_window_seconds: u64,
    },
    /// Custom recovery condition
    Custom {
        condition_type: String,
        parameters: HashMap<String, String>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CircuitBreakerAction {
    /// Only log a warning
    LogWarning,
    /// Reject specific contract calls
    RejectSpecificContractCalls {
        contract_ids: Vec<String>,
    },
    /// Reject all transactions
    RejectAllTransactions,
    /// Notify administrator
    NotifyAdministrator {
        notification_method: String,
    },
    /// Custom action
    Custom {
        action_type: String,
        parameters: HashMap<String, String>,
    },
    /// Block all transactions
    BlockAllTransactions,
    /// Reject high value transactions
    RejectHighValueTransactions {
        threshold: u64,
    },
}

/// Information about an active circuit breaker
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveCircuitBreaker {
    /// ID of the circuit breaker that was triggered
    pub breaker_id: String,
    /// Region ID where this circuit breaker is active
    pub region_id: String,
    /// When the circuit breaker was activated
    pub activated_at: DateTime<Utc>,
    /// Who or what triggered the circuit breaker
    pub triggered_by: String,
    /// Optional reason for activation
    pub reason: Option<String>,
    /// Current level of the circuit breaker
    pub level: CircuitBreakerLevel,
    /// Actions taken when triggered
    pub actions_taken: Vec<CircuitBreakerAction>,
}

/// Error types related to policy violations
#[derive(Debug, Error)]
pub enum PolicyViolation {
    #[error("Transaction limit exceeded: {0}")]
    TransactionLimitExceeded(String),
    
    #[error("Rate limit exceeded: {0}")]
    RateLimitExceeded(String),
    
    #[error("Contract not allowed: {0}")]
    ContractNotAllowed(String),
    
    #[error("Address blocked: {0}")]
    AddressBlocked(String),
    
    #[error("Data localization violation: {0}")]
    DataLocalizationViolation(String),
    
    #[error("Circuit breaker active: {0}")]
    CircuitBreakerActive(String),
    
    #[error("Custom policy violation: {0}")]
    Custom(String),
}

/// Basic transaction representation for policy checking
/// This is simplified and would be replaced by your actual Transaction type
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub sender: String,
    pub recipient: Option<String>,
    pub amount: u64,
    pub contract_id: Option<String>,
    pub function: Option<String>,
    pub parameters: Option<Vec<u8>>,
    pub region_id: String,
    pub timestamp: DateTime<Utc>,
}

impl Transaction {
    /// Create a new transaction for testing
    pub fn new_test(
        sender: &str,
        recipient: Option<&str>,
        amount: u64,
        contract_id: Option<&str>,
        function: Option<&str>,
        region_id: &str,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            sender: sender.to_string(),
            recipient: recipient.map(|s| s.to_string()),
            amount,
            contract_id: contract_id.map(|s| s.to_string()),
            function: function.map(|s| s.to_string()),
            parameters: None,
            region_id: region_id.to_string(),
            timestamp: Utc::now(),
        }
    }
}

/// Manages policies and enforces them
pub struct PolicyManager {
    policies: HashMap<String, Policy>,
    active_circuit_breakers: HashMap<String, ActiveCircuitBreaker>,
    transaction_history: VecDeque<Transaction>,
    max_history_size: usize,
}

impl PolicyManager {
    /// Create a new policy manager
    pub fn new() -> Self {
        Self {
            policies: HashMap::new(),
            active_circuit_breakers: HashMap::new(),
            transaction_history: VecDeque::new(),
            max_history_size: 1000, // Keep last 1000 transactions for policy checks
        }
    }
    
    /// Add or update a policy
    pub fn add_policy(&mut self, policy: Policy) {
        self.policies.insert(policy.id.clone(), policy);
    }
    
    /// Get a policy by ID
    pub fn get_policy(&self, policy_id: &str) -> Option<&Policy> {
        self.policies.get(policy_id)
    }
    
    /// Remove a policy
    pub fn remove_policy(&mut self, policy_id: &str) {
        self.policies.remove(policy_id);
    }
    
    /// Check if a transaction complies with all applicable policies
    pub async fn check_transaction(&mut self, transaction: &Transaction) -> Result<(), PolicyViolation> {
        // First check if any circuit breakers are active
        if let Err(policy_violation) = self.check_circuit_breakers(transaction).await {
            return Err(policy_violation);
        }

        // Check each policy
        let _violations: Vec<PolicyViolation> = Vec::new();
        let mut circuit_breakers_to_activate = Vec::new();
        
        for (_, policy) in &self.policies {
            // Skip if policy does not apply to this transaction's region
            if let Some(policy_region) = &policy.region_id {
                if policy_region != &transaction.region_id {
                    continue;
                }
            }

            // Check each rule in policy
            for rule in &policy.rules {
                match rule {
                    PolicyRule::TransactionLimit { max_amount, time_window_seconds: _ } => {
                        // Check if transaction exceeds the maximum amount
                        if transaction.amount > *max_amount {
                            circuit_breakers_to_activate.push(("high_value", CircuitBreakerLevel::Warning, transaction.region_id.clone()));
                            return Err(PolicyViolation::TransactionLimitExceeded(
                                format!("Transaction amount {} exceeds maximum allowed {}", transaction.amount, *max_amount)
                            ));
                        }
                    },
                    PolicyRule::RateLimiting { max_transactions, time_window_seconds: _ } => {
                        // For now, just count transactions to see if we're over the limit
                        let timestamp = transaction.timestamp;
                        let one_minute_ago = timestamp - chrono::Duration::minutes(1);
                        
                        let recent_transaction_count = self.transaction_history.iter()
                            .filter(|tx| tx.timestamp > one_minute_ago && tx.region_id == transaction.region_id)
                            .count();
                        
                        let max_tx = *max_transactions as usize;
                        if recent_transaction_count >= max_tx {
                            circuit_breakers_to_activate.push(("high_volume", CircuitBreakerLevel::Critical, transaction.region_id.clone()));
                            return Err(PolicyViolation::RateLimitExceeded(
                                format!("Transaction rate {} exceeds maximum allowed {} per minute", recent_transaction_count, max_tx)
                            ));
                        }
                    },
                    PolicyRule::AllowedContracts { contract_ids } => {
                        if let Some(contract_id) = &transaction.contract_id {
                            if !contract_ids.contains(contract_id) {
                                circuit_breakers_to_activate.push(("contract_violation", CircuitBreakerLevel::Error, transaction.region_id.clone()));
                                return Err(PolicyViolation::ContractNotAllowed(
                                    format!("Contract {} is not in the allowed list", contract_id)
                                ));
                            }
                        }
                    },
                    PolicyRule::BlockedAddresses { addresses } => {
                        // Check both sender and recipient
                        if addresses.contains(&transaction.sender) {
                            circuit_breakers_to_activate.push(("sender_blocked", CircuitBreakerLevel::Error, transaction.region_id.clone()));
                            return Err(PolicyViolation::AddressBlocked(
                                format!("Sender address {} is blocked", transaction.sender)
                            ));
                        }
                        
                        if let Some(recipient) = &transaction.recipient {
                            if addresses.contains(recipient) {
                                circuit_breakers_to_activate.push(("recipient_blocked", CircuitBreakerLevel::Error, transaction.region_id.clone()));
                                return Err(PolicyViolation::AddressBlocked(
                                    format!("Recipient address {} is blocked", recipient)
                                ));
                            }
                        }
                    },
                    PolicyRule::DataLocalization { must_stay_in_region } => {
                        // This would need more context in a real implementation
                        // For now, we'll assume cross-region transactions violate this if enabled
                        if *must_stay_in_region {
                            // In a real implementation, we would check if the transaction crosses regions
                            // For now, just demonstrate the concept
                            if let Some(recipient) = &transaction.recipient {
                                if recipient.contains("cross-region") {
                                    circuit_breakers_to_activate.push(("cross_region", CircuitBreakerLevel::Error, transaction.region_id.clone()));
                                    return Err(PolicyViolation::DataLocalizationViolation(
                                        format!("Data must stay in region {}", transaction.region_id)
                                    ));
                                }
                            }
                        }
                    },
                    PolicyRule::Custom { rule_type: _, parameters: _ } => {
                        // Custom rules would be implemented based on rule_type and parameters
                        // This is just a placeholder for extensibility
                    }
                }
            }
        }

        // Activate circuit breakers
        for (breaker_id, level, region_id) in circuit_breakers_to_activate {
            self.activate_circuit_breaker(breaker_id, level, Some(region_id));
        }

        // If we reach here, transaction passes all policy checks
        // Add to transaction history for future checks
        if self.transaction_history.len() >= self.max_history_size {
            self.transaction_history.pop_front();
        }
        self.transaction_history.push_back(transaction.clone());
        
        Ok(())
    }
    
    /// Check if any circuit breakers would block this transaction
    async fn check_circuit_breakers(&self, transaction: &Transaction) -> Result<(), PolicyViolation> {
        // First check region-specific circuit breakers
        let region_breakers: Vec<_> = self.active_circuit_breakers.iter()
            .filter(|(_, b)| b.region_id == transaction.region_id)
            .collect();

        // Then check global circuit breakers
        let global_breakers: Vec<_> = self.active_circuit_breakers.iter()
            .filter(|(_, b)| b.region_id == "global")
            .collect();

        // Combine both sets of breakers
        let all_breakers: Vec<_> = [region_breakers, global_breakers].concat();

        // Check all relevant circuit breakers
        for (breaker_id, active_breaker) in all_breakers {
            // Check for specific actions that would block this transaction
            for action in &active_breaker.actions_taken {
                match action {
                    CircuitBreakerAction::BlockAllTransactions => {
                        return Err(PolicyViolation::CircuitBreakerActive(
                            format!("Transaction blocked by active circuit breaker {}", breaker_id)
                        ));
                    },
                    CircuitBreakerAction::RejectHighValueTransactions { threshold } => {
                        if transaction.amount > *threshold {
                            return Err(PolicyViolation::CircuitBreakerActive(
                                format!("High value transaction ({}) blocked by circuit breaker {}", 
                                        transaction.amount, breaker_id)
                            ));
                        }
                    },
                    _ => {} // Other action types would be processed accordingly
                }
            }
        }
        
        Ok(())
    }
    
    /// Activate a circuit breaker
    pub fn activate_circuit_breaker(&mut self, breaker_id: &str, level: CircuitBreakerLevel, region_id: Option<String>) {
        let region_id = region_id.unwrap_or_else(|| "global".to_string());
        
        let active_breaker = ActiveCircuitBreaker {
            breaker_id: breaker_id.to_string(),
            region_id,
            activated_at: Utc::now(),
            triggered_by: "system".to_string(), // In a real system, this would come from authentication
            reason: None,
            level,
            actions_taken: vec![CircuitBreakerAction::BlockAllTransactions],
        };
        
        // Don't activate if already active
        if !self.active_circuit_breakers.contains_key(breaker_id) {
            self.active_circuit_breakers.insert(breaker_id.to_string(), active_breaker);
        }
    }
    
    /// Deactivate a circuit breaker
    pub fn deactivate_circuit_breaker(&mut self, breaker_id: &str) {
        self.active_circuit_breakers.remove(breaker_id);
        
        // In a real implementation, we would log this and notify administrators
        // We would also broadcast to other nodes in the network
    }
    
    /// Get all active circuit breakers
    pub fn get_active_circuit_breakers(&self) -> Vec<&ActiveCircuitBreaker> {
        self.active_circuit_breakers.values().collect()
    }
    
    /// Get active circuit breakers for a specific region
    pub fn get_circuit_breaker_status(&self, region_id: &str) -> HashMap<String, bool> {
        let mut status = HashMap::new();
        
        // Collect circuit breakers from all relevant policies
        for (_, policy) in &self.policies {
            if policy.region_id.as_deref() == Some(region_id) || policy.region_id.is_none() {
                for breaker in &policy.circuit_breakers {
                    let is_active = self.active_circuit_breakers.contains_key(&breaker.id);
                    status.insert(breaker.id.clone(), is_active);
                }
            }
        }
        
        status
    }
    
    /// Reset a circuit breaker by ID
    pub fn reset_circuit_breaker(&mut self, region_id: &str, breaker_id: &str) -> Result<(), String> {
        // Check if the breaker exists in a policy for this region
        let mut found = false;
        
        for (_, policy) in &self.policies {
            if policy.region_id.as_deref() == Some(region_id) || policy.region_id.is_none() {
                for breaker in &policy.circuit_breakers {
                    if breaker.id == breaker_id {
                        found = true;
                        break;
                    }
                }
            }
        }
        
        if !found {
            return Err(format!("Circuit breaker {} not found in region {}", breaker_id, region_id));
        }
        
        // Remove from active circuit breakers
        self.active_circuit_breakers.remove(breaker_id);
        
        Ok(())
    }
}

/// A thread-safe wrapper around PolicyManager for use in services
#[derive(Clone)]
pub struct SharedPolicyManager {
    inner: Arc<RwLock<PolicyManager>>,
}

impl SharedPolicyManager {
    /// Create a new shared policy manager
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(PolicyManager::new())),
        }
    }
    
    /// Add a policy
    pub async fn add_policy(&self, policy: Policy) {
        let mut manager = self.inner.write().await;
        manager.add_policy(policy);
    }
    
    /// Activate a circuit breaker
    pub async fn activate_circuit_breaker(&self, breaker_id: &str, level: CircuitBreakerLevel, region_id: Option<String>) {
        let mut manager = self.inner.write().await;
        manager.activate_circuit_breaker(breaker_id, level, region_id);
    }
    
    /// Reset a circuit breaker
    pub async fn reset_circuit_breaker(&self, region_id: &str, breaker_id: &str) -> Result<(), String> {
        let mut manager = self.inner.write().await;
        manager.reset_circuit_breaker(region_id, breaker_id)
    }
    
    /// Check a transaction
    pub async fn check_transaction(&self, transaction: &Transaction) -> Result<(), PolicyViolation> {
        let mut manager = self.inner.write().await;
        manager.check_transaction(transaction).await
    }
    
    /// Get circuit breaker status for a region
    pub async fn get_circuit_breaker_status(&self, region_id: &str) -> HashMap<String, bool> {
        let manager = self.inner.read().await;
        manager.get_circuit_breaker_status(region_id)
    }
    
    /// Get a clone of the inner Arc for sharing
    pub fn clone_inner(&self) -> Arc<RwLock<PolicyManager>> {
        self.inner.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_policy_enforcement() {
        // Create policy manager
        let mut manager = PolicyManager::new();
        
        // Test simple compliance check
        let valid_tx = Transaction::new_test("user1", Some("user2"), 500, Some("contract1"), Some("transfer"), "us-east");
        
        // Add a policy after creating transaction
        let mut policy = Policy::new("test-policy", "1.0", Some("us-east"));
        policy.add_rule(PolicyRule::TransactionLimit { 
            max_amount: 100, 
            time_window_seconds: 60 
        });
        manager.add_policy(policy);
        
        // Now check - should fail due to amount exceeding limit
        let result = manager.check_transaction(&valid_tx).await;
        assert!(result.is_err());
        
        // Create another transaction under the limit
        let small_tx = Transaction::new_test("user1", Some("user2"), 50, Some("contract1"), Some("transfer"), "us-east");
        
        // This one should pass
        let result = manager.check_transaction(&small_tx).await;
        assert!(result.is_ok());
        
        // Manually activate a circuit breaker
        manager.activate_circuit_breaker("high-value", CircuitBreakerLevel::Critical, Some("us-east".to_string()));
        
        // Now even the small transaction should fail
        let result = manager.check_transaction(&small_tx).await;
        assert!(result.is_err(), "Transaction should be rejected due to active circuit breaker");
        
        // Create a transaction for a different region
        let other_region_tx = Transaction::new_test("user1", Some("user2"), 5000, Some("contract1"), Some("transfer"), "eu-west");
        
        // This should pass since policy only applies to us-east
        let result = manager.check_transaction(&other_region_tx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_check_transaction_validation() {
        // Set up a test transaction
        let mut manager = PolicyManager::new();

        // Create a policy with a rate limit rule
        let mut policy = Policy::new("test-region", "1.0", Some("test-region"));
        policy.add_rule(PolicyRule::RateLimiting { 
            max_transactions: 5, 
            time_window_seconds: 60 
        });
        
        // Add the policy
        manager.add_policy(policy);
        
        // First few transactions should pass
        for i in 0..5 {
            let tx = Transaction {
                id: format!("tx-{}", i),
                timestamp: chrono::Utc::now(),
                region_id: "test-region".to_string(),
                sender: "test-account".to_string(),
                recipient: None,
                amount: 100,
                contract_id: None,
                function: None,
                parameters: None,
            };
            
            let result = manager.check_transaction(&tx).await;
            assert!(result.is_ok(), "Transaction {} should be accepted", i);
        }
        
        // This transaction should exceed the rate limit
        let tx = Transaction {
            id: "tx-overflow".to_string(),
            timestamp: chrono::Utc::now(),
            region_id: "test-region".to_string(),
            sender: "test-account".to_string(),
            recipient: None,
            amount: 100,
            contract_id: None,
            function: None,
            parameters: None,
        };
        
        let result = manager.check_transaction(&tx).await;
        assert!(result.is_err(), "Transaction should be rejected due to rate limit");
        
        // Verify it's the right error type
        match result {
            Err(PolicyViolation::RateLimitExceeded(_)) => {
                // Expected result
            },
            _ => panic!("Expected RateLimitExceeded error, got {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_policy_check_transaction() {
        // Set up a basic valid transaction
        let mut manager = PolicyManager::new();
        
        // Create a policy with a transaction limit rule
        let mut policy = Policy::new("test-region", "1.0", Some("test-region"));
        policy.add_rule(PolicyRule::TransactionLimit { 
            max_amount: 1000, 
            time_window_seconds: 60 
        });
        
        // Add the policy
        manager.add_policy(policy);
        
        // Test a valid transaction
        let valid_tx = Transaction::new_test("user1", Some("user2"), 500, Some("contract1"), Some("transfer"), "test-region");
        
        let result = manager.check_transaction(&valid_tx).await;
        assert!(result.is_ok());
        
        // Test a transaction exceeding the limit
        let invalid_tx = Transaction::new_test("user1", Some("user2"), 1500, Some("contract1"), Some("transfer"), "test-region");
        
        let result = manager.check_transaction(&invalid_tx).await;
        assert!(result.is_err());
        if let Err(PolicyViolation::TransactionLimitExceeded(_)) = result {
            // Expected error
        } else {
            panic!("Expected TransactionLimitExceeded, got {:?}", result);
        }
    }
}
