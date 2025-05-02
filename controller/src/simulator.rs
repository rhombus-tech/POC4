use std::sync::Arc;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::RwLock;
use tee_interface::{TeeExecutor, ExecutionPayload, TeeError, TeeAttestation, RegionInfo, ExecutionResult, ExecutionStats, TeeType};
use wasmlanche::{
    simulator::{SimulatorExt as WasmlSimulatorExt},
    types::WasmlAddress,
    Event,
};
use uuid::Uuid;
use async_trait::async_trait;
use chrono;
use sha2::{Sha256, Digest};
use hex;

const DEFAULT_GAS: u64 = 1_000_000;

// Simulator for testing
pub struct Simulator {
    balances: HashMap<WasmlAddress, u64>,
    state: HashMap<Vec<u8>, Vec<u8>>,
    contracts: HashMap<String, Vec<u8>>,
    operations: HashMap<String, Vec<u8>>,
}

impl Simulator {
    pub async fn new() -> Self {
        Self {
            balances: HashMap::new(),
            state: HashMap::new(),
            contracts: HashMap::new(),
            operations: HashMap::new(),
        }
    }

    pub fn get_balance(&self, account: &WasmlAddress) -> u64 {
        *self.balances.get(account).unwrap_or(&0)
    }

    pub fn set_balance(&mut self, account: &WasmlAddress, balance: u64) {
        self.balances.insert(account.clone(), balance);
    }

    pub fn get_state(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.state.get(key).cloned()
    }
    
    pub fn set_state(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.state.insert(key, value);
    }

    pub async fn call_contract<U: AsRef<[u8]>>(
        &mut self, 
        target: WasmlAddress, 
        method: &str, 
        args: U,
        gas: u64
    ) -> Result<Vec<u8>, String> {
        self.execute(&target, target.as_bytes(), method, args.as_ref(), gas).await
    }

    pub fn execute<'a>(
        &'a mut self,
        _actor: &'a WasmlAddress,
        _target: &'a [u8],
        _method: &'a str,
        args: &'a [u8],
        _gas: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>> {
        let result = args.to_vec();
        Box::pin(async move { Ok(result) })
    }

    pub fn set_call_result(&mut self, _op_id: &[u8], _result: &[u8]) -> Result<(), String> {
        Ok(())
    }

    pub fn is_operation_completed(&self, _op_id: &[u8]) -> bool {
        true
    }

    pub fn remaining_fuel(&self) -> u64 {
        1_000_000
    }

    pub fn get_events(&self) -> Vec<Event> {
        Vec::new()
    }

    pub fn create_contract(&mut self, wasm_code: Vec<u8>) -> Result<WasmlAddress, String> {
        // Generate a unique contract ID
        let contract_id = Uuid::new_v4().to_string();
        // Store the contract
        self.contracts.insert(contract_id.clone(), wasm_code);
        
        // Create a deterministic address from the contract ID
        let mut hasher = Sha256::new();
        hasher.update(contract_id.as_bytes());
        let result = hasher.finalize();
        
        let mut addr = [0u8; 32];
        addr.copy_from_slice(&result[..32]);
        
        Ok(WasmlAddress::new(addr))
    }

    pub async fn create_contract_async(&mut self, wasm_code: Vec<u8>) -> Result<WasmlAddress, String> {
        self.create_contract(wasm_code)
    }
}

impl wasmlanche::Simulator for Simulator {
    fn get_balance(&self, account: &WasmlAddress) -> u64 {
        *self.balances.get(account).unwrap_or(&0)
    }

    fn set_balance(&mut self, account: &WasmlAddress, balance: u64) {
        self.balances.insert(account.clone(), balance);
    }

    fn remaining_fuel(&self) -> u64 {
        self.remaining_fuel()
    }

    fn get_events(&self) -> Vec<Event> {
        self.get_events()
    }
}

