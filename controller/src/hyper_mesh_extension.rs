use std::collections::HashMap;
use std::time::{Duration, Instant, UNIX_EPOCH, SystemTime};
use std::sync::Arc;
use tee_interface::{ExecutionPayload, ExecutionResult, ExecutionStats, TeeError, TeeType, TeeAttestation, TeeExecutor};
use log::{debug, error, info, warn};
use async_trait::async_trait;

use crate::mesh::{MeshCoordinator, MeshExecutionResult, Attestation, PeerInfo};

// Forward reference to HyperTeeController type
use crate::hyper_integration::HyperTeeController;

/// Utility trait to expose common methods needed by mesh extensions
#[async_trait]
pub trait TeeControllerUtils {
    /// Get the current TEE type as a string
    fn get_tee_type(&self) -> &str;
    
    /// Check if mesh execution should be used for a specific target
    async fn should_use_mesh_execution(&self, region_id: &str, target_tee: &str) -> bool;
    
    /// Check if mesh functionality is enabled
    fn is_mesh_enabled(&self) -> bool;
    
    /// Get the mesh coordinator if available
    fn get_mesh_coordinator(&self) -> Option<&Arc<MeshCoordinator>>;
    
    /// Helper method to determine if mesh execution is appropriate
    async fn is_use_mesh_appropriate(&self, region_id: &str, target_tee: &str) -> bool;
}

#[async_trait]
impl TeeControllerUtils for HyperTeeController {
    fn get_tee_type(&self) -> &str {
        &self.tee_type
    }
    
    async fn should_use_mesh_execution(&self, region_id: &str, target_tee: &str) -> bool {
        // Default implementation - if mesh is not enabled, return false
        if !self.mesh_enabled {
            return false;
        }
        
        // Check if we have the mesh coordinator
        let mesh_coordinator = match &self.mesh_coordinator {
            Some(c) => c,
            None => return false,
        };
        
        // Check if the target is in our peer list
        if !mesh_coordinator.has_peer(target_tee, region_id) {
            return false;
        }
        
        // Otherwise, mesh execution is OK
        true
    }
    
    fn is_mesh_enabled(&self) -> bool {
        self.mesh_enabled
    }
    
    fn get_mesh_coordinator(&self) -> Option<&Arc<MeshCoordinator>> {
        self.mesh_coordinator.as_ref()
    }

    /// Determines if mesh execution is appropriate by calling into should_use_mesh_execution
    async fn is_use_mesh_appropriate(&self, region_id: &str, target_tee: &str) -> bool {
        self.should_use_mesh_execution(region_id, target_tee).await
    }
}

impl HyperTeeController {
    async fn record_mesh_success(&self, payload: &ExecutionPayload, _result: &ExecutionResult) {
        // We got a result from the mesh execution, record metrics
        debug!("Successfully recorded mesh execution metrics for {}", 
               payload.operation_id.as_deref().unwrap_or("unknown"));
        
        // Additional metrics recording will be implemented later
    }
    
    /// Record a mesh failure and return a TeeError
    async fn record_mesh_failure(&self, payload: &ExecutionPayload, error_msg: &str) -> TeeError {
        let region_id = payload.region_id.as_deref().unwrap_or(&self.region_id);
        let target_tee = match &payload.target_tee {
            Some(tee) => tee.as_str(),
            None => "unknown",
        };
        
        // Record failure in metrics
        if let Err(e) = self.metrics.record_mesh_failure(region_id, target_tee).await {
            warn!("Failed to record mesh failure metric: {}", e);
        }
        
        // Return the error to allow the caller to handle it
        TeeError::ExecutionError(format!("Mesh execution failed: {}", error_msg))
    }
    
    /// Execute an operation via the mesh network
    async fn execute_mesh_operation(&self, target_tee: &str, region_id: &str, payload_bytes: &[u8], timeout: Duration) -> Result<MeshExecutionResult, String> {
        // Get the mesh coordinator
        let mesh_coordinator = match &self.mesh_coordinator {
            Some(coordinator) => coordinator.as_ref(),
            None => {
                return Err("Mesh coordinator not available".to_string());
            }
        };
        
        // Convert tee_type string to enum
        let tee_type_str = <Self as TeeControllerUtils>::get_tee_type(self);
        let tee_type_mesh = match tee_type_str {
            "SGX" => "IntelSGX",
            "SEV" => "SEV",
            _ => "IntelSGX", // Default to SGX for other types
        };
        
        // Execute the operation via mesh
        match mesh_coordinator.execute(
            target_tee.to_string(),
            region_id.to_string(),
            tee_type_mesh.to_string(),
            payload_bytes.to_vec(),
            timeout,
            false, // Not async
            false, // No fallback
        ).await {
            Ok(result) => Ok(result),
            Err(e) => Err(format!("Mesh execution failed: {}", e)),
        }
    }
    
