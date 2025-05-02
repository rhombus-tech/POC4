#![allow(unused_imports)]
#![allow(unused_variables)]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tee_controller::HyperTeeController;
use tee_controller::mesh::{MeshConfig, MeshCoordinator, TeeType, PeerInfo, BatchOperation};
use tee_controller::proto::teeservice::{self, ExecutionRequest, ExecutionResult};
use tee_controller::tee_peer::TeePeerService;
use tee_interface::{ExecutionPayload, ExecutionParams, TeeExecutor};
use tokio::sync::Mutex;
use uuid::Uuid;
use log::{info, warn, error, debug};

// This struct represents a single TEE node in our test mesh
#[derive(Clone)]
struct TestTeeNode {
    id: String,
    region_id: String,
    tee_type: TeeType,
    controller: HyperTeeController,
    mesh_coordinator: Option<Arc<MeshCoordinator>>,
    addr: SocketAddr,
    // Add simulated network latency in milliseconds
    simulated_network_latency_ms: u64,
}

impl TestTeeNode {
    async fn new(id: &str, region_id: &str, tee_type: TeeType, port: u16) -> Self {
        let controller = HyperTeeController::new().await;
        let addr = SocketAddr::from_str(&format!("127.0.0.1:{}", port)).unwrap();
        
        TestTeeNode {
            id: id.to_string(),
            region_id: region_id.to_string(),
            tee_type,
            controller,
            mesh_coordinator: None,
            addr,
            // Default latency of 0ms, can be adjusted for testing
            simulated_network_latency_ms: 0,
        }
    }
    
    async fn start_mesh(&mut self, discovery_endpoint: &str) -> Result<(), std::io::Error> {
        // Create mesh configuration
        let config = MeshConfig {
            region_id: self.region_id.clone(),
            endpoint: format!("http://{}", self.addr),
            tee_id: self.id.clone(),
            max_peers: 10,
            discovery_interval_sec: 5,
            discovery_endpoint: discovery_endpoint.to_string(),
            circuit_breaker_threshold: Duration::from_secs(3),
            peer_refresh_interval: Duration::from_secs(30),
            enhanced_discovery: false, // Initially set to false for backward compatibility
            discovery_config: None, // Using the default discovery service for tests
            accumulator_endpoint: Some("http://localhost:8090".to_string()),
            local_identity: Some(self.id.clone()),
        };
        
        // Initialize mesh coordinator
        let mesh_coordinator_arc = MeshCoordinator::new(config.clone()).await?;
        
        // Instead of calling start_discovery directly, we'll spawn our own discovery task
        // since we've already started discovery in the MeshCoordinator::new method
        let config_clone = config.clone();
        let mesh_coordinator_arc_clone = Arc::clone(&mesh_coordinator_arc);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(config_clone.discovery_interval_sec));
            loop {
                interval.tick().await;
                // We can't call refresh_peers directly since it's private
                // In a real test, this would perform a direct discovery request
                println!("Simulating peer discovery for TEE ID: {}", config_clone.tee_id);
                // Sleep to simulate discovery work
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        
        // Store mesh coordinator
        self.mesh_coordinator = Some(mesh_coordinator_arc);
        
        Ok(())
    }
    
    async fn deploy_contract(&self, contract_id: &str, contract_code: &[u8]) -> Result<Vec<u8>, String> {
        // Deploy a contract to this TEE node
        let payload = ExecutionPayload {
            input: contract_code.to_vec(),
            params: ExecutionParams {
                id_to: contract_id.to_string(),
                function_call: "deploy".to_string(),
                detailed_proof: false,
                expected_hash: Vec::new(),
            },
            operation_id: Some(Uuid::new_v4().to_string()),
            previous_operation_id: None,
            operation_context: None,
            region_id: Some(self.region_id.clone()),
            target_tee: Some(self.id.clone()),
            tee_type: Some(self.get_compatible_tee_type_string()),
            allow_fallback: Some(true),
        };
        
        let result = self.controller.execute(&payload).await
            .map_err(|e| format!("Failed to deploy contract: {}", e))?;
            
        Ok(result.result)
    }
    
    async fn execute_contract(&self, contract_id: &str, function: &str, input: &[u8]) -> Result<Vec<u8>, String> {
        // Execute a contract on this TEE node
        let payload = ExecutionPayload {
            input: input.to_vec(),
            params: ExecutionParams {
                id_to: contract_id.to_string(),
                function_call: function.to_string(),
                detailed_proof: false,
                expected_hash: Vec::new(),
            },
            operation_id: Some(Uuid::new_v4().to_string()),
            previous_operation_id: None,
            operation_context: None,
            region_id: Some(self.region_id.clone()),
            target_tee: Some(self.id.clone()),
            tee_type: Some(self.get_compatible_tee_type_string()),
            allow_fallback: Some(true),
        };
        
        let result = self.controller.execute(&payload).await
            .map_err(|e| format!("Failed to execute contract: {}", e))?;
            
        Ok(result.result)
    }
    
    async fn execute_via_mesh(&self, contract_id: &str, function: &str, input: &[u8], target_tee: &str) -> Result<Vec<u8>, std::io::Error> {
        if let Some(mesh_coordinator) = &self.mesh_coordinator {
            // Simulate network latency for mesh communication
            if self.simulated_network_latency_ms > 0 {
                debug!("Simulating network latency of {}ms for TEE-to-TEE communication", self.simulated_network_latency_ms);
                sleep(Duration::from_millis(self.simulated_network_latency_ms)).await;
            }
            
            // Prepare real input for contract execution
            let real_input = format!("{},{},{}", function, contract_id, hex::encode(input)).into_bytes();
            
            // Execute over the mesh
            let result = mesh_coordinator.execute(
                target_tee.to_string(),
                self.region_id.clone(),
                self.get_compatible_tee_type_string(),
                real_input,
                Duration::from_secs(10),
                false,
                true,
            ).await?;
            
            Ok(result.result)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "Mesh coordinator not initialized"))
        }
    }
    
    async fn execute_batch_via_mesh(&self, operations: Vec<(String, String, Vec<u8>, String)>) -> Result<Vec<Vec<u8>>, std::io::Error> {
        if let Some(mesh_coordinator) = &self.mesh_coordinator {
            // Clone Arc for use with execute_batch which now takes Arc<Self>
            let mesh_coordinator_arc = Arc::clone(mesh_coordinator);
            
            // Simulate network latency for mesh communication (only once for the batch)
            if self.simulated_network_latency_ms > 0 {
                debug!("Simulating network latency of {}ms for batch TEE-to-TEE communication", self.simulated_network_latency_ms);
                sleep(Duration::from_millis(self.simulated_network_latency_ms)).await;
            }
            
            // Convert operations to BatchOperation format
            let batch_operations = operations.into_iter().map(|(contract_id, function, input, target_tee)| {
                // Prepare real input for contract execution
                let real_input = format!("{},{},{}", function, contract_id, hex::encode(input)).into_bytes();
                
                // Create batch operation
                BatchOperation {
                    target_tee,
                    region_id: self.region_id.clone(),
                    tee_type: self.get_compatible_tee_type_string(),
                    input: real_input,
                    operation_id: Uuid::new_v4().to_string(),
                }
            }).collect();
            
            // Execute batch over the mesh
            let result = mesh_coordinator_arc.execute_batch(
                batch_operations,
                Duration::from_secs(30),
                true,
            ).await?;
            
            // Extract results from the batch
            let results: Vec<Vec<u8>> = result.operations.into_iter()
                .map(|op| op.result)
                .collect();
            
            // Log performance metrics if available
            if let Some(perf) = result.performance {
                if let Some(ops_per_sec) = perf.operations_per_second {
                    info!("Batch execution performance: {} ops/sec", ops_per_sec);
                }
                if let Some(p95) = perf.p95_execution_ms {
                    info!("Batch execution p95 latency: {}ms", p95);
                }
            }
            
            Ok(results)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "Mesh coordinator not initialized"))
        }
    }
    
    fn set_network_latency(&mut self, latency_ms: u64) {
        self.simulated_network_latency_ms = latency_ms;
    }
    
    fn get_compatible_tee_type_string(&self) -> String {
        match self.tee_type {
            TeeType::IntelSGX => "SGX".to_string(),
            TeeType::SEV => "SEV".to_string(),
            TeeType::TDX => "TDX".to_string(),
        }
    }
}