#[async_trait]
impl WasmlSimulatorExt for Simulator {
    fn get_balance_async<'a>(&'a self, account: &'a WasmlAddress) -> Pin<Box<dyn Future<Output = u64> + Send + 'a>> {
        let balance = self.get_balance(account);
        Box::pin(async move { balance })
    }
    
    fn set_balance_async<'a>(&'a mut self, account: &'a WasmlAddress, balance: u64) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            self.balances.insert(account.clone(), balance);
        })
    }

    fn store_state<'a>(&'a mut self, key: &'a [u8], value: &'a [u8]) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        let key_vec = key.to_vec();
        let value_vec = value.to_vec();
        self.set_state(key_vec, value_vec);
        Box::pin(async { })
    }

    fn get_state<'a>(&'a self, key: &'a [u8]) -> Pin<Box<dyn Future<Output = Option<Vec<u8>>> + Send + 'a>> {
        let result = self.get_state(key);
        Box::pin(async move { result })
    }

    fn delete_state<'a>(&'a mut self, key: &'a [u8]) -> Pin<Box<dyn Future<Output = Option<Vec<u8>>> + Send + 'a>> {
        let key_vec = key.to_vec();
        let previous = self.state.remove(&key_vec);
        Box::pin(async move { previous })
    }

    fn execute<'a>(
        &'a mut self,
        actor: &'a WasmlAddress,
        target: &'a [u8],
        method: &'a str,
        args: &'a [u8],
        gas: u64,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send + 'a>> {
        if method == "add" {
            // For add method, we expect comma-separated values
            let args_str = String::from_utf8_lossy(args);
            let parts: Vec<&str> = args_str.split(',').collect();
            
            if parts.len() < 2 {
                let error_msg = format!("Invalid parameters for add method. Expected 2 parameters, got {}", parts.len());
                return Box::pin(async move { Err(error_msg) });
            }
            
            match (parts[0].trim().parse::<i32>(), parts[1].trim().parse::<i32>()) {
                (Ok(a), Ok(b)) => {
                    let result = a + b;
                    let result_bytes = result.to_le_bytes().to_vec();
                    Box::pin(async move { Ok(result_bytes) })
                },
                _ => {
                    let error_msg = format!("Failed to parse parameters for add method: {:?}", parts);
                    Box::pin(async move { Err(error_msg) })
                }
            }
        } else {
            // Unsupported method
            let error_msg = format!("Unsupported method: {}", method);
            Box::pin(async move { Err(error_msg) })
        }
    }

    fn remaining_fuel_async<'a>(&'a self) -> Pin<Box<dyn Future<Output = u64> + Send + 'a>> {
        let fuel = self.remaining_fuel();
        Box::pin(async move { fuel })
    }

    fn get_events_async<'a>(&'a self) -> Pin<Box<dyn Future<Output = Vec<Event>> + Send + 'a>> {
        let events = self.get_events();
        Box::pin(async move { events })
    }
}

pub struct SimulatorController {
    simulator: Arc<RwLock<Simulator>>,
    contracts: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl SimulatorController {
    pub async fn new() -> Self {
        let simulator = Arc::new(RwLock::new(Simulator::new().await));
        let contracts = Arc::new(RwLock::new({
            let mut map = HashMap::new();
            map.insert("test-contract".to_string(), vec![0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]);
            map
        }));
        
        Self {
            simulator,
            contracts,
        }
    }

    fn create_deterministic_address(contract_bytes: &[u8]) -> WasmlAddress {
        let mut hasher = Sha256::new();
        hasher.update(contract_bytes);
        let result = hasher.finalize();
        let mut addr = [0u8; 32];
        addr.copy_from_slice(&result[..32]);
        WasmlAddress::new(addr)
    }
}

#[async_trait]
impl TeeExecutor for SimulatorController {
    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // Check if contract exists
        let contracts = self.contracts.read().await;
        if !contracts.contains_key(&payload.params.id_to) {
            return Err(TeeError::ExecutionError(format!("Contract not found: {}", payload.params.id_to)));
        }