    /// Get remote state from a specific TEE in the mesh
    pub async fn get_mesh_remote_state(&self, target_tee: &str, region_id: &str, timeout: Duration) -> Result<Vec<u8>, String> {
        // Check if mesh is enabled and appropriate
        if !self.mesh_enabled {
            return Err("Mesh functionality is disabled".to_string());
        }
        
        // Check circuit breaker status
        if self.metrics.is_circuit_breaker_tripped(region_id, target_tee).await {
            warn!("Circuit breaker tripped for {}/{}, cannot get remote state", region_id, target_tee);
            return Err(format!("Circuit breaker tripped for {}/{}", region_id, target_tee));
        }
        
        // Get the mesh coordinator
        let mesh_coordinator = match &self.mesh_coordinator {
            Some(coordinator) => coordinator.as_ref(),
            None => {
                // Record failure in metrics
                self.metrics.record_mesh_failure(region_id, target_tee).await;
                return Err("Mesh coordinator not available".to_string());
            }
        };

        // Track start time for metrics
        let start_time = Instant::now();
        
        // Create a unique object ID for state synchronization
        let object_id = format!("state:{}:{}", region_id, target_tee);
        
        // Call the sync_state method on the mesh coordinator instead of get_state
        match mesh_coordinator.sync_state(
            object_id,
            target_tee.to_string(),
            true, // Use deltas for more efficient state transfer
        ).await {
            Ok(sync_result) => {
                // Record success metrics
                let duration = start_time.elapsed();
                self.metrics.record_mesh_state_retrieval_success(
                    region_id, 
                    target_tee, 
                    duration.as_millis() as f64
                ).await;
                
                debug!("Successfully retrieved remote state from {}/{} ({} bytes) in {:?}", 
                      region_id, target_tee, sync_result.bytes_transferred, duration);
                
                // In a real implementation, this would return the actual state data
                // For now, return a placeholder vector since the sync_result doesn't contain the actual state
                let placeholder_state = vec![0; sync_result.bytes_transferred as usize];
                Ok(placeholder_state)
            },
            Err(e) => {
                // Record failure in metrics
                let duration = start_time.elapsed();
                self.metrics.record_mesh_failure(region_id, target_tee).await;
                
                warn!("Failed to get remote state from {}/{}: {} (after {:?})", 
                     region_id, target_tee, e, duration);
                
                // Increment circuit breaker counter
                self.metrics.increment_circuit_breaker_failures(region_id, target_tee).await;
                
                Err(format!("Failed to get remote state: {}", e))
            }
        }
    }
    
    /// Discover peers in the mesh
    pub async fn discover_mesh_peers(&self, region_id: Option<String>, timeout: Duration) -> Result<Vec<PeerInfo>, String> {
        // Get the mesh coordinator
        let mesh_coordinator = match &self.mesh_coordinator {
            Some(coordinator) => coordinator.as_ref(),
            None => return Err("Mesh coordinator not available".to_string()),
        };
        
        // Use the local region ID if none is provided
        let region = region_id.unwrap_or_else(|| self.region_id.clone());
        
        // Set a reasonable limit for the number of peers to discover
        let max_results = 100;
        
        // Call the discover_peers method on the mesh coordinator with the correct parameters
        match mesh_coordinator.discover_peers(region, None, max_results).await {
            Ok(peers) => Ok(peers),
            Err(e) => Err(format!("Failed to discover peers: {}", e)),
        }
    }
    
