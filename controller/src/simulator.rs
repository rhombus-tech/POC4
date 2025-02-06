use std::sync::Arc;
use tokio::sync::RwLock;
use wasmlanche::simulator::Simulator as BaseSimulator;
use wasmlanche::Address;
use wasmlanche::bytemuck::Zeroable;
use tee_interface::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Default)]
pub struct WasmSimulator {
    simulator: Arc<RwLock<BaseSimulator>>,
    contract_addr: Option<Address>,
}

impl WasmSimulator {
    pub fn new() -> Self {
        Self {
            simulator: Arc::new(RwLock::new(BaseSimulator::new())),
            contract_addr: None,
        }
    }

    pub fn get_contract_addr(&self) -> Option<Address> {
        self.contract_addr.clone()
    }

    pub fn set_contract_addr(&mut self, addr: Address) {
        self.contract_addr = Some(addr);
    }

    pub async fn execute_wasm(
        &mut self,
        wasm_code: &[u8],
        function_name: &str,
        params: &[u8],
        gas: u64,
    ) -> Result<Vec<u8>, TeeError> {
        let mut simulator = self.simulator.write().await;
        
        // Create contract
        let result = simulator.create_contract(wasm_code.to_vec()).await
            .map_err(|e| TeeError::ExecutionError(format!("Contract creation failed: {}", e)))?;
        let contract_addr = result.address;

        // Call contract
        let result = simulator.call_contract(
            contract_addr,
            function_name,
            params.to_vec(),  // Convert &[u8] to Vec<u8> for BorshSerialize
            gas
        ).await
            .map_err(|e| TeeError::ExecutionError(format!("Contract execution failed: {}", e)))?;
        Ok(result)
    }

    pub async fn create_contract(&mut self, wasm_code: Vec<u8>) -> Result<Address, TeeError> {
        let mut simulator = self.simulator.write().await;
        let result = simulator.create_contract(wasm_code).await
            .map_err(|e| TeeError::ExecutionError(format!("Contract creation failed: {}", e)))?;
        Ok(result.address)
    }

    pub async fn execute_contract<T: BorshDeserialize>(
        &mut self,
        _contract_code: &[u8],
        contract_addr: Address,
        function_name: &str,
        params: &[u8],
        gas: u64,
    ) -> Result<T, TeeError> {
        let mut simulator = self.simulator.write().await;
        let result = simulator.call_contract(
            contract_addr,
            function_name,
            params.to_vec(),  // Convert &[u8] to Vec<u8> for BorshSerialize
            gas
        ).await
            .map_err(|e| TeeError::ExecutionError(format!("Contract execution failed: {}", e)))?;
        borsh::BorshDeserialize::try_from_slice(&result)
            .map_err(|e| TeeError::ExecutionError(format!("Failed to deserialize result: {}", e)))
    }

    pub async fn call_contract(
        &mut self,
        contract_addr: Address,
        function_name: &str,
        params: &[u8],
        gas: u64,
    ) -> Result<Vec<u8>, TeeError> {
        let simulator = self.simulator.clone();
        let function_name = function_name.to_string();
        let params = params.to_vec();

        let mut sim = simulator.write().await;
        let result = sim.call_contract(contract_addr, &function_name, params, gas)
            .await
            .map_err(|e| TeeError::ExecutionError(format!("Contract execution failed: {}", e)))?;

        Ok(result)
    }

    pub async fn get_balance(&self, account: Address) -> u64 {
        self.simulator.read().await.get_balance(account).await
    }

    pub async fn set_balance(&mut self, account: Address, balance: u64) {
        self.simulator.write().await.set_balance(account, balance).await;
    }
}

#[async_trait::async_trait]
impl TeeController for WasmSimulator {
    async fn init(&mut self) -> Result<(), TeeError> {
        Ok(())
    }

    async fn execute(&mut self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        let contract_addr = if let Some(addr) = self.contract_addr.clone() {
            addr
        } else {
            // Create contract if not already created
            let addr = self.create_contract(payload.input.clone()).await?;
            self.contract_addr = Some(addr.clone());
            addr
        };

        // Execute contract
        let result = self.call_contract(
            contract_addr,
            &payload.params.function_call,
            &[], // Empty params for now
            1_000_000, // Default gas limit
        ).await?;

        Ok(ExecutionResult {
            result,
            attestation: TeeAttestation {
                data: vec![1, 2, 3], // TODO: Real data
                signature: vec![1, 2, 3], // TODO: Real signature
                enclave_id: [0; 32],
                measurement: vec![4, 5, 6],
                region_proof: Some(vec![7, 8, 9]),
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                enclave_type: TeeType::SGX,
            },
            state_hash: vec![10, 11, 12], // TODO: Real state hash
            stats: ExecutionStats {
                execution_time: 0,
                memory_used: 0,
                syscall_count: 0,
            },
        })
    }

    async fn get_config(&self) -> Result<TeeConfig, TeeError> {
        Ok(TeeConfig::default())
    }

    async fn update_config(&mut self, _new_config: TeeConfig) -> Result<(), TeeError> {
        Ok(())
    }

    async fn get_attestations(&self) -> Result<Vec<TeeAttestation>, TeeError> {
        Ok(vec![])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(BorshDeserialize, BorshSerialize)]
    struct AddParams {
        a: u64,
        b: u64,
    }

    #[tokio::test]
    async fn test_simple_add() {
        let mut simulator = WasmSimulator::new();
        let wasm_code = include_bytes!("../tests/contracts/simple_add/target/wasm32-unknown-unknown/release/simple_add.wasm").to_vec();
        let contract_addr = simulator.create_contract(wasm_code.clone()).await.unwrap();
        let params = AddParams { a: 40, b: 2 };
        let params_bytes = borsh::to_vec(&params).unwrap();
        let result: u64 = simulator.execute_contract(
            &wasm_code,
            contract_addr,
            "add",
            &params_bytes,
            0
        ).await.unwrap();
        assert_eq!(result, 42);
    }
}
