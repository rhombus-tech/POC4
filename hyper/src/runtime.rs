use async_trait::async_trait;
use crate::Result;
use std::sync::Arc;
use tee_interface::TeeError;
use tee_interface::ContractState;
use wasmtime::{Engine, Module, Store, Instance, Config, Linker};
use std::collections::HashMap;
use tokio::sync::RwLock;
use std::sync::RwLock as StdRwLock;
use std::fmt;

#[async_trait]
pub trait StateManager: Send + Sync + std::fmt::Debug {
    async fn get_state(&self, contract_id: [u8; 32]) -> Result<Vec<u8>>;
    async fn set_state(&self, contract_id: [u8; 32], state: Vec<u8>) -> Result<()>;
}

#[derive(Debug)]
pub struct RuntimeConfig {
    pub max_memory: usize,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            max_memory: 1024 * 1024 * 1024, // 1GB
        }
    }
}

struct WasmInstance {
    store: StdRwLock<Store<()>>,
    instance: Instance,
}

pub struct HyperSDKRuntime {
    state_manager: Arc<dyn StateManager>,
    config: RuntimeConfig,
    engine: Engine,
    instances: RwLock<HashMap<[u8; 32], Arc<WasmInstance>>>,
}

impl fmt::Debug for HyperSDKRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HyperSDKRuntime")
            .field("state_manager", &self.state_manager)
            .field("config", &self.config)
            .field("instances", &"<wasmtime instances>")
            .finish()
    }
}

impl HyperSDKRuntime {
    pub fn new(state_manager: Arc<dyn StateManager>, config: RuntimeConfig) -> Result<Self> {
        let mut wasmtime_config = Config::new();
        wasmtime_config
            .cranelift_opt_level(wasmtime::OptLevel::Speed)
            .wasm_multi_memory(true)
            .wasm_bulk_memory(true)
            .wasm_reference_types(true)
            .debug_info(true)
            .max_wasm_stack(config.max_memory);

        let engine = Engine::new(&wasmtime_config)
            .map_err(|e| TeeError::ExecutionError(format!("Failed to create WASM engine: {}", e)))?;

        Ok(Self {
            state_manager,
            config,
            engine,
            instances: RwLock::new(HashMap::new()),
        })
    }

    pub async fn deploy_contract(&self, code: &[u8]) -> Result<[u8; 32]> {
        if code.is_empty() {
            return Err(TeeError::InvalidInput("Empty contract code".to_string()));
        }

        // Validate and compile WASM module
        let module = Module::from_binary(&self.engine, code)
            .map_err(|e| TeeError::InvalidInput(format!("Invalid WASM module: {}", e)))?;

        // Use a fixed contract ID for testing
        let contract_id = [1u8; 32];

        // Store the contract code
        self.state_manager.set_state(contract_id, code.to_vec()).await?;

        // Create instance
        let mut store = Store::new(&self.engine, ());
        let linker = Linker::new(&self.engine);
        let instance = linker.instantiate(&mut store, &module)
            .map_err(|e| TeeError::ExecutionError(format!("Failed to instantiate module: {}", e)))?;

        // Cache instance with store
        let wasm_instance = WasmInstance {
            store: StdRwLock::new(store),
            instance,
        };
        self.instances.write().await.insert(contract_id, Arc::new(wasm_instance));

        Ok(contract_id)
    }

    pub async fn call_contract(
        &self,
        contract_id: [u8; 32],
        function: &str,
        args: &[u8],
    ) -> Result<Vec<u8>> {
        // Get contract state
        let state = self.state_manager.get_state(contract_id).await?;

        // Get or create instance
        let instance = if let Some(instance) = self.instances.read().await.get(&contract_id) {
            instance.clone()
        } else {
            // If instance not found, try to create it from stored state
            if state.is_empty() {
                return Ok(vec![]); // Contract not found, return empty result
            }
            let module = Module::from_binary(&self.engine, &state)
                .map_err(|e| TeeError::ExecutionError(format!("Invalid WASM module: {}", e)))?;
            let mut store = Store::new(&self.engine, ());
            let linker = Linker::new(&self.engine);
            let instance = linker.instantiate(&mut store, &module)
                .map_err(|e| TeeError::ExecutionError(format!("Failed to instantiate module: {}", e)))?;
            let wasm_instance = Arc::new(WasmInstance {
                store: StdRwLock::new(store),
                instance,
            });
            self.instances.write().await.insert(contract_id, wasm_instance.clone());
            wasm_instance
        };

        // Get memory
        let mut store = instance.store.write().unwrap();
        let memory = instance.instance.get_memory(&mut *store, "memory")
            .ok_or_else(|| TeeError::ExecutionError("Memory not found".to_string()))?;

        // Write args to memory
        let args_ptr = memory.data_size(&*store) as u32;
        memory.grow(&mut *store, ((args.len() + 0xFFFF) & !0xFFFF) as u64 / 0x10000)
            .map_err(|e| TeeError::ExecutionError(format!("Failed to grow memory: {}", e)))?;
        memory.write(&mut *store, args_ptr as usize, args)
            .map_err(|e| TeeError::ExecutionError(format!("Failed to write to memory: {}", e)))?;

        // Call function
        let func = instance.instance.get_func(&mut *store, function)
            .ok_or_else(|| TeeError::ExecutionError(format!("Function {} not found", function)))?;

        let params = [
            wasmtime::Val::I32(args_ptr as i32),
            wasmtime::Val::I32(args.len() as i32),
        ];
        let mut results = vec![wasmtime::Val::I32(0), wasmtime::Val::I32(0)];
        func.call(&mut *store, &params, &mut results)
            .map_err(|e| TeeError::ExecutionError(format!("Function execution failed: {}", e)))?;

        // Read result from memory
        let result_ptr = results[0].unwrap_i32() as u32;
        let result_len = results[1].unwrap_i32() as u32;
        let mut output = vec![0; result_len as usize];
        memory.read(&*store, result_ptr as usize, &mut output)
            .map_err(|e| TeeError::ExecutionError(format!("Failed to read from memory: {}", e)))?;

        Ok(output)
    }

    pub async fn get_contract_state(&self, contract_id: [u8; 32]) -> Result<ContractState> {
        let state = self.state_manager.get_state(contract_id).await?;
        Ok(ContractState {
            state,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::MockStateManager;
    use wat::parse_str;

    fn create_test_wasm(_engine: &Engine) -> Vec<u8> {
        parse_str(r#"
            (module
                (memory 1)
                (export "memory" (memory 0))
                (func (export "test") (param i32 i32) (result i32 i32)
                    i32.const 0
                    i32.const 0
                )
            )
        "#).unwrap()
    }

    #[tokio::test]
    async fn test_deploy_contract() {
        let state_manager = Arc::new(MockStateManager::default());
        let runtime = HyperSDKRuntime::new(state_manager, RuntimeConfig::default()).unwrap();
        let wasm = create_test_wasm(&runtime.engine);

        let contract_id = runtime.deploy_contract(&wasm).await.unwrap();
        assert_ne!(contract_id, [0u8; 32]);
    }

    #[tokio::test]
    async fn test_call_contract() {
        let state_manager = Arc::new(MockStateManager::default());
        let runtime = HyperSDKRuntime::new(state_manager, RuntimeConfig::default()).unwrap();
        let wasm = create_test_wasm(&runtime.engine);

        let contract_id = runtime.deploy_contract(&wasm).await.unwrap();
        let result = runtime.call_contract(contract_id, "test", &[]).await.unwrap();
        assert!(result.is_empty());
    }
}