        // Process input parameters
        let input = payload.input.clone();
        let function_call = payload.params.function_call.clone();
        
        // For simplification in our mock implementation, we'll handle specific functions
        let result = if function_call == "add" {
            // Handle the add function - expects two u32 values in byte representation
            if input.len() >= 8 {
                let num1 = u32::from_le_bytes([
                    input[0], 
                    input[1], 
                    input[2], 
                    input[3]
                ]);
                let num2 = u32::from_le_bytes([
                    input[4], 
                    input[5], 
                    input[6], 
                    input[7]
                ]);
                
                let sum = num1 + num2;
                println!("Successfully calculated {} + {} = {}", num1, num2, sum);
                sum.to_le_bytes().to_vec()
            } else {
                return Err(TeeError::ExecutionError(format!(
                    "Invalid parameters for add method. Expected 8 bytes, got {}",
                    input.len()
                )));
            }
        } else {
            // Unsupported method
            return Err(TeeError::ExecutionError(format!("Unsupported method: {}", function_call)));
        };

        // Return result
        Ok(ExecutionResult {
            result,
            state_hash: vec![0; 32], // Mock state hash
            stats: ExecutionStats {
                execution_time: 0,
                memory_used: 0,
                syscall_count: 0,
                network_latency: 0,
                custom_metrics: None,
            },
            attestations: vec![TeeAttestation {
                enclave_id: b"simulator".to_vec(),
                measurement: vec![0; 32],
                timestamp: chrono::Utc::now().timestamp() as u64,
                data: vec![0; 32],
                signature: vec![0; 64],
                region_proof: Some(vec![]),
                // Choose TDX for AI workloads or SGX for other workloads
                enclave_type: if payload.input.len() > 1024 || 
                                (payload.operation_context.is_some() && 
                                 payload.operation_context.as_ref().unwrap().len() > 1024) { 
                    TeeType::TDX // Use TDX for larger workloads (likely AI compute)
                } else {
                    TeeType::SGX // Use SGX for standard workloads
                },
            }],
            timestamp: chrono::Utc::now().to_rfc3339(),
            operation_status: None,
            operation_id: None,
            pending_operations: None,
        })
    }

    async fn deploy_contract(&self, wasm_code: &[u8], _region_id: &str) -> Result<String, TeeError> {
        // Generate contract ID using hash for consistency with EnarxController
        let mut hasher = Sha256::new();
        hasher.update(wasm_code);
        let contract_id = hex::encode(hasher.finalize());

        // Store contract code
        {
            let mut contracts = self.contracts.write().await;
            contracts.insert(contract_id.clone(), wasm_code.to_vec());
        }

        // Log the deployment
        println!("Deployed contract with ID: {}", contract_id);
        println!("Contract code size: {} bytes", wasm_code.len());

        Ok(contract_id)
    }

    async fn get_state_hash(&self, _contract_address: &str) -> Result<Vec<u8>, TeeError> {
        // Just return a dummy hash for now
        Ok(vec![0; 32])
    }

    async fn get_regions(&self) -> Result<Vec<RegionInfo>, TeeError> {
        // Return a dummy region for simulator
        Ok(vec![RegionInfo {
            id: "simulator".to_string(),
            worker_ids: vec!["local-worker".to_string()],
            max_tasks: 10,
        }])
    }

    async fn get_attestations(&self, _region_id: &str) -> Result<Vec<TeeAttestation>, TeeError> {
        // Return a dummy attestation
        Ok(vec![TeeAttestation {
            enclave_id: b"simulator".to_vec(),
            measurement: vec![0; 32],
            timestamp: chrono::Utc::now().timestamp() as u64,
            data: vec![0; 32],
            signature: vec![0; 64],
            region_proof: Some(vec![]),
            // Choose TDX for batch operations (like in AI trading scenarios) or SGX for standard ops
            enclave_type: TeeType::TDX, // Support high-throughput AI trading scenarios
        }])
    }
}
