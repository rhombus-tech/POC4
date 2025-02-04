use std::error::Error;
use std::sync::Arc;

use tee_interface::{ExecutionInput, ExecutionResult, TeeController};

pub struct HyperExecutor {
    controller: Box<dyn TeeController>,
}

impl HyperExecutor {
    pub fn new(controller: Box<dyn TeeController>) -> Self {
        Self { controller }
    }

    pub async fn execute_wasm(
        &self,
        region_id: String,
        wasm_bytes: Vec<u8>,
        function: String,
        args: Vec<u8>,
        tx_id: Vec<u8>,
    ) -> Result<ExecutionResult, Box<dyn Error>> {
        let input = ExecutionInput {
            wasm_bytes,
            function,
            args,
            tx_id,
        };
        
        // Always require attestation for hypersdk integration
        self.controller.execute(region_id, input, true).await
    }

    pub async fn health_check(&self) -> Result<bool, Box<dyn Error>> {
        self.controller.health_check().await
    }
}
