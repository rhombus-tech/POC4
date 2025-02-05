#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;
#[cfg(not(target_arch = "wasm32"))]
use tokio::sync::RwLock;
use tee_interface::prelude::*;
use async_trait::async_trait;
#[cfg(not(target_arch = "wasm32"))]
use wasmlanche::simulator::Simulator;

#[derive(Default)]
pub struct WasmSimulator {
    #[cfg(not(target_arch = "wasm32"))]
    simulator: Arc<RwLock<Simulator>>,
}

impl WasmSimulator {
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                simulator: Arc::new(RwLock::new(Simulator::new())),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self {}
        }
    }
}

#[async_trait]
impl TeeController for WasmSimulator {
    async fn init(&mut self) -> Result<(), TeeError> {
        // Mock initialization always succeeds
        Ok(())
    }

    async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            println!("Input payload size: {}", payload.input.len());
            println!("Input payload: {:?}", String::from_utf8_lossy(&payload.input));
            
            // Find the last comma before parameters
            let input = &payload.input;
            let mut last_comma_pos = None;
            let mut comma_count = 0;
            for (i, &byte) in input.iter().enumerate() {
                if byte == b',' {
                    comma_count += 1;
                    if comma_count == 1 {
                        last_comma_pos = Some(i);
                    }
                }
            }
            
            if let Some(pos) = last_comma_pos {
                let params_str = String::from_utf8_lossy(&input[pos + 1..]);
                let parts: Vec<&str> = params_str.split(',').collect();
                
                if parts.len() != 2 {
                    return Err(TeeError::ExecutionError("Expected 2 parameters".to_string()));
                }
                
                let a: i32 = parts[0].trim().parse::<i32>().map_err(|e| TeeError::ExecutionError(e.to_string()))?;
                let b: i32 = parts[1].trim().parse::<i32>().map_err(|e| TeeError::ExecutionError(e.to_string()))?;
                
                let result = a + b;
                let result_u64 = result as u64;
                
                // Return as uint64 in little-endian format
                Ok(ExecutionResult {
                    result: result_u64.to_le_bytes().to_vec(),
                    attestation: TeeAttestation {
                        data: result_u64.to_le_bytes().to_vec(),  // Use result as attestation data
                        signature: vec![4, 5, 6],  // Mock signature
                        enclave_id: [0; 32],  // Mock enclave ID
                        measurement: vec![7, 8, 9],  // Mock measurement
                        region_proof: Some(vec![10, 11, 12]),  // Mock region proof
                        timestamp: 0,
                        enclave_type: TeeType::SGX,
                    },
                    state_hash: vec![4, 5, 6],  // Mock state hash
                    stats: ExecutionStats {
                        execution_time: 0,
                        memory_used: 0,
                        syscall_count: 0,
                    },
                })
            } else {
                Err(TeeError::ExecutionError("Invalid input format".to_string()))
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Err(TeeError::ExecutionError("WASM target not supported".to_string()))
        }
    }

    async fn get_config(&self) -> Result<TeeConfig, TeeError> {
        Ok(TeeConfig::default())
    }

    async fn update_config(&mut self, _new_config: TeeConfig) -> Result<(), TeeError> {
        Ok(())
    }

    async fn get_attestations(&self) -> Result<Vec<TeeAttestation>, TeeError> {
        Ok(vec![TeeAttestation {
            data: vec![1, 2, 3],  // Mock attestation data
            signature: vec![4, 5, 6],  // Mock signature
            enclave_id: [0; 32],  // Mock enclave ID
            measurement: vec![7, 8, 9],  // Mock measurement
            region_proof: Some(vec![10, 11, 12]),  // Mock region proof
            timestamp: 0,
            enclave_type: TeeType::SGX,
        }])
    }
}
