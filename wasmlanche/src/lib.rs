use anyhow::Result;
use wasmi::{Engine, Linker, Module, Store, Func, Caller};

pub mod simulator {
    use super::*;

    pub struct WasmSimulator {
        engine: Engine,
        linker: Linker<()>,
        result: Option<Vec<u8>>,
    }

    impl WasmSimulator {
        pub fn new() -> Self {
            let mut config = wasmi::Config::default();
            config
                .wasm_multi_value(true)
                .wasm_mutable_global(true)
                .wasm_sign_extension(true)
                .wasm_bulk_memory(true)
                .wasm_reference_types(true);
                
            let engine = Engine::new(&config);
            let mut linker = Linker::new(&engine);
            
            // Define memory for the WASM module
            linker.define(
                "",
                "memory",
                wasmi::Memory::new(&engine, wasmi::MemoryType::new(1, Some(1024)).unwrap()).unwrap(),
            ).unwrap();
            
            // Define host function for setting call result
            linker.define(
                "contract",
                "set_call_result",
                Func::wrap(&engine, |mut caller: Caller<'_, ()>, ptr: i32, len: i32| {
                    let memory = caller
                        .get_export("memory")
                        .and_then(|e| e.into_memory())
                        .expect("Failed to get memory");
                    
                    let mut result = vec![0; len as usize];
                    memory.read(caller.as_context_mut(), ptr as usize, &mut result)
                        .expect("Failed to read memory");
                        
                    // Store result in simulator
                    if let Some(data) = caller.data_mut().downcast_mut::<Option<Vec<u8>>>() {
                        *data = Some(result);
                    }
                }),
            ).unwrap();
            
            Self {
                engine,
                linker,
                result: None,
            }
        }

        pub fn execute(&mut self, code: &[u8], function: &str, args: &[u8]) -> Result<Vec<u8>> {
            // Parse WASM module
            let module = Module::new(&self.engine, code)?;
            
            // Create store with mutable reference to result
            let mut store = Store::new(&self.engine, &mut self.result);
            
            // Instantiate module
            let instance = self.linker.instantiate(&mut store, &module)?;
            
            // First call init() if it exists
            if let Ok(init_fn) = instance.get_typed_func::<(), ()>(&mut store, "init") {
                init_fn.call(&mut store, ())?;
            }
            
            // Get function with correct signature (i32) -> ()
            let func = instance.get_typed_func::<i32, ()>(&mut store, function)?;
            
            // Create a memory instance to store args
            let memory = instance.get_memory(&mut store, "memory")
                .ok_or_else(|| Error::ExecutionError("No memory export found".into()))?;
            
            // Write args to memory
            let args_ptr = 1024; // Start at page boundary
            memory.write(&mut store, args_ptr as usize, args)?;
            
            // Call function with pointer to args
            func.call(&mut store, args_ptr)?;
            
            // Return the result that was set via set_call_result
            Ok(self.result.take().unwrap_or_default())
        }
    }
}
