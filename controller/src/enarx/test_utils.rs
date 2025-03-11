use std::path::{Path, PathBuf};
use std::fs;
use std::env;
use std::process::Command;
use log::{info, error, warn};
use crate::enarx::{EnarxController, error::EnarxError};
use tee_interface::{ExecutionPayload, ExecutionParams, ExecutionResult, TeeExecutor, TeeAttestation, TeeType, TeeError};
use std::sync::Arc;
use tokio::sync::RwLock;
use hex;

// Simple WASM program that adds two integers
// This is a minimal WebAssembly module in hex format that exports an "add" function
pub const TEST_WASM_HEX: &str = "0061736d0100000001070160027f7f017f03020100070a0106036164640000\
                               0a09010700200020016a0b";

/// Utility functions for testing Enarx functionality
pub struct EnarxTester {
    enarx_path: String,
    temp_dir: PathBuf,
    use_simulation: bool,
}

impl EnarxTester {
    /// Create a new EnarxTester
    pub fn new() -> Self {
        let enarx_path = env::var("ENARX_PATH").unwrap_or_else(|_| "enarx".to_string());
        let temp_dir = env::temp_dir().join("enarx_test");
        
        Self {
            enarx_path,
            temp_dir,
            use_simulation: false,
        }
    }
    
    /// Create a new EnarxTester with simulation mode
    pub fn new_with_simulation() -> Self {
        let mut tester = Self::new();
        tester.use_simulation = true;
        tester
    }
    
    /// Initialize the test environment
    pub fn initialize(&self) -> Result<(), EnarxError> {
        info!("Initializing Enarx test environment");
        
        // Create temp directory if it doesn't exist
        if !self.temp_dir.exists() {
            fs::create_dir_all(&self.temp_dir)
                .map_err(|e| EnarxError::Other(format!("Failed to create temp directory: {}", e)))?;
        }
        
        // In simulation mode, we don't need to verify the Enarx binary
        if !self.use_simulation {
            // Verify the Enarx binary exists and is executable
            if !Path::new(&self.enarx_path).exists() {
                return Err(EnarxError::KeepManagerError(
                    format!("Enarx binary not found at {}", self.enarx_path)
                ));
            }
            
            // Check if we can run the enarx binary
            let output = Command::new(&self.enarx_path)
                .arg("--version")
                .output()
                .map_err(|e| EnarxError::KeepManagerError(format!("Failed to execute Enarx: {}", e)))?;
            
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(EnarxError::KeepManagerError(format!("Enarx failed: {}", stderr)));
            }
            
            info!("Enarx binary validated successfully: {}", self.enarx_path);
        } else {
            info!("Running in simulation mode - no Enarx binary required");
        }
        
        Ok(())
    }
    
    /// Create a test WASM file
    pub fn create_test_wasm(&self) -> Result<PathBuf, EnarxError> {
        let wasm_path = self.temp_dir.join("test.wasm");
        
        // Convert hex to bytes
        let wasm_bytes = hex::decode(TEST_WASM_HEX.replace("\n", "").replace(" ", ""))
            .map_err(|e| EnarxError::Other(format!("Failed to decode WASM hex: {}", e)))?;
        
        // Write to file
        fs::write(&wasm_path, wasm_bytes)
            .map_err(|e| EnarxError::Other(format!("Failed to write WASM file: {}", e)))?;
        
        info!("Created test WASM file at: {}", wasm_path.display());
        
        Ok(wasm_path)
    }
    
    /// Test executing a WASM file
    pub async fn test_execute(&self) -> Result<(), EnarxError> {
        // Create test WASM file
        let wasm_path = self.create_test_wasm()?;
        
        info!("Testing execution of WASM file: {}", wasm_path.display());
        
        if self.use_simulation {
            self.test_execute_simulation(&wasm_path).await?;
        } else {
            self.test_execute_real(&wasm_path)?;
        }
        
        info!("Execution test completed successfully");
        Ok(())
    }
    
    /// Test executing a WASM file with real Enarx
    fn test_execute_real(&self, wasm_path: &Path) -> Result<(), EnarxError> {
        // Execute the WASM file with Enarx
        let output = Command::new(&self.enarx_path)
            .arg("run")
            .arg(wasm_path)
            .output()
            .map_err(|e| EnarxError::KeepManagerError(format!("Failed to execute Enarx: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Enarx execution failed: {}", stderr);
            return Err(EnarxError::KeepManagerError(format!("Enarx execution failed: {}", stderr)));
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        info!("Enarx execution successful: {}", stdout);
        
        Ok(())
    }
    
    /// Test executing a WASM file in simulation mode using EnarxController
    async fn test_execute_simulation(&self, wasm_path: &Path) -> Result<(), EnarxError> {
        info!("Executing in simulation mode");
        
        // Create an EnarxController in simulation mode
        let config_dir = self.temp_dir.to_string_lossy().to_string();
        let controller = EnarxController::new(
            TeeType::SGX,
            &config_dir,
            true // Use simulation mode
        ).await.map_err(|e| EnarxError::Other(format!("Failed to create controller: {}", e)))?;
        
        // Read the WASM file
        let wasm_bytes = fs::read(wasm_path)
            .map_err(|e| EnarxError::Other(format!("Failed to read WASM file: {}", e)))?;
        
        // Deploy the contract
        let contract_id = controller.deploy_contract(&wasm_bytes, "default").await
            .map_err(|e| EnarxError::Other(format!("Failed to deploy contract: {}", e)))?;
        
        info!("Contract deployed with ID: {}", contract_id);
        
        // Parameters for the add function: 5 + 7
        let input_params = vec![5, 0, 0, 0, 7, 0, 0, 0];
        
        // Create execution payload
        let payload = ExecutionPayload {
            params: ExecutionParams {
                id_to: contract_id.clone(),
                function_call: "add".to_string(),
                detailed_proof: false,
                expected_hash: vec![],
            },
            input: input_params.clone(),
            operation_id: Some("test_operation".to_string()),
            previous_operation_id: None,
            operation_context: None,
        };
        
        // Execute the contract
        let result = controller.execute(&payload).await
            .map_err(|e| EnarxError::Other(format!("Failed to execute contract: {}", e)))?;
        
        info!("Contract execution result: {:?}", result.result);
        
        // Verify the result (should be 12 for 5+7)
        let expected_result = 12u32.to_le_bytes().to_vec();
        
        if result.result != expected_result {
            return Err(EnarxError::Other(format!(
                "Unexpected result: {:?}, expected: {:?}",
                result.result, expected_result
            )));
        }
        
        info!("Contract execution result verified successfully");
        Ok(())
    }
    
    /// Cleanup test environment
    pub fn cleanup(&self) -> Result<(), EnarxError> {
        info!("Cleaning up test environment");
        if self.temp_dir.exists() {
            fs::remove_dir_all(&self.temp_dir)
                .map_err(|e| EnarxError::Other(format!("Failed to remove temp directory: {}", e)))?;
        }
        Ok(())
    }
}