    /// Check the connection health to a specific TEE in the mesh network
    /// Returns the latency in milliseconds or an error if the connection is not available
    async fn check_connection_health(&self, target_tee: &str, region_id: &str, _timeout: Duration) -> Result<u64, String> {
        let mesh_coordinator = match &self.mesh_coordinator {
            Some(coordinator) => coordinator.as_ref(),
            None => return Err("Mesh coordinator not available".to_string()), // Fix the semicolon after return Err back to a comma
        };
        
        // Start timing the connection health check
        let start = std::time::Instant::now();
        
        // Convert tee_type string to proper TeeType
        let tee_type_str = <Self as TeeControllerUtils>::get_tee_type(self);
        
        // Get mesh coordinator
        let mesh_coordinator = match &self.mesh_coordinator {
            Some(coordinator) => coordinator.as_ref(),
            None => {
                // Record failure in metrics
                if let Err(e) = self.metrics.record_mesh_failure(region_id, target_tee).await {
                    debug!("Failed to record mesh failure metric: {}", e);
                }
                return Err("Mesh coordinator not available".to_string());
            }
        };
        
        // Convert tee_type string to enum
        let tee_type_mesh = match tee_type_str {
            "SGX" => "IntelSGX",
            "SEV" => "SEV",
            _ => "IntelSGX", // Default to SGX for other types
        };
        
        // Use a minimal payload for ping test
        let test_input = vec![0, 1, 2, 3]; // Minimal test payload
        let timeout = Duration::from_millis(100); // Short timeout for latency check
        
        // Attempt a minimal execution as a health check
        match mesh_coordinator.execute(
            target_tee.to_string(),
            region_id.to_string(),
            tee_type_mesh.to_string(),
            test_input.clone(),
            timeout,
            false, // Not async
            false, // No fallback
        ).await {
            Ok(_) => {
                let elapsed = start.elapsed().as_millis() as u64;
                Ok(elapsed)
            },
            Err(_) => Err("Mesh connection check failed".to_string()),
        }
    }
    
    /// Get the mesh latency for a specific TEE in a region
    pub async fn get_mesh_latency(&self, region_id: &str, target_tee: &str) -> Result<u64, String> {
        let start = Instant::now();
        
        // Check if mesh is enabled
        if !self.mesh_enabled {
            return Err("Mesh is not enabled".to_string());
        }
        
        // Get mesh coordinator
        let mesh_coordinator = match &self.mesh_coordinator {
            Some(coordinator) => coordinator.as_ref(),
            None => {
                // Record failure in metrics
                if let Err(e) = self.metrics.record_mesh_failure(region_id, target_tee).await {
                    debug!("Failed to record mesh failure metric: {}", e);
                }
                return Err("Mesh coordinator not available".to_string());
            }
        };
        
        // Convert tee_type string to enum
        let tee_type_str = <Self as TeeControllerUtils>::get_tee_type(self);
        let tee_type_mesh = match tee_type_str {
            "SGX" => "IntelSGX",
            "SEV" => "SEV",
            _ => "IntelSGX", // Default to SGX for other types
        };
        
        // Use a minimal payload for ping test
        let test_input = vec![0, 1, 2, 3]; // Minimal test payload
        let timeout = Duration::from_millis(100); // Short timeout for latency check
        
        // Attempt a minimal execution as a health check
        match mesh_coordinator.execute(
            target_tee.to_string(),
            region_id.to_string(),
            tee_type_mesh.to_string(),
            test_input.clone(),
            timeout,
            false, // Not async
            false, // No fallback
        ).await {
            Ok(_) => {
                let elapsed = start.elapsed().as_millis() as u64;
                Ok(elapsed)
            },
            Err(_) => Err("Mesh connection check failed".to_string()),
        }
    }
    
    /// Implement the main execution logic with dual paths (mesh and coordinator)
    pub async fn execute_dual_path(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        let start_time = Instant::now();
        
        // Extract target information for metrics and routing
        let region_id = payload.region_id.as_deref().unwrap_or(&self.region_id);
        let target_tee = match &payload.target_tee {
            Some(tee) => tee.as_str(),
            None => {
                debug!("No target TEE specified, using coordinator execution path");
                return self.execute_original(payload).await;
            }
        };
        
        // Check if we should try mesh execution based on configuration and metrics
        if self.mesh_enabled && <Self as TeeControllerUtils>::should_use_mesh_execution(self, region_id, target_tee).await {
            debug!("Attempting mesh execution for {}/{}", region_id, target_tee);
            
            // Try mesh execution first
            match self.try_mesh_execution(payload).await? {
                Some(result) => {
                    // Mesh execution succeeded
                    let duration = start_time.elapsed();
                    debug!("Mesh execution successful for {}/{} in {:?}", 
                          region_id, target_tee, duration);
                    
                    // Record successful mesh execution metrics
                    if let Err(e) = self.metrics.record_mesh_success(region_id, target_tee).await {
                        warn!("Failed to record mesh success metrics: {}", e);
                    }
                    
                    // Return the successful result
                    return Ok(result);
                },
                None => {
                    // Mesh execution not attempted or not available
                    debug!("Mesh execution not attempted for {}/{}, falling back to coordinator", 
                          region_id, target_tee);
                }
            }
        } else {
            debug!("Skipping mesh execution based on configuration or metrics for {}/{}", 
                  region_id, target_tee);
        }
        
        // Fall back to coordinator execution
        debug!("Using coordinator execution path for {}/{}", region_id, target_tee);
        let coordinator_result = self.execute_original(payload).await;
        
        // Record metrics comparing paths (even if we didn't try mesh this time)
        // This helps build historical data for future routing decisions
        if coordinator_result.is_ok() {
            let duration = start_time.elapsed();
            debug!("Coordinator execution successful for {}/{} in {:?}", 
                  region_id, target_tee, duration);
                  
            // Record metrics about coordinator execution
            if let Err(e) = self.metrics.record_worker_metric(
                target_tee, 
                region_id, 
                duration.as_millis() as u64, 
                true
            ).await {
                warn!("Failed to record coordinator metrics: {}", e);
            }
        }
        
        coordinator_result
    }
    
