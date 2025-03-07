use std::sync::Arc;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use tokio::sync::RwLock;
use tee_interface::{TeeExecutor, ExecutionPayload, TeeError, TeeAttestation, Region, ExecutionResult, ExecutionStats, TeeType};
use wasmlanche::{
    simulator::{SimulatorExt as WasmlSimulatorExt},
    types::WasmlAddress,
    Event,
};
use uuid::Uuid;
use async_trait::async_trait;
use chrono;
use sha2::{Sha256, Digest};

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
        let result = Vec::from(format!("Executed {} with {} gas", method, gas).as_bytes());
        Box::pin(async move { Ok(result) })
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
        Self {
            simulator: Arc::new(RwLock::new(Simulator::new().await)),
            contracts: Arc::new(RwLock::new(HashMap::new())),
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
        // Get contract bytes
        let contract_code = {
            let contracts = self.contracts.read().await;
            contracts
                .get(&payload.params.id_to)
                .ok_or_else(|| TeeError::Contract("Contract not found".to_string()))?
                .clone()
        };

        // Execute contract
        let result = {
            let mut simulator = self.simulator.write().await;
            let default_actor = WasmlAddress::new([0; 32]); // Create a default actor address
            
            // Since simulator is a mutex guard, we have to extract what we need and release it
            let contract_code_vec = contract_code.clone();
            let function_call = payload.params.function_call.clone();
            let input = payload.input.clone();
            
            // Drop the mutex guard before waiting for async operation
            drop(simulator);
            
            // Create a new scope to get the simulator again
            let mut simulator = self.simulator.write().await;
            simulator.execute(
                &default_actor,
                &contract_code_vec,
                &function_call,
                &input,
                DEFAULT_GAS,
            )
            .await
            .map_err(|e| TeeError::Contract(e.to_string()))?
        };

        // Return result
        Ok(ExecutionResult {
            result,
            state_hash: vec![0; 32], // Mock state hash
            stats: ExecutionStats {
                execution_time: 0,
                memory_used: 0,
                syscall_count: 0,
            },
            attestations: vec![TeeAttestation {
                enclave_id: b"mock".to_vec(),
                measurement: vec![0; 32],
                timestamp: chrono::Utc::now().timestamp() as u64,
                signature: vec![0; 64],
                region_proof: Some(vec![0; 32]),
                data: vec![0; 32],
                enclave_type: TeeType::SGX,
            }],
            timestamp: chrono::Utc::now().to_rfc3339(),
            operation_status: None,
            operation_id: None,
            pending_operations: None,
        })
    }

    async fn deploy_contract(&self, wasm_code: &[u8], _region_id: &str) -> Result<String, TeeError> {
        // Generate contract ID
        let contract_id = Uuid::new_v4().to_string();

        // Store contract code
        {
            let mut contracts = self.contracts.write().await;
            contracts.insert(contract_id.clone(), wasm_code.to_vec());
        }

        // Deploy contract
        {
            // Make a copy of wasm_code since we'll need to drop the guard
            let wasm_code_vec = wasm_code.to_vec();
            
            let mut simulator = self.simulator.write().await;
            let default_actor = WasmlAddress::new([0; 32]); // Create a default actor address
            
            // Drop the mutex guard before waiting for async operation
            drop(simulator);
            
            // Create a new scope to get the simulator again
            let mut simulator = self.simulator.write().await;
            let _result = simulator.execute(
                &default_actor,
                &wasm_code_vec, 
                "deploy", 
                &[], 
                DEFAULT_GAS
            ).await.map_err(|e| TeeError::Contract(e.to_string()))?;
        }

        Ok(contract_id)
    }

    async fn get_state_hash(&self, _contract_address: &str) -> Result<Vec<u8>, TeeError> {
        // Just return a dummy hash for now
        Ok(vec![0; 32])
    }

    async fn get_regions(&self) -> Result<Vec<Region>, TeeError> {
        // Return a dummy region for simulator
        Ok(vec![Region {
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
            data: vec![],
            signature: vec![],
            region_proof: None,
            enclave_type: TeeType::SGX,
        }])
    }
}
