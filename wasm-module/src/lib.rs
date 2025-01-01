use wasm_bindgen::prelude::*;
use tee_interface::prelude::*;
use sallyport::guest::{self, Platform};

#[wasm_bindgen]
pub struct TeeModule {
    // Platform for host communication
    platform: Platform,
    // Current execution state
    state: ExecutionState,
}

#[wasm_bindgen]
impl TeeModule {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<TeeModule, JsValue> {
        Ok(TeeModule {
            platform: Platform::default(),
            state: ExecutionState::Pending,
        })
    }

    /// Main execution entry point
    pub fn execute(&mut self, input: &[u8]) -> Result<Vec<u8>, JsValue> {
        self.state = ExecutionState::Running;

        // Deserialize the input payload
        let payload: ExecutionPayload = borsh::from_slice(input)
            .map_err(|e| JsError::new(&format!("Failed to deserialize payload: {}", e)))?;

        // Execute the computation
        let result = self.compute(payload.input)?;

        // Get attestation
        let attestation = self.get_attestation()?;

        // Create execution result with proof
        let execution_result = ExecutionResult {
            result_hash: self.compute_hash(&result),
            result,
            attestation,
        };

        // Serialize the result
        let serialized = borsh::to_vec(&execution_result)
            .map_err(|e| JsError::new(&format!("Failed to serialize result: {}", e)))?;

        self.state = ExecutionState::Completed;
        Ok(serialized)
    }

    /// Get attestation from the platform
    fn get_attestation(&self) -> Result<AttestationReport, JsValue> {
        let measurement = self.platform.get_measurement()
            .map_err(|e| JsError::new(&format!("Failed to get measurement: {}", e)))?;

        Ok(AttestationReport {
            enclave_type: self.get_platform_type(),
            measurement,
            timestamp: self.get_timestamp()?,
            platform_data: self.get_platform_data()?,
        })
    }

    /// Perform the actual computation
    fn compute(&self, input: Vec<u8>) -> Result<Vec<u8>, JsValue> {
        // Here we would implement the actual computation logic
        // For now, just return the input as output
        Ok(input)
    }

    fn get_platform_type(&self) -> EnclaveType {
        match self.platform.platform_type() {
            guest::PlatformType::Sgx => EnclaveType::IntelSGX,
            guest::PlatformType::Sev => EnclaveType::AMDSEV,
            _ => panic!("Unsupported platform type"),
        }
    }

    fn get_timestamp(&self) -> Result<u64, JsValue> {
        self.platform.get_time()
            .map_err(|e| JsError::new(&format!("Failed to get timestamp: {}", e)))
    }

    fn get_platform_data(&self) -> Result<Vec<u8>, JsValue> {
        // Get platform-specific data for attestation
        // This would include things like TCB levels, etc.
        Ok(Vec::new())
    }

    fn compute_hash(&self, data: &[u8]) -> [u8; 32] {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn test_module_initialization() {
        let module = TeeModule::new();
        assert!(module.is_ok());
    }

    #[wasm_bindgen_test]
    fn test_execution() {
        let mut module = TeeModule::new().unwrap();
        
        let payload = ExecutionPayload {
            execution_id: 1,
            input: vec![1, 2, 3],
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: false,
            },
        };

        let input = borsh::to_vec(&payload).unwrap();
        let result = module.execute(&input);
        assert!(result.is_ok());
    }
}