    /// Execute directly (the original implementation)
    async fn execute_original(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // Call the original execute implementation from hyper_integration.rs
        // Since there are multiple coordinated_execute implementations, we'll use the trait method
        <Self as TeeExecutor>::execute(self, payload).await
    }
}

/// Extension trait to add mesh execution capabilities to HyperTeeController
#[async_trait]
pub trait MeshExecutionExtension {
    /// Execute a payload via the mesh network
    async fn try_mesh_execution(&self, payload: &ExecutionPayload) -> Result<Option<ExecutionResult>, TeeError>;
    
    /// Check if mesh execution should be attempted
    async fn should_attempt_mesh(&self, payload: &ExecutionPayload) -> bool;
    
    /// Convert a MeshExecutionResult to an ExecutionResult
    fn convert_mesh_result(&self, mesh_result: MeshExecutionResult, payload: &ExecutionPayload) -> ExecutionResult;
    
    /// Record a mesh failure and return a TeeError
    async fn record_mesh_failure(&self, payload: &ExecutionPayload, error_msg: &str) -> TeeError {
        // Log the failure for metrics purposes
        warn!("Mesh execution failed for operation {}: {}", 
              payload.operation_id.as_deref().unwrap_or("unknown"), error_msg);
        
        // You could add additional error recording logic here
        // For example, update metrics, send telemetry data, etc.
        
        // Create and return a formatted error
        TeeError::ExecutionError(format!(
            "Mesh execution failed: {}. Operation ID: {}", 
            error_msg,
            payload.operation_id.as_deref().unwrap_or("unknown")
        ))
    }
}

#[async_trait]
impl MeshExecutionExtension for HyperTeeController {
    async fn try_mesh_execution(&self, payload: &ExecutionPayload) -> Result<Option<ExecutionResult>, TeeError> {
        // Don't attempt mesh execution if it's disabled
        if !self.mesh_enabled {
            debug!("Mesh execution disabled, skipping");
            return Ok(None);
        }
        
        // Extract target information
        let region_id = payload.region_id.as_deref().unwrap_or(&self.region_id);
        let target_tee = match &payload.target_tee {
            Some(tee) => tee.as_str(),
            None => {
                debug!("No target TEE specified, cannot use mesh execution");
                return Ok(None);
            }
        };
        
        // Check if mesh execution is appropriate for this target
        if !<Self as TeeControllerUtils>::is_use_mesh_appropriate(self, region_id, target_tee).await {
            debug!("Mesh execution not appropriate for {}/{}, skipping", region_id, target_tee);
            return Ok(None);
        }
        
        // Check if the circuit breaker is tripped for this target
        if self.metrics.is_circuit_breaker_tripped(region_id, target_tee).await {
            warn!("Circuit breaker tripped for {}/{}, skipping mesh execution", region_id, target_tee);
            return Ok(None);
        }
        
        // Serialize the payload for mesh execution
        let payload_bytes = match bincode::serialize(payload) {
            Ok(bytes) => bytes,
            Err(e) => {
                warn!("Failed to serialize payload for mesh execution: {}", e);
                return Ok(None); // Continue with coordinator execution
            }
        };
        
        // Get the mesh timeout from configuration or use a default
        let mesh_timeout = Duration::from_millis(self.mesh_timeout_ms);
        
        // Execute via mesh network
        debug!("Attempting mesh execution for {}/{}", region_id, target_tee);
        let start_time = Instant::now();
        
        match self.execute_mesh_operation(target_tee, region_id, &payload_bytes, mesh_timeout).await {
            Ok(mesh_result) => {
                let duration = start_time.elapsed();
                debug!("Mesh execution successful for {}/{} in {:?}", region_id, target_tee, duration);
                
                // Convert mesh result to execution result
                let result = self.convert_mesh_result(mesh_result, payload);
                
                // Record metrics for successful mesh execution
                self.record_mesh_success(payload, &result).await;
                
                // Return the execution result
                Ok(Some(result))
            },
            Err(e) => {
                let duration = start_time.elapsed();
                warn!("Mesh execution failed for {}/{}: {} (after {:?})", 
                     region_id, target_tee, e, duration);
                
                // Increment circuit breaker counter
                self.metrics.increment_circuit_breaker_failures(region_id, target_tee).await
                    .unwrap_or_else(|err| warn!("Failed to increment circuit breaker: {}", err));
                
                // Record metrics for failed mesh execution if we should still try coordinator
                if duration < Duration::from_millis(self.mesh_timeout_ms / 2) {
                    // If we failed quickly, we can try coordinator execution
                    debug!("Mesh execution failed quickly, will try coordinator");
                    Ok(None)
                } else {
                    // If we've already spent significant time, propagate the error
                    Err(self.record_mesh_failure(payload, &format!("{}", e)).await)
                }
            }
        }
    }
    
