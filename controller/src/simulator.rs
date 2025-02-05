use std::{error::Error, sync::Mutex, time::Instant};
use async_trait::async_trait;
use tee_interface::prelude::*;
use wasmlanche::simulator::{Simulator as WasmVM, SimpleState};

pub struct WasmSimulator {
    pub enclave_id: [u8; 32],
    state: Mutex<SimpleState>,
}

impl WasmSimulator {
    pub fn new() -> Self {
        Self {
            enclave_id: [0u8; 32],
            state: Mutex::new(SimpleState::new()),
        }
    }

    pub async fn execute_wasm(&self, code: &[u8], function: &str, args: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
        // Write WASM code to a temporary file
        let temp_dir = tempfile::tempdir()?;
        let wasm_path = temp_dir.path().join("contract.wasm");
        std::fs::write(&wasm_path, code)?;

        // Create contract and execute using wasmlanche simulator
        let mut state = self.state.lock().unwrap();
        let vm = WasmVM::new(&mut *state);
        let contract_result = vm.create_contract(wasm_path.to_str().unwrap())?;
        let result = vm.call_contract(contract_result.address, function, args, 1_000_000)?;
        Ok(result)
    }
}

#[async_trait]
impl TeeController for WasmSimulator {
    async fn init(&mut self) -> Result<(), TeeError> {
        // No initialization needed for simulator
        Ok(())
    }

    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        // Extract WASM code, function name, and args from payload
        let input = &payload.input;
        let (wasm_code, rest) = input.split_at(32); // First 32 bytes are WASM code
        let (function_name, args) = rest.split_at(32); // Next 32 bytes are function name

        // Execute the WASM code
        let start = Instant::now();
        let result = self.execute_wasm(wasm_code, &String::from_utf8_lossy(function_name), args)
            .await
            .map_err(|e| TeeError::ExecutionError(format!("Failed to execute WASM: {}", e)))?;
        let execution_time = start.elapsed().as_millis() as u64;

        Ok(ExecutionResult {
            result,
            attestation: TeeAttestation {
                enclave_id: self.enclave_id,
                measurement: vec![],
                data: vec![],
                signature: vec![],
                region_proof: None,
            },
            state_hash: vec![0; 32], // Mock state hash
            stats: ExecutionStats {
                execution_time,
                memory_used: 1024,
                syscall_count: 5,
            },
        })
    }

    async fn get_config(&self) -> Result<TeeConfig, TeeError> {
        Ok(TeeConfig::default())
    }

    async fn update_config(&mut self, _config: TeeConfig) -> Result<(), TeeError> {
        Ok(())
    }
}
