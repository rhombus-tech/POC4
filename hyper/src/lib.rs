use std::error::Error;
use tee_interface::{ExecutionPayload, ExecutionParams, ExecutionResult, TeeController, TeeAttestation, TeeError};

pub struct HyperExecutor {
    controller: Box<dyn TeeController>,
}

impl HyperExecutor {
    pub fn new(controller: Box<dyn TeeController>) -> Self {
        Self { controller }
    }

    pub async fn execute_wasm(
        &self,
        _region_id: String,
        wasm_bytes: Vec<u8>,
        function: String,
        args: Vec<u8>,
        tx_id: Vec<u8>,
    ) -> Result<ExecutionResult, Box<dyn Error>> {
        // Serialize the WASM execution data
        let mut input_data = Vec::new();
        input_data.extend_from_slice(&wasm_bytes);
        input_data.extend_from_slice(&function.into_bytes());
        input_data.extend_from_slice(&args);
        input_data.extend_from_slice(&tx_id);

        let payload = ExecutionPayload {
            execution_id: 0, // TODO: Generate proper execution ID
            input: input_data,
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: true,
            },
        };
        
        // Execute in TEE
        self.controller.execute(&payload)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    struct MockTeeController {
        expected_result: ExecutionResult,
    }

    #[async_trait]
    impl TeeController for MockTeeController {
        async fn init(&mut self) -> Result<(), TeeError> {
            Ok(())
        }

        async fn execute(&self, payload: &ExecutionPayload) -> Result<ExecutionResult, TeeError> {
            // Verify the payload contains our expected data
            let input = &payload.input;
            assert!(input.len() > 0, "Input should not be empty");
            
            Ok(self.expected_result.clone())
        }

        async fn get_config(&self) -> Result<tee_interface::TeeConfig, TeeError> {
            Ok(tee_interface::TeeConfig::default())
        }

        async fn update_config(&mut self, _config: tee_interface::TeeConfig) -> Result<(), TeeError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_execute_wasm() {
        // Create mock result
        let expected_result = ExecutionResult {
            result: vec![1, 2, 3],
            attestation: TeeAttestation {
                enclave_id: [0u8; 32],
                measurement: vec![],
                data: vec![],
                signature: vec![],
                region_proof: None,
            },
            state_hash: vec![4, 5, 6],
            stats: tee_interface::ExecutionStats {
                execution_time: 100,
                memory_used: 1024,
                syscall_count: 5,
            },
        };

        // Create mock controller
        let controller = MockTeeController {
            expected_result: expected_result.clone(),
        };

        // Create executor
        let executor = HyperExecutor::new(Box::new(controller));

        // Execute test WASM
        let result = executor.execute_wasm(
            "test-region".to_string(),
            vec![1, 2, 3], // Mock WASM bytes
            "test_function".to_string(),
            vec![4, 5, 6], // Mock args
            vec![7, 8, 9], // Mock tx_id
        ).await.unwrap();

        // Verify result matches
        assert_eq!(result.result, expected_result.result);
        assert_eq!(result.state_hash, expected_result.state_hash);
        assert_eq!(result.stats.execution_time, expected_result.stats.execution_time);
        assert_eq!(result.stats.memory_used, expected_result.stats.memory_used);
        assert_eq!(result.stats.syscall_count, expected_result.stats.syscall_count);
    }
}