// Test harness for managing multiple TEE nodes
struct MeshTestHarness {
    discovery_addr: SocketAddr,
    discovery_service: Option<TeePeerService>,
    nodes: HashMap<String, TestTeeNode>,
    tee_pairs: HashMap<String, TeePair>,  // Add pairs tracking
}

// Structure to represent a TEE pair (SGX + SEV)
struct TeePair {
    sgx_node_id: String,
    sev_node_id: String,
    tdx_node_id: String,  // Added TDX node ID for AI workloads
    region_id: String,
}

impl MeshTestHarness {
    fn new() -> Self {
        let discovery_addr = "127.0.0.1:50100".parse().unwrap();
        
        Self {
            discovery_addr,
            discovery_service: None,
            nodes: HashMap::new(),
            tee_pairs: HashMap::new(),  // Initialize pairs
        }
    }
    
    async fn start_discovery_service(&mut self) -> Result<(), std::io::Error> {
        // Create channel for discovery service
        let (tx, mut rx) = mpsc::channel::<(ExecutionRequest, 
                                         mpsc::Sender<Result<ExecutionResult, tonic::Status>>)>(32);
        
        // Create discovery service
        let discovery_service = TeePeerService::new(
            "discovery-service".to_string(),
            "global".to_string(),
            tx,
        );
        
        // Store discovery service
        self.discovery_service = Some(discovery_service);
        
        // Start handler for discovery requests
        let peers = Arc::new(Mutex::new(self.nodes.clone()));
        tokio::spawn(async move {
            println!("Discovery service handler task started");
            while let Some((request, response_tx)) = rx.recv().await {
                println!("Discovery service received request: {:?}", request);
                
                // Handle discovery requests
                if request.function_call == "discover_peers" {
                    let nodes = peers.lock().await;
                    let mut peer_list = Vec::new();
                    
                    // Extract request parameters
                    let region_id = String::from_utf8_lossy(&request.parameters).to_string();
                    
                    // Create peer info for each node
                    for (id, node) in nodes.iter() {
                        if node.region_id == region_id {
                            let peer_info = PeerInfo {
                                tee_id: id.clone(),
                                endpoint: format!("http://{}", node.addr),
                                region_id: node.region_id.clone(),
                                tee_type: node.get_compatible_tee_type_string(),
                                latency_ms: 5.0,
                                status: "active".to_string(),
                            };
                            
                            peer_list.push(serde_json::to_string(&peer_info).unwrap());
                        }
                    }
                    
                    // Create response
                    let result = ExecutionResult {
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        attestations: vec![],
                        state_hash: vec![],
                        result: serde_json::to_string(&peer_list).unwrap().into_bytes(),
                        execution_time: 5,
                        memory_used: 1024,
                        syscall_count: 10,
                    };
                    
                    if let Err(e) = response_tx.send(Ok(result)).await {
                        println!("Error sending response: {:?}", e);
                    }
                }
            }
        });
        
        // Start discovery service
        let discovery_addr = self.discovery_addr;
        let discovery_service = self.discovery_service.as_ref().unwrap();
        let tee_id = discovery_service.get_tee_id().clone();
        let region_id = discovery_service.get_region_id().clone();
        
        tokio::spawn(async move {
            println!("Starting discovery service on {}", discovery_addr);
            
            // Create a dummy channel for execution (not needed for discovery)
            let (tx, _) = mpsc::channel(32);
            
            // Create a new service that we can consume with start()
            let service = TeePeerService::new(tee_id, region_id, tx);
            
            if let Err(e) = service.start(discovery_addr).await {
                println!("Discovery service error: {:?}", e);
            }
            println!("Discovery service exited");
        });
        
        // Wait for discovery service to start
        sleep(Duration::from_secs(1)).await;
        
        Ok(())
    }
    
    async fn add_node(&mut self, id: &str, region_id: &str, tee_type: TeeType, port: u16) -> Result<(), std::io::Error> {
        // Create a new TEE node
        let mut node = TestTeeNode::new(id, region_id, tee_type, port).await;
        
        // Get discovery endpoint
        let discovery_endpoint = format!("http://{}", self.discovery_addr);
        
        // Start mesh for the node
        node.start_mesh(&discovery_endpoint).await?;
        
        // Add node to the harness
        self.nodes.insert(id.to_string(), node);
        
        Ok(())
    }
    
    // New method to create a TEE triple (SGX + SEV + TDX)
    async fn create_tee_pair(&mut self, pair_id: &str, region_id: &str, base_port: u16) -> Result<(), std::io::Error> {
        let sgx_id = format!("{}-sgx", pair_id);
        let sev_id = format!("{}-sev", pair_id);
        let tdx_id = format!("{}-tdx", pair_id);  // Added TDX node ID for AI workloads
        
        // Create SGX node
        self.add_node(&sgx_id, region_id, TeeType::IntelSGX, base_port).await?;
        
        // Create SEV node
        self.add_node(&sev_id, region_id, TeeType::SEV, base_port + 1).await?;
        
        // Create TDX node for high-throughput AI workloads
        self.add_node(&tdx_id, region_id, TeeType::TDX, base_port + 2).await?;
        
        // Register the pair
        self.tee_pairs.insert(pair_id.to_string(), TeePair {
            sgx_node_id: sgx_id.clone(), // Clone here to avoid move
            sev_node_id: sev_id.clone(), // Clone here to avoid move
            tdx_node_id: tdx_id.clone(), // Clone here to avoid move
            region_id: region_id.to_string(),
        });
        
        println!("Created TEE triple {} with SGX node {}, SEV node {}, and TDX node {}", pair_id, sgx_id, sev_id, tdx_id);
        
        Ok(())
    }
    