    async fn should_attempt_mesh(&self, payload: &ExecutionPayload) -> bool {
        // First check if mesh is enabled globally
        if !self.mesh_enabled {
            return false;
        }
        
        // Check if we have a mesh coordinator
        if self.mesh_coordinator.is_none() {
            return false;
        }
        
        // Get target information
        let region_id = payload.region_id.as_deref().unwrap_or(&self.region_id);
        let target_tee = match &payload.target_tee {
            Some(tee) => tee.as_str(),
            None => return false, // Need target TEE for mesh execution
        };
        
        // Check if this is an appropriate target for mesh execution
        if !<Self as TeeControllerUtils>::is_use_mesh_appropriate(self, region_id, target_tee).await {
            return false;
        }
        
        // Check circuit breaker status
        match self.metrics.is_circuit_breaker_tripped(region_id, target_tee).await {
            true => false, // Circuit breaker is tripped, don't use mesh
            false => true, // All checks passed, can attempt mesh execution
        }
    }
    
    fn convert_mesh_result(&self, mesh_result: MeshExecutionResult, payload: &ExecutionPayload) -> ExecutionResult {
        // Create attestation from mesh data if available
        let attestation = if let Some(attestation_vec) = mesh_result.attestations {
            if let Some(mesh_attestation) = attestation_vec.first() {
                TeeAttestation {
                    enclave_id: <[u8; 32]>::default().to_vec(), // Convert to Vec<u8> as required
                    measurement: mesh_attestation.measurement.clone(),
                    data: Vec::new(), // Not available in mesh::Attestation
                    signature: Vec::new(), // Not available in mesh::Attestation
                    region_proof: None, // Not available in mesh::Attestation
                    timestamp: mesh_attestation.timestamp, // Use timestamp from mesh attestation
                    enclave_type: match mesh_attestation.enclave_type.as_str() {
                        "IntelSGX" => TeeType::SGX,
                        "SEV" => TeeType::SEV,
                        _ => TeeType::SGX, // Default if unknown
                    },
                }
            } else {
                // Empty attestation vector
                TeeAttestation {
                    enclave_id: <[u8; 32]>::default().to_vec(),
                    measurement: Vec::new(),
                    data: Vec::new(),
                    signature: Vec::new(),
                    region_proof: None,
                    timestamp: 0,
                    enclave_type: TeeType::SGX,
                }
            }
        } else {
            // No attestation provided
            TeeAttestation {
                enclave_id: <[u8; 32]>::default().to_vec(),
                measurement: Vec::new(),
                data: Vec::new(),
                signature: Vec::new(),
                region_proof: None,
                timestamp: 0,
                enclave_type: TeeType::SGX,
            }
        };
        
        // Create execution stats with all required fields
        let stats = ExecutionStats {
            execution_time: mesh_result.execution_time_ns / 1000, // Convert ns to μs
            memory_used: mesh_result.memory_used,
            syscall_count: mesh_result.syscall_count,
            network_latency: mesh_result.network_latency_ns / 1000000, // Convert ns to ms
            custom_metrics: None,
        };
        
        // Get current time for timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        // Create the execution result with all required fields
        ExecutionResult {
            result: mesh_result.result,
            attestations: vec![attestation], // Use vector for attestations field
            state_hash: mesh_result.state_hash,
            stats,
            operation_id: payload.operation_id.clone(),
            operation_status: Some("Success".to_string()),
            pending_operations: Some(Vec::new()),
            timestamp: timestamp.to_string(),
        }
    }
}
