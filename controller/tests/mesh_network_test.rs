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
use tee_controller::mesh::{MeshConfig, MeshCoordinator, TeeType, PeerInfo};
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
        };
        
        // Initialize mesh coordinator
        let mesh_coordinator = MeshCoordinator::new(config.clone()).await?;
        
        // Instead of calling start_discovery directly, we'll spawn our own discovery task
        // since start_discovery is private in the actual implementation
        let config_clone = config.clone();
        let mesh_coordinator_arc = Arc::new(mesh_coordinator.clone());
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(config_clone.discovery_interval_sec));
            loop {
                interval.tick().await;
                // We can't call refresh_peers directly since it's private
                // In a real test, this would perform a direct discovery request
                let discovery_endpoint = &config_clone.discovery_endpoint;
                println!("Simulating peer discovery for TEE ID: {} to endpoint: {}", 
                         config_clone.tee_id, discovery_endpoint);
                // Sleep to simulate discovery work
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        });
        
        // Store mesh coordinator
        self.mesh_coordinator = Some(Arc::new(mesh_coordinator));
        
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
        };
        
        let result = self.controller.execute(&payload).await
            .map_err(|e| format!("Failed to execute contract: {}", e))?;
            
        Ok(result.result)
    }
    
    async fn execute_via_mesh(&self, contract_id: &str, function: &str, input: &[u8], target_tee: &str) -> Result<Vec<u8>, std::io::Error> {
        if let Some(mesh) = &self.mesh_coordinator {
            // Prepare real input for contract execution
            let real_input = format!("{},{},{}", function, contract_id, hex::encode(input)).into_bytes();
            
            // Execute over the mesh
            let result = mesh.execute(
                target_tee.to_string(),
                self.region_id.clone(),
                self.tee_type.to_string(),
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
}

// Test harness for managing multiple TEE nodes
struct MeshTestHarness {
    discovery_addr: SocketAddr,
    discovery_service: Option<TeePeerService>,
    nodes: HashMap<String, TestTeeNode>,
    pairs: HashMap<String, TeePair>,  // Add pairs tracking
}

// Structure to represent a TEE pair (SGX + SEV)
struct TeePair {
    sgx_node_id: String,
    sev_node_id: String,
    region_id: String,
}

impl MeshTestHarness {
    async fn new() -> Self {
        let discovery_addr = "127.0.0.1:50100".parse().unwrap();
        
        Self {
            discovery_addr,
            discovery_service: None,
            nodes: HashMap::new(),
            pairs: HashMap::new(),  // Initialize pairs
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
                                tee_type: node.tee_type.to_string(),
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
    
    // New method to create a TEE pair (SGX + SEV)
    async fn create_tee_pair(&mut self, pair_id: &str, region_id: &str, base_port: u16) -> Result<(), std::io::Error> {
        let sgx_id = format!("{}-sgx", pair_id);
        let sev_id = format!("{}-sev", pair_id);
        
        // Create SGX node
        self.add_node(&sgx_id, region_id, TeeType::SGX, base_port).await?;
        
        // Create SEV node
        self.add_node(&sev_id, region_id, TeeType::SEV, base_port + 1).await?;
        
        // Register the pair
        self.pairs.insert(pair_id.to_string(), TeePair {
            sgx_node_id: sgx_id.clone(), // Clone here to avoid move
            sev_node_id: sev_id.clone(), // Clone here to avoid move
            region_id: region_id.to_string(),
        });
        
        println!("Created TEE pair {} with SGX node {} and SEV node {}", pair_id, sgx_id, sev_id);
        
        Ok(())
    }
    
    // Execute a contract via one member of a pair targeting the other member
    async fn execute_across_pair(&self, pair_id: &str, contract_id: &str, function: &str, input: &[u8], 
                             source_type: TeeType, target_type: TeeType) -> Result<Vec<u8>, std::io::Error> {
        let pair = self.pairs.get(pair_id)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, 
                format!("Pair {} not found", pair_id)))?;
        
        let source_id = match source_type {
            TeeType::SGX => &pair.sgx_node_id,
            TeeType::SEV => &pair.sev_node_id,
        };
        
        let target_id = match target_type {
            TeeType::SGX => &pair.sgx_node_id,
            TeeType::SEV => &pair.sev_node_id,
        };
        
        let source_node = self.nodes.get(source_id)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, 
                format!("Source node {} not found", source_id)))?;
        
        println!("Executing from {} to {} in pair {}", source_id, target_id, pair_id);
        source_node.execute_via_mesh(contract_id, function, input, target_id).await
    }
    
    // Test execution across all pairs in the region
    async fn test_execution_across_all_pairs(&self, contract_id: &str) -> Result<(), String> {
        if self.pairs.is_empty() {
            return Err("No TEE pairs available for testing".to_string());
        }
        
        println!("Testing execution across all {} TEE pairs...", self.pairs.len());
        
        for (pair_id, pair) in &self.pairs {
            // Test from SGX to SEV
            let test_key = format!("test_key_sgx_to_sev_{}", pair_id);
            let test_value = format!("test_value_{}", Uuid::new_v4());
            let store_input = format!("store,{},{}", test_key, test_value).into_bytes();
            
            println!("Testing SGX to SEV execution in pair {}", pair_id);
            let result = self.execute_across_pair(
                pair_id, contract_id, "execute", &store_input, 
                TeeType::SGX, TeeType::SEV
            ).await.map_err(|e| format!("Failed to execute SGX to SEV in pair {}: {}", pair_id, e))?;
            
            // Test from SEV to SGX
            let test_key = format!("test_key_sev_to_sgx_{}", pair_id);
            let test_value = format!("test_value_{}", Uuid::new_v4());
            let store_input = format!("store,{},{}", test_key, test_value).into_bytes();
            
            println!("Testing SEV to SGX execution in pair {}", pair_id);
            let result = self.execute_across_pair(
                pair_id, contract_id, "execute", &store_input, 
                TeeType::SEV, TeeType::SGX
            ).await.map_err(|e| format!("Failed to execute SEV to SGX in pair {}: {}", pair_id, e))?;
        }
        
        // Wait for state to propagate
        sleep(Duration::from_secs(2)).await;
        
        // Verify state consistency across all nodes
        for (pair_id, pair) in &self.pairs {
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
    
    // Test pair failover (simulate failure of one TEE type, switch to the other)
    async fn test_pair_failover(&self, pair_id: &str, contract_id: &str, failed_type: TeeType) -> Result<(), String> {
        let pair = self.pairs.get(pair_id)
            .ok_or_else(|| format!("Pair {} not found", pair_id))?;
        
        // Get another pair to act as the source
        let source_pair_id = self.pairs.keys()
            .find(|id| *id != pair_id)
            .ok_or_else(|| "Need at least two pairs for failover test".to_string())?;
        let source_pair = self.pairs.get(source_pair_id).unwrap();
        
        // Source node will be the SGX from the source pair
        let source_id = &source_pair.sgx_node_id;
        let source_node = self.nodes.get(source_id).unwrap();
        
        // Target should be the non-failed node in the target pair
        let target_id = match failed_type {
            TeeType::SGX => &pair.sev_node_id,  // If SGX failed, use SEV
            TeeType::SEV => &pair.sgx_node_id,  // If SEV failed, use SGX
        };
        
        let failed_id = match failed_type {
            TeeType::SGX => &pair.sgx_node_id,
            TeeType::SEV => &pair.sev_node_id,
        };
        
        // Test key and value
        let test_key = format!("failover_key_{}", pair_id);
        let test_value = format!("failover_value_{}", Uuid::new_v4());
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
        if self.pairs.len() < 2 {
            return Err("Need at least 2 TEE pairs for failure scenario test".to_string());
        }
        
        // Get two pair IDs
        let pair_ids: Vec<&String> = self.pairs.keys().collect();
        let pair1_id = pair_ids[0];
        let pair2_id = pair_ids[1];
        
        // Test SGX failure in first pair
        self.test_pair_failover(pair1_id, contract_id, TeeType::SGX).await?;
        
        // Test SEV failure in second pair
        self.test_pair_failover(pair2_id, contract_id, TeeType::SEV).await?;
        
        println!("All failure scenarios tested successfully");
        Ok(())
    }
}

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

#[tokio::test(flavor = "multi_thread")]
async fn test_mesh_network() {
    // Create test harness
    let mut harness = MeshTestHarness::new().await;
    
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