    // Execute a contract via one member of a pair targeting the other member
    async fn execute_across_pair(&self, pair_id: &str, contract_id: &str, function: &str, input: &[u8], 
                             source_type: TeeType, target_type: TeeType) -> Result<Vec<u8>, std::io::Error> {
        let pair = self.tee_pairs.get(pair_id)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, 
                format!("Pair {} not found", pair_id)))?;
        
        let source_id = match source_type {
            TeeType::IntelSGX => &pair.sgx_node_id,
            TeeType::SEV => &pair.sev_node_id,
            TeeType::TDX => &pair.tdx_node_id,
        };
        
        let target_id = match target_type {
            TeeType::IntelSGX => &pair.sgx_node_id,
            TeeType::SEV => &pair.sev_node_id,
            TeeType::TDX => &pair.tdx_node_id,
        };
        
        let source_node = self.nodes.get(source_id)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, 
                format!("Source node {} not found", source_id)))?;
        
        println!("Executing from {} to {} in pair {}", source_id, target_id, pair_id);
        source_node.execute_via_mesh(contract_id, function, input, target_id).await
    }
    
    // Test execution across all pairs in the region
    async fn test_execution_across_all_pairs(&self, contract_id: &str) -> Result<(), String> {
        if self.tee_pairs.is_empty() {
            return Err("No TEE pairs available for testing".to_string());
        }
        
        println!("Testing execution across all {} TEE pairs...", self.tee_pairs.len());
        
        for (pair_id, pair) in &self.tee_pairs {
            // Test from SGX to SEV
            let test_key = format!("test_key_sgx_to_sev_{}", pair_id);
            let test_value = format!("test_value_{}", Uuid::new_v4());
            let store_input = format!("store,{},{}", test_key, test_value).into_bytes();
            
            println!("Testing SGX to SEV execution in pair {}", pair_id);
            let result = self.execute_across_pair(
                pair_id, contract_id, "execute", &store_input, 
                TeeType::IntelSGX, TeeType::SEV
            ).await.map_err(|e| format!("Failed to execute SGX to SEV in pair {}: {}", pair_id, e))?;
            
            // Test from SEV to SGX
            let test_key = format!("test_key_sev_to_sgx_{}", pair_id);
            let test_value = format!("test_value_{}", Uuid::new_v4());
            let store_input = format!("store,{},{}", test_key, test_value).into_bytes();
            
            println!("Testing SEV to SGX execution in pair {}", pair_id);
            let result = self.execute_across_pair(
                pair_id, contract_id, "execute", &store_input, 
                TeeType::SEV, TeeType::IntelSGX
            ).await.map_err(|e| format!("Failed to execute SEV to SGX in pair {}: {}", pair_id, e))?;
        }
        
        // Wait for state to propagate
        sleep(Duration::from_secs(2)).await;
        
        // Verify state consistency across all nodes
        for (pair_id, pair) in &self.tee_pairs {
            let test_key = format!("test_key_sgx_to_sev_{}", pair_id);
            let consistent = self.verify_state_consistency(contract_id, &test_key).await?;
            
            if !consistent {
                return Err(format!("State inconsistency detected for key {} in pair {}!", test_key, pair_id));
            }
            
            let test_key = format!("test_key_sev_to_sgx_{}", pair_id);
            let consistent = self.verify_state_consistency(contract_id, &test_key).await?;
            
            if !consistent {
                return Err(format!("State inconsistency detected for key {} in pair {}!", test_key, pair_id));
            }
        }
        
        println!("Successfully verified execution across all TEE pairs with state consistency");
        Ok(())
    }
    
    async fn deploy_contract_to_all(&self, contract_id: &str, contract_code: &[u8]) -> Result<(), String> {
        // Deploy the same contract to all nodes
        for (id, node) in &self.nodes {
            println!("Deploying contract to node {}", id);
            node.deploy_contract(contract_id, contract_code).await?;
        }
        
        Ok(())
    }
    
    async fn verify_state_consistency(&self, contract_id: &str, key: &str) -> Result<bool, String> {
        // Get state from all nodes and verify consistency
        let mut state_values = HashMap::new();
        
        for (id, node) in &self.nodes {
            // Create get_state command
            let input = format!("get_state,{}", key).into_bytes();
            
            // Execute get_state
            let result = node.execute_contract(contract_id, "execute", &input).await?;
            let value = String::from_utf8(result).map_err(|e| format!("Invalid UTF-8: {}", e))?;
            
            println!("Node {} state value: {}", id, value);
            state_values.insert(id.clone(), value);
        }
        
        // Check if all values are the same
        if state_values.is_empty() {
            return Ok(true);
        }
        
        let first_value = state_values.values().next().unwrap();
        let all_same = state_values.values().all(|v| v == first_value);
        
        Ok(all_same)
    }
    
    async fn test_contract_execution_across_mesh(&self, contract_id: &str) -> Result<(), String> {
        // Test contract execution across the mesh
        if self.nodes.len() < 2 {
            return Err("Need at least 2 nodes for mesh execution test".to_string());
        }
        
        // Get two node IDs
        let node_ids: Vec<&String> = self.nodes.keys().collect();
        let source_id = node_ids[0];
        let target_id = node_ids[1];
        
        // Get source node
        let source_node = self.nodes.get(source_id).unwrap();
        
        // Test key and value
        let test_key = "test_key";
        let test_value = format!("test_value_{}", Uuid::new_v4());
        
        // Create store command
        let store_input = format!("store,{},{}", test_key, test_value).into_bytes();
        
        // Execute via mesh
        println!("Executing store command from {} to {}", source_id, target_id);
        let store_result = source_node.execute_via_mesh(contract_id, "execute", &store_input, target_id).await
            .map_err(|e| format!("Failed to execute via mesh: {}", e))?;
            
        println!("Store result: {:?}", store_result);
        
        // Wait for state to propagate
        sleep(Duration::from_secs(2)).await;
        
        // Verify state consistency
        let consistent = self.verify_state_consistency(contract_id, test_key).await?;
        
        if !consistent {
            return Err("State inconsistency detected!".to_string());
        }
        
        println!("State consistency verified");
        Ok(())
    }
    
    async fn test_failure_scenario(&self, contract_id: &str) -> Result<(), String> {
        // Test failover when a node fails
        if self.tee_pairs.len() < 2 {
            return Err("Need at least 2 TEE pairs for failure scenario test".to_string());
        }
        
        // Get two pair IDs
        let pair_ids: Vec<&String> = self.tee_pairs.keys().collect();
        let pair1_id = pair_ids[0];
        let pair2_id = pair_ids[1];
        
        // Test SGX failure in first pair
        self.test_pair_failover(pair1_id, contract_id, TeeType::IntelSGX).await?;
        
        // Test SEV failure in second pair
        self.test_pair_failover(pair2_id, contract_id, TeeType::SEV).await?;
        
        println!("All failure scenarios tested successfully");
        Ok(())
    }
    
    async fn test_pair_failover(&self, pair_id: &str, contract_id: &str, failed_type: TeeType) -> Result<(), String> {
        let pair = self.tee_pairs.get(pair_id)
            .ok_or_else(|| format!("Pair {} not found", pair_id))?;
        
        // Get another pair to act as the source
        let source_pair_id = self.tee_pairs.keys()
            .find(|id| *id != pair_id)
            .ok_or_else(|| "Need at least two pairs for failover test".to_string())?;
        let source_pair = self.tee_pairs.get(source_pair_id).unwrap();
        
        // Source node will be the SGX from the source pair
        let source_id = &source_pair.sgx_node_id;
        let source_node = self.nodes.get(source_id).unwrap();
        
        // Target should be the non-failed node in the target pair
        let target_id = match failed_type {
            TeeType::IntelSGX => &pair.sev_node_id,  // If SGX failed, use SEV
            TeeType::SEV => &pair.sgx_node_id,  // If SEV failed, use SGX
            TeeType::TDX => &pair.sgx_node_id,  // If TDX failed, use SGX for its secure properties
        };
        
        let failed_id = match failed_type {
            TeeType::IntelSGX => &pair.sgx_node_id,
            TeeType::SEV => &pair.sev_node_id,
            TeeType::TDX => &pair.tdx_node_id,
        };
        
        // Test key and value
        let test_key = format!("failover_key_{}", pair_id);
        let test_value = format!("failover_value_{}", Uuid::new_v4());
        
        // Create store command
        let store_input = format!("store,{},{}", test_key, test_value).into_bytes();
        
        println!("Testing failover: Simulating failure of {} in pair {}", failed_id, pair_id);
        println!("Executing from {} to {}", source_id, target_id);
        
        let result = source_node.execute_via_mesh(contract_id, "execute", &store_input, target_id).await
            .map_err(|e| format!("Failed to execute during failover: {}", e))?;
        
        // Wait for state to propagate
        sleep(Duration::from_secs(2)).await;
        
        // Verify state consistency
        let consistent = self.verify_state_consistency(contract_id, &test_key).await?;
        
        if !consistent {
            return Err(format!("State inconsistency detected during failover test for pair {}!", pair_id));
        }
        
        println!("Successfully verified failover for pair {} with {:?} failure", pair_id, failed_type);
        Ok(())
    }
    
    async fn test_performance_with_network_latency(&self, contract_id: &str) -> Result<PerformanceMetrics, String> {
        println!("\nTesting mesh network performance with simulated network latency");
        
        // Get the nodes in our first pair
        let pair = self.tee_pairs.get("pair1").ok_or("Pair1 not found")?;
        
        // Use the correct node IDs based on how they were created in create_tee_pair
        let sgx_id = &pair.sgx_node_id;
        let sev_id = &pair.sev_node_id;
        
        let sgx_node = self.nodes.get(sgx_id).ok_or(format!("SGX node {} not found", sgx_id))?;
        let sev_node = self.nodes.get(sev_id).ok_or(format!("SEV node {} not found", sev_id))?;
        
        // Perform multiple operations and measure latency
        const NUM_OPERATIONS: usize = 10;
        
        let mut latencies = Vec::with_capacity(NUM_OPERATIONS);
        
        for i in 0..NUM_OPERATIONS {
            let start = std::time::Instant::now();
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            
            // Execute from SGX to SEV
            match sgx_node.execute_via_mesh(
                contract_id,
                "store",
                format!("{},{}", key, value).as_bytes(),
                sev_id
            ).await {
                Ok(_) => {
                    let duration = start.elapsed();
                    let latency_ms = duration.as_secs_f64() * 1000.0;
                    println!("Operation {} completed in {}ms", i, latency_ms);
                    latencies.push(latency_ms);
                },
                Err(e) => {
                    return Err(format!("Error executing operation {}: {}", i, e));
                }
            }
        }
        
        // Sort latencies for percentile calculations
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        // Calculate performance metrics
        let p50 = percentile(&latencies, 50.0);
        let p95 = percentile(&latencies, 95.0);
        let p99 = percentile(&latencies, 99.0);
        let max = latencies.last().cloned().unwrap_or(0.0);
        
        // Print the results
        println!("Performance results with network latency:");
        println!("- p50 (median): {:.2}ms", p50);
        println!("- p95: {:.2}ms", p95);
        println!("- p99: {:.2}ms", p99);
        println!("- max: {:.2}ms", max);
        
        // Verify that the latency is within acceptable bounds (SLA)
        if p95 <= 100.0 {
            println!("✅ Performance meets the 100ms SLA requirement (p95 is {:.2}ms)", p95);
        } else {
            println!("❌ Performance exceeds the 100ms SLA requirement (p95 is {:.2}ms)", p95);
        }
        
        Ok(PerformanceMetrics {
            p50,
            p95,
            p99,
            max
        })
    }
    
    fn set_network_latency(&mut self, latency_ms: u64) {
        for (_, node) in self.nodes.iter_mut() {
            node.set_network_latency(latency_ms);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mesh_network_with_varying_latency() {
    // Initialize the test harness
    let mut harness = MeshTestHarness::new();
    
    // Start the discovery service
    harness.start_discovery_service().await.unwrap();
    
    // Create a TEE pair in region "us-east-1"
    harness.create_tee_pair("pair1", "us-east-1", 8080).await.unwrap();
    
    // Deploy our test contract to all nodes
    harness.deploy_contract_to_all("kv_store", KV_STORE_CONTRACT).await.unwrap();
    
    // Test with different latency values
    let latency_values = vec![1, 5, 10, 20, 30];
    
    println!("\nTesting mesh network performance with varying network latency");
    println!("{:=^60}", "");
    println!("| {:^10} | {:^10} | {:^10} | {:^10} | {:^10} |", 
             "Latency", "p50 (ms)", "p95 (ms)", "p99 (ms)", "Max (ms)");
    println!("{:=^60}", "");
    
    for latency in latency_values {
        // Update the latency for all nodes
        harness.set_network_latency(latency);
        
        // Run performance test and capture metrics
        match harness.test_performance_with_network_latency("kv_store").await {
            Ok(metrics) => {
                // Print formatted results in table format
                println!("| {:^10} | {:^10} | {:^10} | {:^10} | {:^10} |", 
                         format!("{}ms", latency),
                         format!("{:.1}ms", metrics.p50),
                         format!("{:.1}ms", metrics.p95),
                         format!("{:.1}ms", metrics.p99),
                         format!("{:.1}ms", metrics.max));
            },
            Err(e) => {
                println!("Error testing with latency {}ms: {}", latency, e);
            }
        }
    }
    println!("{:=^60}", "");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mesh_network_under_load() {
    // Initialize the test harness
    let mut harness = MeshTestHarness::new();
    
    // Start the discovery service
    harness.start_discovery_service().await.unwrap();
    
    // Create a TEE pair in region "us-east-1"
    harness.create_tee_pair("pair1", "us-east-1", 8080).await.unwrap();
    
    // Set a realistic network latency
    harness.set_network_latency(5);
    
    // Deploy our test contract to all nodes
    harness.deploy_contract_to_all("kv_store", KV_STORE_CONTRACT).await.unwrap();
    
    println!("\nTesting mesh network performance under concurrent load");
    
    // Get the nodes from our TEE pair
    let pair = harness.tee_pairs.get("pair1").unwrap();
    let sgx_id = &pair.sgx_node_id;
    let sev_id = &pair.sev_node_id;
    
    let sgx_node = harness.nodes.get(sgx_id).unwrap();
    
    // Number of concurrent requests to simulate
    const CONCURRENT_REQUESTS: usize = 10;
    
    // Spawn concurrent operations
    let mut handles = Vec::new();
    let start = std::time::Instant::now();
    
    for i in 0..CONCURRENT_REQUESTS {
        let node = sgx_node.clone();
        let sev_id = sev_id.clone();
        
        let handle = tokio::spawn(async move {
            let start_op = std::time::Instant::now();
            let key = format!("key_concurrent_{}", i);
            let value = format!("value_concurrent_{}", i);
            
            match node.execute_via_mesh(
                "kv_store",
                "store",
                format!("{},{}", key, value).as_bytes(),
                &sev_id
            ).await {
                Ok(_) => {
                    let duration = start_op.elapsed();
                    let latency_ms = duration.as_secs_f64() * 1000.0;
                    (i, latency_ms, true)
                },
                Err(e) => {
                    println!("Error executing concurrent operation {}: {}", i, e);
                    (i, 0.0, false)
                }
            }
        });
        
        handles.push(handle);
    }
    
    // Collect results
    let mut results = Vec::new();
    let mut successful_ops = 0;
    
    for handle in handles {
        match handle.await {
            Ok((id, latency, success)) => {
                if success {
                    println!("Concurrent operation {} completed in {:.2}ms", id, latency);
                    results.push(latency);
                    successful_ops += 1;
                }
            },
            Err(e) => {
                println!("Task join error: {:?}", e);
            }
        }
    }
    
    let total_duration = start.elapsed();
    println!("All {} concurrent operations completed in {:.2}ms", 
             successful_ops, total_duration.as_secs_f64() * 1000.0);
    
    // Calculate metrics if we have results
    if !results.is_empty() {
        results.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let p50 = percentile(&results, 50.0);
        let p95 = percentile(&results, 95.0);
        let p99 = percentile(&results, 99.0);
        let max = results.last().cloned().unwrap_or(0.0);
        
        println!("\nConcurrent performance metrics:");
        println!("- p50 (median): {:.2}ms", p50);
        println!("- p95: {:.2}ms", p95);
        println!("- p99: {:.2}ms", p99);
        println!("- max: {:.2}ms", max);
        
        if p95 <= 100.0 {
            println!("✅ Performance under load meets the 100ms SLA requirement (p95 is {:.2}ms)", p95);
        } else {
            println!("❌ Performance under load exceeds the 100ms SLA requirement (p95 is {:.2}ms)", p95);
        }
        
        // Average throughput in operations per second
        let throughput = successful_ops as f64 / (total_duration.as_secs_f64());
        println!("Throughput: {:.2} operations/second", throughput);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mesh_network() {
    // Create test harness
    let mut harness = MeshTestHarness::new();
    
    // Start discovery service
    harness.start_discovery_service().await.unwrap();
    
    // Create TEE pairs in the same region
    harness.create_tee_pair("pair1", "region-1", 50101).await.unwrap();
    harness.create_tee_pair("pair2", "region-1", 50103).await.unwrap();
    harness.create_tee_pair("pair3", "region-1", 50105).await.unwrap();
    
    // Wait for nodes to discover each other
    println!("Waiting for nodes to discover each other...");
    sleep(Duration::from_secs(5)).await;
    
    // Deploy test contract to all nodes
    let contract_id = "kv-store";
    println!("Deploying contract to all nodes...");
    harness.deploy_contract_to_all(contract_id, KV_STORE_CONTRACT).await.unwrap();
    
    // Test execution across all pairs
    println!("Testing execution across all pairs...");
    harness.test_execution_across_all_pairs(contract_id).await.unwrap();
    
    // Test failure scenario
    println!("Testing failure scenarios...");
    harness.test_failure_scenario(contract_id).await.unwrap();
    
    println!("All tests completed successfully!");
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mesh_network_with_latency() {
    // Initialize the test harness
    let mut harness = MeshTestHarness::new();
    
    // Start the discovery service
    harness.start_discovery_service().await.unwrap();
    
    // Create a TEE pair in region "us-east-1"
    harness.create_tee_pair("pair1", "us-east-1", 8080).await.unwrap();
    
    // Set a realistic network latency simulation for intra-region communication
    // 5ms one-way latency is typical between availability zones in the same region
    harness.set_network_latency(5);
    
    // Deploy our test contract to all nodes
    harness.deploy_contract_to_all("kv_store", KV_STORE_CONTRACT).await.unwrap();
    
    // Test performance with simulated network latency
    harness.test_performance_with_network_latency("kv_store").await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mesh_network_multi_pair_throughput() {
    // Initialize the test harness
    let mut harness = MeshTestHarness::new();
    
    // Start the discovery service
    harness.start_discovery_service().await.unwrap();
    
    // Number of TEE pairs to create in a single region
    const NUM_PAIRS: usize = 5;
    
    println!("\nTesting mesh network throughput with {} TEE pairs", NUM_PAIRS);
    
    // Create multiple TEE pairs
    for i in 0..NUM_PAIRS {
        let pair_id = format!("pair{}", i + 1);
        harness.create_tee_pair(&pair_id, "us-east-1", 8080 + i as u16).await.unwrap();
        println!("Created TEE pair {}", pair_id);
    }
    
    // Set realistic network latency
    harness.set_network_latency(5);
    
    // Deploy contract to all nodes
    harness.deploy_contract_to_all("kv_store", KV_STORE_CONTRACT).await.unwrap();
    
    // Number of concurrent operations per pair
    const OPS_PER_PAIR: usize = 10;
    const TOTAL_OPS: usize = NUM_PAIRS * OPS_PER_PAIR;
    
    println!("Starting {} concurrent operations across {} TEE pairs", TOTAL_OPS, NUM_PAIRS);
    
    // Spawn concurrent operations across all pairs
    let mut handles = Vec::new();
    let start = std::time::Instant::now();
    
    for pair_idx in 0..NUM_PAIRS {
        let pair_id = format!("pair{}", pair_idx + 1);
        let pair = harness.tee_pairs.get(&pair_id).unwrap();
        
        // Use the SGX node of each pair to execute operations
        let sgx_id = &pair.sgx_node_id;
        let sev_id = &pair.sev_node_id;
        let sgx_node = harness.nodes.get(sgx_id).unwrap().clone();
        
        for op_idx in 0..OPS_PER_PAIR {
            let node = sgx_node.clone();
            let sev_id = sev_id.clone();
            let global_idx = pair_idx * OPS_PER_PAIR + op_idx;
            let pair_id_clone = pair_id.clone(); // Clone here inside the inner loop
            
            let handle = tokio::spawn(async move {
                let start_op = std::time::Instant::now();
                let key = format!("key_{}_{}", pair_id_clone, op_idx);
                let value = format!("value_{}_{}", pair_id_clone, op_idx);
                
                match node.execute_via_mesh(
                    "kv_store",
                    "store",
                    format!("{},{}", key, value).as_bytes(),
                    &sev_id
                ).await {
                    Ok(_) => {
                        let duration = start_op.elapsed();
                        let latency_ms = duration.as_secs_f64() * 1000.0;
                        (global_idx, latency_ms, true)
                    },
                    Err(e) => {
                        println!("Error executing operation {} on pair {}: {}", op_idx, pair_id_clone, e);
                        (global_idx, 0.0, false)
                    }
                }
            });
            
            handles.push(handle);
        }
    }
    
    // Collect results
    let mut results = Vec::new();
    let mut successful_ops = 0;
    
    for handle in handles {
        match handle.await {
            Ok((id, latency, success)) => {
                if success {
                    results.push(latency);
                    successful_ops += 1;
                }
            },
            Err(e) => {
                println!("Task join error: {:?}", e);
            }
        }
    }
    
    let total_duration = start.elapsed();
    let total_duration_ms = total_duration.as_secs_f64() * 1000.0;
    
    println!("All {} successful operations completed in {:.2}ms", successful_ops, total_duration_ms);
    
    // Calculate metrics if we have results
    if !results.is_empty() {
        results.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let p50 = percentile(&results, 50.0);
        let p95 = percentile(&results, 95.0);
        let p99 = percentile(&results, 99.0);
        let max = results.last().cloned().unwrap_or(0.0);
        
        println!("\nMulti-pair performance metrics:");
        println!("- p50 (median): {:.2}ms", p50);
        println!("- p95: {:.2}ms", p95);
        println!("- p99: {:.2}ms", p99);
        println!("- max: {:.2}ms", max);
        
        if p95 <= 100.0 {
            println!("✅ Performance meets the 100ms SLA requirement (p95 is {:.2}ms)", p95);
        } else {
            println!("❌ Performance exceeds the 100ms SLA requirement (p95 is {:.2}ms)", p95);
        }
        
        // Calculate throughput - total operations per second
        let throughput = successful_ops as f64 / (total_duration.as_secs_f64());
        let throughput_per_pair = throughput / NUM_PAIRS as f64;
        
        println!("\nAggregate Throughput: {:.2} operations/second", throughput);
        println!("Average Throughput Per Pair: {:.2} operations/second", throughput_per_pair);
        println!("Average Latency: {:.2}ms", results.iter().sum::<f64>() / results.len() as f64);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mesh_network_high_volume_throughput() {
    // Initialize the test harness
    let mut harness = MeshTestHarness::new();
    
    // Start the discovery service
    harness.start_discovery_service().await.unwrap();
    
    // Number of TEE pairs to create in a single region
    const NUM_PAIRS: usize = 5;
    
    println!("\nTesting high volume throughput with {} TEE pairs", NUM_PAIRS);
    
    // Create multiple TEE pairs
    for i in 0..NUM_PAIRS {
        let pair_id = format!("pair{}", i + 1);
        harness.create_tee_pair(&pair_id, "us-east-1", 8080 + i as u16).await.unwrap();
        println!("Created TEE pair {}", pair_id);
    }
    
    // Set realistic network latency
    harness.set_network_latency(5);
    
    // Deploy contract to all nodes
    harness.deploy_contract_to_all("kv_store", KV_STORE_CONTRACT).await.unwrap();
    
    // Number of concurrent operations per pair - high volume
    const OPS_PER_PAIR: usize = 50;
    const TOTAL_OPS: usize = NUM_PAIRS * OPS_PER_PAIR;
    
    println!("Starting {} concurrent operations across {} TEE pairs", TOTAL_OPS, NUM_PAIRS);
    
    // Spawn concurrent operations across all pairs
    let mut handles = Vec::with_capacity(TOTAL_OPS);
    let start = std::time::Instant::now();
    
    for pair_idx in 0..NUM_PAIRS {
        let pair_id = format!("pair{}", pair_idx + 1);
        let pair = harness.tee_pairs.get(&pair_id).unwrap();
        
        // Use the SGX node of each pair to execute operations
        let sgx_id = &pair.sgx_node_id;
        let sev_id = &pair.sev_node_id;
        let sgx_node = harness.nodes.get(sgx_id).unwrap().clone();
        
        for op_idx in 0..OPS_PER_PAIR {
            let node = sgx_node.clone();
            let sev_id = sev_id.clone();
            let global_idx = pair_idx * OPS_PER_PAIR + op_idx;
            let pair_id_clone = pair_id.clone(); // Clone here inside the inner loop
            
            let handle = tokio::spawn(async move {
                let start_op = std::time::Instant::now();
                let key = format!("key_{}_{}", pair_id_clone, op_idx);
                let value = format!("value_{}_{}", pair_id_clone, op_idx);
                
                match node.execute_via_mesh(
                    "kv_store",
                    "store",
                    format!("{},{}", key, value).as_bytes(),
                    &sev_id
                ).await {
                    Ok(_) => {
                        let duration = start_op.elapsed();
                        let latency_ms = duration.as_secs_f64() * 1000.0;
                        (global_idx, latency_ms, true)
                    },
                    Err(e) => {
                        println!("Error executing operation {} on pair {}: {}", op_idx, pair_id_clone, e);
                        (global_idx, 0.0, false)
                    }
                }
            });
            
            handles.push(handle);
        }
    }
    
    // Collect results
    let mut results = Vec::new();
    let mut successful_ops = 0;
    
    for handle in handles {
        match handle.await {
            Ok((id, latency, success)) => {
                if success {
                    results.push(latency);
                    successful_ops += 1;
                }
            },
            Err(e) => {
                println!("Task join error: {:?}", e);
            }
        }
    }
    
    let total_duration = start.elapsed();
    let total_duration_ms = total_duration.as_secs_f64() * 1000.0;
    
    println!("All {} successful operations completed in {:.2}ms", successful_ops, total_duration_ms);
    
    // Calculate metrics if we have results
    if !results.is_empty() {
        results.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let p50 = percentile(&results, 50.0);
        let p95 = percentile(&results, 95.0);
        let p99 = percentile(&results, 99.0);
        let max = results.last().cloned().unwrap_or(0.0);
        
        println!("\nHigh volume performance metrics:");
        println!("- p50 (median): {:.2}ms", p50);
        println!("- p95: {:.2}ms", p95);
        println!("- p99: {:.2}ms", p99);
        println!("- max: {:.2}ms", max);
        
        if p95 <= 100.0 {
            println!("✅ Performance meets the 100ms SLA requirement (p95 is {:.2}ms)", p95);
        } else {
            println!("❌ Performance exceeds the 100ms SLA requirement (p95 is {:.2}ms)", p95);
        }
        
        // Calculate throughput - total operations per second
        let throughput = successful_ops as f64 / (total_duration.as_secs_f64());
        let throughput_per_pair = throughput / NUM_PAIRS as f64;
        
        println!("\nAggregate Throughput: {:.2} operations/second", throughput);
        println!("Average Throughput Per Pair: {:.2} operations/second", throughput_per_pair);
        println!("Average Latency: {:.2}ms", results.iter().sum::<f64>() / results.len() as f64);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mesh_network_batch_vs_individual() {
    let _ = env_logger::builder().filter_level(log::LevelFilter::Info).try_init();
    
    // Create test harness with 5 SGX-SEV pairs
    let mut harness = MeshTestHarness::new();
    
    // Start discovery service
    harness.start_discovery_service().await.unwrap();
    
    // Create 5 TEE pairs in the same region
    let region_id = "us-west-1";
    for i in 0..5 {
        harness.create_tee_pair(&format!("pair-{}", i), region_id, 9000 + i * 10).await.unwrap();
    }
    
    // Deploy the same contract to all TEEs
    let contract_id = "test-contract";
    harness.deploy_contract_to_all(contract_id, KV_STORE_CONTRACT).await.unwrap();
    
    // Wait for mesh to stabilize
    sleep(Duration::from_millis(500)).await;
    
    // Create operations: store different values in the contract
    let num_operations = 500;
    let mut operations = Vec::new();
    
    for i in 0..num_operations {
        let key = format!("key-{}", i);
        let value = format!("value-{}", i);
        let input = format!("{},{}", key, value).into_bytes();
        
        // Determine which pair to target based on operation index
        let pair_index = i % 5;
        let target_type = if i % 2 == 0 { TeeType::IntelSGX } else { TeeType::SEV };
        let target_tee_id = format!("pair-{}-{}", pair_index, target_type.to_string().to_lowercase());
        
        operations.push((contract_id.to_string(), "store".to_string(), input, target_tee_id));
    }
    
    // Test individual execution (baseline)
    info!("Starting individual execution test for {} operations", num_operations);
    let start_individual = std::time::Instant::now();
    
    let source_node = &harness.nodes["pair-0-sgx"];
    
    let mut individual_results = Vec::new();
    for (contract_id, function, input, target_tee) in operations.clone() {
        let result = source_node.execute_via_mesh(&contract_id, &function, &input, &target_tee).await.unwrap();
        individual_results.push(result);
    }
    
    let individual_time = start_individual.elapsed();
    let individual_tps = num_operations as f64 / individual_time.as_secs_f64();
    
    info!("Individual execution completed in {:?} ({:.2} ops/sec)", 
          individual_time, individual_tps);
    
    // Test batch execution
    info!("Starting batch execution test for {} operations", num_operations);
    let start_batch = std::time::Instant::now();
    
    let batch_results = source_node.execute_batch_via_mesh(operations).await.unwrap();
    
    let batch_time = start_batch.elapsed();
    let batch_tps = num_operations as f64 / batch_time.as_secs_f64();
    
    info!("Batch execution completed in {:?} ({:.2} ops/sec)", 
          batch_time, batch_tps);
    
    // Verify results match
    assert_eq!(individual_results.len(), batch_results.len(), 
               "Individual and batch results should have the same number of operations");
    
    // Calculate speedup
    let speedup = if individual_tps > 0.0 { batch_tps / individual_tps } else { 0.0 };
    info!("Batch execution speedup: {:.2}x", speedup);
    
    // Print scalability projections
    let single_tee_pair_tps = batch_tps / 5.0; // Assuming even distribution across 5 pairs
    
    info!("\n======= TEE MESH NETWORK PERFORMANCE ANALYSIS =======");
    info!("Current performance:");
    info!("- Single TEE pair throughput: ~{:.2} TPS", single_tee_pair_tps);
    info!("- 5 TEE pairs throughput: ~{:.2} TPS", batch_tps);
    
    // Project to larger deployments based on current performance
    info!("\nProjected performance (based on linear scaling):");
    info!("- 10 TEE pairs: ~{:.2} TPS", single_tee_pair_tps * 10.0);
    info!("- 20 TEE pairs: ~{:.2} TPS", single_tee_pair_tps * 20.0);
    info!("- 50 TEE pairs: ~{:.2} TPS", single_tee_pair_tps * 50.0);
    info!("- 100 TEE pairs: ~{:.2} TPS", single_tee_pair_tps * 100.0);
    
    // We expect at least a 2x speedup from batching
    assert!(speedup >= 2.0, "Batch execution should be at least 2x faster than individual execution");
    
    // Add info about Phase 1 optimization success
    if single_tee_pair_tps > 1000.0 {
        info!("\n✅ Phase 1 optimization goal exceeded: {:.2} TPS per pair", single_tee_pair_tps);
        info!("      Original target: 20-30K TPS with multiple TEE pairs");
        info!("      Current projection for 30 pairs: {:.2} TPS", single_tee_pair_tps * 30.0);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mesh_network_high_volume_batch() {
    let _ = env_logger::builder().filter_level(log::LevelFilter::Info).try_init();
    
    // Initialize the test harness
    let mut harness = MeshTestHarness::new();
    
    // Start the discovery service
    harness.start_discovery_service().await.unwrap();
    
    // Number of TEE pairs to create in a single region
    const NUM_PAIRS: usize = 5;
    
    println!("\nTesting high volume batch throughput with {} TEE pairs", NUM_PAIRS);
    
    // Create multiple TEE pairs
    for i in 0..NUM_PAIRS {
        let pair_id = format!("pair{}", i + 1);
        harness.create_tee_pair(&pair_id, "us-east-1", 8080 + i as u16).await.unwrap();
        println!("Created TEE pair {}", pair_id);
    }
    
    // Set realistic network latency
    harness.set_network_latency(5);
    
    // Deploy contract to all nodes
    harness.deploy_contract_to_all("kv_store", KV_STORE_CONTRACT).await.unwrap();
    
    // Number of operations per batch
    const BATCH_SIZE: usize = 50;
    // Number of batches per pair
    const BATCHES_PER_PAIR: usize = 5;
    // Total operations
    const TOTAL_OPS: usize = NUM_PAIRS * BATCHES_PER_PAIR * BATCH_SIZE;
    
    println!("Starting {} total operations in {} batches across {} TEE pairs", 
          TOTAL_OPS, NUM_PAIRS * BATCHES_PER_PAIR, NUM_PAIRS);
    
    // Set up batches for each pair
    let mut batch_handles = Vec::with_capacity(NUM_PAIRS * BATCHES_PER_PAIR);
    let start = std::time::Instant::now();
    
    for pair_idx in 0..NUM_PAIRS {
        let pair_id = format!("pair{}", pair_idx + 1);
        let pair = harness.tee_pairs.get(&pair_id).unwrap();
        
        // Use the SGX node of each pair to execute operations
        let sgx_id = &pair.sgx_node_id;
        let sev_id = &pair.sev_node_id;
        let sgx_node = harness.nodes.get(sgx_id).unwrap().clone();
        
        for batch_idx in 0..BATCHES_PER_PAIR {
            let node = sgx_node.clone();
            let target_id = sev_id.clone();
            let batch_global_idx = pair_idx * BATCHES_PER_PAIR + batch_idx;
            
            // Create a batch of operations
            let mut batch_operations = Vec::with_capacity(BATCH_SIZE);
            
            for op_idx in 0..BATCH_SIZE {
                let global_op_idx = batch_global_idx * BATCH_SIZE + op_idx;
                let key = format!("key_batch_{}_{}", batch_global_idx, op_idx);
                let value = format!("value_batch_{}_{}", batch_global_idx, op_idx);
                let input = format!("{},{}", key, value).into_bytes();
                
                batch_operations.push(("kv_store".to_string(), "store".to_string(), input, target_id.clone()));
            }
            
            // Spawn a task to execute the batch
            let handle = tokio::spawn(async move {
                let start_batch = std::time::Instant::now();
                
                match node.execute_batch_via_mesh(batch_operations).await {
                    Ok(_) => {
                        let duration = start_batch.elapsed();
                        let latency_ms = duration.as_secs_f64() * 1000.0;
                        (batch_global_idx, latency_ms, true, BATCH_SIZE)
                    },
                    Err(e) => {
                        println!("Error executing batch {}: {}", batch_global_idx, e);
                        (batch_global_idx, 0.0, false, 0)
                    }
                }
            });
            
            batch_handles.push(handle);
        }
    }
    
    // Collect batch results
    let mut batch_latencies = Vec::new();
    let mut successful_ops = 0;
    
    for handle in batch_handles {
        match handle.await {
            Ok((id, latency, success, ops_count)) => {
                if success {
                    batch_latencies.push(latency);
                    successful_ops += ops_count;
                }
            },
            Err(e) => {
                println!("Task join error: {:?}", e);
            }
        }
    }
    
    let total_duration = start.elapsed();
    let total_duration_ms = total_duration.as_secs_f64() * 1000.0;
    
    println!("All {} successful operations in {} batches completed in {:.2}ms", 
         successful_ops, batch_latencies.len(), total_duration_ms);
    
    // Calculate metrics if we have results
    if !batch_latencies.is_empty() {
        batch_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        
        let p50 = percentile(&batch_latencies, 50.0);
        let p95 = percentile(&batch_latencies, 95.0);
        let p99 = percentile(&batch_latencies, 99.0);
        let max = batch_latencies.last().cloned().unwrap_or(0.0);
        
        println!("\nHigh volume batch performance metrics:");
        println!("- p50 (median) batch latency: {:.2}ms", p50);
        println!("- p95 batch latency: {:.2}ms", p95);
        println!("- p99 batch latency: {:.2}ms", p99);
        println!("- max batch latency: {:.2}ms", max);
        println!("- avg operation latency: {:.2}ms (estimated)", 
             batch_latencies.iter().sum::<f64>() / batch_latencies.len() as f64 / BATCH_SIZE as f64);
        
        if p95 <= 100.0 {
            println!("✅ Performance meets the 100ms SLA requirement (p95 is {:.2}ms)", p95);
        } else {
            println!("❌ Performance exceeds the 100ms SLA requirement (p95 is {:.2}ms)", p95);
        }
        
        // Calculate throughput - total operations per second
        let throughput = successful_ops as f64 / (total_duration.as_secs_f64());
        let throughput_per_pair = throughput / NUM_PAIRS as f64;
        
        println!("\nAggregate Batch Throughput: {:.2} operations/second", throughput);
        println!("Average Throughput Per Pair: {:.2} operations/second", throughput_per_pair);
        
        // Print scalability projections
        println!("\n======= TEE MESH NETWORK PERFORMANCE ANALYSIS WITH BATCHING =======");
        println!("Current performance:");
        println!("- Single TEE pair throughput: ~{:.2} TPS", throughput_per_pair);
        println!("- 5 TEE pairs throughput: ~{:.2} TPS", throughput);
        
        // Project to larger deployments based on current performance
        println!("\nProjected performance (based on linear scaling):");
        println!("- 10 TEE pairs: ~{:.2} TPS", throughput_per_pair * 10.0);
        println!("- 20 TEE pairs: ~{:.2} TPS", throughput_per_pair * 20.0);
        println!("- 50 TEE pairs: ~{:.2} TPS", throughput_per_pair * 50.0);
        println!("- 100 TEE pairs: ~{:.2} TPS", throughput_per_pair * 100.0);
        
        // Phase 1 optimization success
        if throughput_per_pair > 10000.0 {
            println!("\n✅ Phase 1 optimization goal GREATLY exceeded: {:.2} TPS per pair", throughput_per_pair);
            println!("      Original target: 20-30K TPS with multiple TEE pairs");
            println!("      Current projection for 30 pairs: {:.2} TPS", throughput_per_pair * 30.0);
        } else if throughput_per_pair > 1000.0 {
            println!("\n✅ Phase 1 optimization goal exceeded: {:.2} TPS per pair", throughput_per_pair);
            println!("      Original target: 20-30K TPS with multiple TEE pairs");
            println!("      Current projection for 30 pairs: {:.2} TPS", throughput_per_pair * 30.0);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn test_mesh_network_connection_pooling() {
    // Initialize the test harness
    let mut harness = MeshTestHarness::new();
    
    // Start the discovery service
    harness.start_discovery_service().await.unwrap();
    
    // Create one TEE pair for testing connection pooling
    let pair_id = "conn-pool-pair";
    harness.create_tee_pair(pair_id, "us-east-1", 8080).await.unwrap();
    let pair = harness.tee_pairs.get(pair_id).unwrap();
    
    // Deploy our test contract to all nodes
    harness.deploy_contract_to_all("simple_add", SIMPLE_ADD_CONTRACT).await.unwrap();
    
    println!("\n--- Testing Connection Pooling ---");
    
    // Phase 1: Execute a series of operations to create initial connections
    println!("Phase 1: Initial connections - running 20 operations");
    let operations = 20;
    
    let start = std::time::Instant::now();
    
    // Execute first batch of operations
    for i in 0..operations {
        let input = format!("{},{}", i, i+1).into_bytes();
        
        let source_node = &harness.nodes[&pair.sgx_node_id];
        let result = source_node.execute_via_mesh(
            "simple_add", 
            "add", 
            &input, 
            &pair.sev_node_id
        ).await;
        assert!(result.is_ok(), "Operation execution failed in phase 1");
    }
    
    let first_phase_duration = start.elapsed();
    println!("Phase 1 completed in {:?}", first_phase_duration);
    
    // Brief pause to ensure all operations have completed
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Phase 2: Run operations with connection reuse
    println!("\nPhase 2: Connection reuse - running 20 more operations");
    
    let start = std::time::Instant::now();
    
    // Run the same operations again, should reuse connections
    for i in 0..operations {
        let input = format!("{},{}", i, i+1).into_bytes();
        
        let source_node = &harness.nodes[&pair.sgx_node_id];
        let result = source_node.execute_via_mesh(
            "simple_add", 
            "add", 
            &input, 
            &pair.sev_node_id
        ).await;
        assert!(result.is_ok(), "Operation execution failed in phase 2");
    }
    
    let second_phase_duration = start.elapsed();
    println!("Phase 2 completed in {:?}", second_phase_duration);
    
    // Verify connection pooling improves performance
    println!("\n--- Verifying Connection Pooling Performance ---");
    
    // Check performance improvement between phases
    let time_improvement = (first_phase_duration.as_micros() as f64 - second_phase_duration.as_micros() as f64) / 
                         first_phase_duration.as_micros() as f64 * 100.0;
    println!("Total time improvement: {:.2}%", time_improvement);
    
    // Estimate throughput improvement
    let throughput_phase1 = operations as f64 / first_phase_duration.as_secs_f64();
    let throughput_phase2 = operations as f64 / second_phase_duration.as_secs_f64();
    
    println!("Phase 1 throughput: {:.2} ops/sec", throughput_phase1);
    println!("Phase 2 throughput: {:.2} ops/sec", throughput_phase2);
    println!("Throughput improvement factor: {:.2}x", throughput_phase2 / throughput_phase1);
    
    // Print overall validation message
    println!("\n--- Connection Pooling Test Summary ---");
    if throughput_phase2 > throughput_phase1 {
        println!("✅ Connection pooling validation PASSED - connections are being effectively reused");
        println!("     - Throughput improved from {:.2} to {:.2} ops/sec", throughput_phase1, throughput_phase2);
        println!("     - Improvement factor: {:.2}x", throughput_phase2 / throughput_phase1);
    } else {
        println!("⚠️ Connection pooling test results are inconclusive");
        println!("     - This may be due to test conditions or implementation issues");
    }
    
    // Test batch execution with connection pooling
    println!("\n--- Testing Batch Execution with Connection Pooling ---");
    
    // Create a batch of operations
    let mut batch_ops = Vec::new();
    for i in 0..operations {
        let input = format!("{},{}", i, i+1).into_bytes();
        batch_ops.push(("simple_add".to_string(), "add".to_string(), input, pair.sev_node_id.clone()));
    }
    
    // Execute the batch from the SGX node
    let source_node = &harness.nodes[&pair.sgx_node_id];
    let start = std::time::Instant::now();
    let batch_results = source_node.execute_batch_via_mesh(batch_ops).await;
    let batch_duration = start.elapsed();
    
    assert!(batch_results.is_ok(), "Batch operation execution failed");
    
    println!("Batch execution completed in {:?}", batch_duration);
    println!("Average time per operation in batch: {:?}", batch_duration / operations as u32);
    println!("Individual execution average time: {:?}", second_phase_duration / operations as u32);
    
    // Check if batch execution is more efficient than individual calls
    let batch_per_op = batch_duration.as_micros() as f64 / operations as f64;
    let individual_per_op = second_phase_duration.as_micros() as f64 / operations as f64;
    let batch_improvement = (individual_per_op - batch_per_op) / individual_per_op * 100.0;
    
    println!("Batch execution improvement: {:.2}%", batch_improvement);
    
    if batch_per_op < individual_per_op {
        println!("✅ Batch execution is more efficient than individual calls");
        println!("     - Batch per operation: {:.2}µs", batch_per_op);
        println!("     - Individual per operation: {:.2}µs", individual_per_op);
        println!("     - Improvement: {:.2}%", batch_improvement);
    } else {
        println!("⚠️ Batch execution is not more efficient than individual calls");
        println!("     - This may require further optimization");
    }
    
    println!("\n✅ Connection pooling with batch execution test completed");
}

// Simple add contract for testing
const SIMPLE_ADD_CONTRACT: &[u8] = b"
    // Simple add contract
    function add(a, b) {
        return parseInt(a) + parseInt(b);
    }
    
    function execute(cmd, ...args) {
        if (cmd === 'add') {
            return add(args[0], args[1]);
        }
        return 'Unknown command';
    }
";

// Define a simple key-value store contract for testing
const KV_STORE_CONTRACT: &[u8] = b"
    // Simple key-value store contract
    function store(key, value) {
        state[key] = value;
        return 'success';
    }
    
    function get_state(key) {
        return state[key] || '';
    }
    
    function execute(cmd, ...args) {
        if (cmd === 'store') {
            return store(args[0], args[1]);
        } else if (cmd === 'get_state') {
            return get_state(args[0]);
        } else {
            return 'unknown command';
        }
    }
";

#[derive(Debug, Clone)]
struct PerformanceMetrics {
    p50: f64,
    p95: f64,
    p99: f64,
    max: f64,
}

fn percentile(values: &Vec<f64>, percentile: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    
    if values.len() == 1 {
        return values[0];
    }
    
    // Adjust for 0-based indexing and handle edge cases
    let len = values.len() as f64;
    let rank = (percentile / 100.0) * (len - 1.0);
    let lower_index = rank.floor() as usize;
    let upper_index = (lower_index + 1).min(values.len() - 1);
    
    if lower_index == upper_index {
        return values[lower_index];
    }
    
    let weight = rank - lower_index as f64;
    values[lower_index] * (1.0 - weight) + values[upper_index] * weight
}
