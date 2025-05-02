// Path is already imported in the std::path::{Path, PathBuf} line below
use std::process::Command;
use std::process::Child;
// use std::io::Write; // Not needed after refactoring
use log::{debug, info, error, warn};
use crate::enarx::error::EnarxError;
use tee_interface::TeeError;
use tee_interface::TeeType;
use std::env;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;
use which::which;

/// Status of a keep in the pool
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepStatus {
    /// Keep is initialized and ready for use
    Ready,
    /// Keep is being used for execution
    InUse,
    /// Keep is being initialized
    Initializing,
    /// Keep has failed and should be removed
    Failed,
}

/// A representation of an Enarx keep process
#[derive(Debug)]
pub struct Keep {
    /// Unique ID for this keep
    pub id: String,
    /// Status of this keep
    pub status: KeepStatus,
    /// Time when this keep was created
    pub created_at: Instant,
    /// Time when this keep was last used
    pub last_used: Option<Instant>,
    /// Process handle for the keep
    pub process: Option<Child>,
    /// TEE type of this keep
    pub tee_type: String,
    /// Flag indicating if this keep is currently in use
    pub in_use: bool,
}

impl Keep {
    /// Create a new keep with the given ID
    pub fn new(id: String, tee_type: String) -> Self {
        Self {
            id,
            status: KeepStatus::Initializing,
            created_at: Instant::now(),
            last_used: None,
            process: None,
            tee_type,
            in_use: false, // Initialize as not in use
        }
    }

    /// Check if this keep is available for use
    pub fn is_available(&self) -> bool {
        self.status == KeepStatus::Ready
    }

    /// Mark this keep as in use
    pub fn mark_in_use(&mut self) {
        self.status = KeepStatus::InUse;
        self.in_use = true; // Set the in_use flag
        self.last_used = Some(Instant::now());
    }
    
    /// Mark this keep as no longer in use
    pub fn mark_not_in_use(&mut self) {
        self.in_use = false;
        self.status = KeepStatus::Ready;
    }

    /// Mark this keep as ready for use
    pub fn mark_ready(&mut self) {
        self.status = KeepStatus::Ready;
    }

    /// Mark this keep as failed
    pub fn mark_failed(&mut self) {
        self.status = KeepStatus::Failed;
    }
}

/// Configuration for the Enarx Keep Manager
#[derive(Debug, Clone)]
pub struct KeepManagerConfig {
    /// Path to the Enarx binary
    pub enarx_path: String,
    /// Maximum number of simultaneous keeps
    pub max_keeps: usize,
    /// Number of warm keeps to maintain
    pub warm_keeps: usize,
    /// Maximum lifetime of a keep in seconds
    pub keep_max_lifetime: u64,
    /// Maximum idle time of a keep in seconds
    pub keep_max_idle_time: u64,
    /// Initialization timeout in milliseconds
    pub init_timeout_ms: u64,
}

impl Default for KeepManagerConfig {
    fn default() -> Self {
        Self {
            enarx_path: env::var("ENARX_PATH")
                .unwrap_or_else(|_| "/usr/local/bin/enarx".to_string()),
            max_keeps: 10,
            warm_keeps: 2,
            keep_max_lifetime: 3600, // 1 hour
            keep_max_idle_time: 300, // 5 minutes
            init_timeout_ms: 5000,   // 5 seconds
        }
    }
}

/// Manages a pool of Enarx keeps for efficient execution
pub struct KeepManager {
    config: KeepManagerConfig,
    enarx_path: String,
    /// Pool of available keeps
    keeps: Arc<RwLock<HashMap<String, Keep>>>,
    /// Queue of keeps that are ready for use
    ready_queue: Arc<Mutex<VecDeque<String>>>,
    /// Is the manager initialized
    initialized: Arc<RwLock<bool>>,
}

impl KeepManager {
    /// Create a new Keep Manager with the specified configuration
    pub fn new(config: KeepManagerConfig) -> Self {
        let enarx_path = match which("enarx") {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => config.enarx_path.clone(),
        };
        
        Self { 
            config, 
            enarx_path,
            keeps: Arc::new(RwLock::new(HashMap::new())),
            ready_queue: Arc::new(Mutex::new(VecDeque::new())),
            initialized: Arc::new(RwLock::new(false)),
        }
    }
    
    /// Initialize the Keep Manager
    pub async fn initialize(&self) -> Result<(), EnarxError> {
        let mut initialized = self.initialized.write().await;
        if *initialized {
            debug!("Keep Manager is already initialized");
            return Ok(());
        }
        
        info!("Initializing Keep Manager with max_keeps={}, warm_keeps={}", 
              self.config.max_keeps, self.config.warm_keeps);
        
        if self.config.enarx_path.is_empty() {
            return Err(EnarxError::KeepManagerError("Enarx binary path not set".to_string()));
        }
        
        // Verify the Enarx binary exists and is executable
        if !Path::new(&self.config.enarx_path).exists() {
            return Err(EnarxError::KeepManagerError(
                format!("Enarx binary not found at {}", self.config.enarx_path)
            ));
        }
        
        // Check if we can run a simple command
        let output = Command::new(&self.config.enarx_path)
            .arg("--version")
            .output()
            .map_err(|e| EnarxError::KeepManagerError(format!("Failed to execute Enarx: {}", e)))?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(EnarxError::KeepManagerError(format!("Enarx failed: {}", stderr)));
        }
        
        info!("Enarx binary validated successfully at {}", self.config.enarx_path);
        
        // Start the background task for managing warm keeps
        self.start_keep_manager_task();
        
        // Initialize warm keeps
        self.ensure_warm_keeps().await?;
        
        *initialized = true;
        Ok(())
    }
    
    /// Start a background task to manage the pool of keeps
    fn start_keep_manager_task(&self) {
        let keeps = self.keeps.clone();
        let ready_queue = self.ready_queue.clone();
        let config = self.config.clone();
        let enarx_path = self.enarx_path.clone();
        let initialized = self.initialized.clone();
        
        tokio::spawn(async move {
            info!("Starting keep manager background task");
            let mut interval = tokio::time::interval(Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                // Only run if initialized
                if !*initialized.read().await {
                    continue;
                }
                
                // Clean up stale keeps
                Self::cleanup_stale_keeps(&keeps, &ready_queue, &config).await;
                
                // Ensure we have enough warm keeps
                if let Err(e) = Self::replenish_warm_keeps(&keeps, &ready_queue, &config, &enarx_path).await {
                    error!("Failed to replenish warm keeps: {}", e);
                }
            }
        });
    }
    
    /// Clean up keeps that have been running too long or idle too long
    async fn cleanup_stale_keeps(
        keeps: &Arc<RwLock<HashMap<String, Keep>>>,
        ready_queue: &Arc<Mutex<VecDeque<String>>>,
        config: &KeepManagerConfig,
    ) {
        let now = Instant::now();
        let mut keeps_to_remove = Vec::new();
        
        // Find keeps to remove
        {
            let keeps_guard = keeps.read().await;
            for (id, keep) in keeps_guard.iter() {
                // Remove keeps that have been running too long
                if now.duration_since(keep.created_at).as_secs() > config.keep_max_lifetime {
                    keeps_to_remove.push(id.clone());
                    continue;
                }
                
                // Remove keeps that have been idle too long
                if let Some(last_used) = keep.last_used {
                    if keep.status == KeepStatus::Ready && 
                       now.duration_since(last_used).as_secs() > config.keep_max_idle_time {
                        keeps_to_remove.push(id.clone());
                    }
                }
                
                // Remove failed keeps
                if keep.status == KeepStatus::Failed {
                    keeps_to_remove.push(id.clone());
                }
            }
        }
        
        // Remove keeps
        if !keeps_to_remove.is_empty() {
            info!("Cleaning up {} stale keeps", keeps_to_remove.len());
            let mut keeps_guard = keeps.write().await;
            let mut ready_queue_guard = ready_queue.lock().unwrap();
            
            for id in keeps_to_remove {
                if let Some(mut keep) = keeps_guard.remove(&id) {
                    // Kill the process if it's still running
                    if let Some(mut process) = keep.process.take() {
                        debug!("Killing keep process for {}", id);
                        let _ = process.kill();
                    }
                    
                    // Remove from ready queue
                    ready_queue_guard.retain(|keep_id| keep_id != &id);
                }
            }
        }
    }
    
    /// Ensure we have enough warm keeps
    async fn replenish_warm_keeps(
        keeps: &Arc<RwLock<HashMap<String, Keep>>>,
        ready_queue: &Arc<Mutex<VecDeque<String>>>,
        config: &KeepManagerConfig,
        enarx_path: &str,
    ) -> Result<(), EnarxError> {
        let warm_keeps_count = {
            let ready_queue_guard = ready_queue.lock().unwrap();
            ready_queue_guard.len()
        };
        
        let needed = config.warm_keeps.saturating_sub(warm_keeps_count);
        if needed == 0 {
            return Ok(());
        }
        
        info!("Replenishing warm keeps: {} needed", needed);
        
        // Check if we can create more keeps
        let current_keeps_count = {
            let keeps_guard = keeps.read().await;
            keeps_guard.len()
        };
        
        if current_keeps_count >= config.max_keeps {
            warn!("Cannot create more keeps: reached max_keeps ({})", config.max_keeps);
            return Ok(());
        }
        
        // Create new keeps
        let to_create = std::cmp::min(needed, config.max_keeps - current_keeps_count);
        for _ in 0..to_create {
            // Create a new keep
            let keep_id = Uuid::new_v4().to_string();
            let mut keep = Keep::new(keep_id.clone(), "sgx".to_string());
            
            // Initialize the keep
            match Self::initialize_keep(&mut keep, enarx_path).await {
                Ok(()) => {
                    keep.mark_ready();
                    
                    // Add to keeps map
                    let mut keeps_guard = keeps.write().await;
                    keeps_guard.insert(keep_id.clone(), keep);
                    
                    // Add to ready queue
                    let mut ready_queue_guard = ready_queue.lock().unwrap();
                    ready_queue_guard.push_back(keep_id.clone());
                    
                    info!("Created new warm keep: {}", keep_id);
                }
                Err(e) => {
                    error!("Failed to initialize keep: {}", e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Initialize a new keep
    async fn initialize_keep(keep: &mut Keep, enarx_path: &str) -> Result<(), EnarxError> {
        debug!("Initializing keep {}", keep.id);
        
        // In a real implementation, we would start an Enarx process ready to accept commands
        // For simulation purposes, we'll use a simple sleep to simulate initialization
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        // Simulate the process handle
        // In a real implementation, this would be an actual subprocess
        keep.process = None;
        
        Ok(())
    }
    
    /// Ensure we have enough warm keeps
    pub async fn ensure_warm_keeps(&self) -> Result<(), EnarxError> {
        Self::replenish_warm_keeps(
            &self.keeps,
            &self.ready_queue,
            &self.config,
            &self.enarx_path,
        ).await
    }
    
    /// Get a keep from the pool
    pub async fn get_keep(&self) -> Result<String, EnarxError> {
        // Make sure we're initialized
        if !*self.initialized.read().await {
            return Err(EnarxError::KeepManagerError("Keep Manager not initialized".to_string()));
        }
        
        // Try to get a keep from the ready queue
        let keep_id = {
            let mut ready_queue_guard = self.ready_queue.lock().unwrap();
            ready_queue_guard.pop_front()
        };
        
        if let Some(id) = keep_id {
            // Mark the keep as in use
            let mut keeps_guard = self.keeps.write().await;
            if let Some(keep) = keeps_guard.get_mut(&id) {
                keep.mark_in_use();
                debug!("Got keep {} from pool", id);
                return Ok(id);
            }
        }
        
        // No keep available, check if we can create a new one
        let current_keeps_count = {
            let keeps_guard = self.keeps.read().await;
            keeps_guard.len()
        };
        
        if current_keeps_count >= self.config.max_keeps {
            return Err(EnarxError::KeepManagerError("No keeps available and at max capacity".to_string()));
        }
        
        // Create a new keep
        let keep_id = Uuid::new_v4().to_string();
        let mut keep = Keep::new(keep_id.clone(), "sgx".to_string());
        
        // Initialize the keep
        Self::initialize_keep(&mut keep, &self.enarx_path).await?;
        
        // Mark as in use and add to keeps map
        keep.mark_in_use();
        
        let mut keeps_guard = self.keeps.write().await;
        keeps_guard.insert(keep_id.clone(), keep);
        
        info!("Created new on-demand keep: {}", keep_id);
        Ok(keep_id)
    }
    
    /// Return a keep to the pool
    pub async fn return_keep(&self, keep_id: &str) -> Result<(), EnarxError> {
        let mut keeps_guard = self.keeps.write().await;
        
        if let Some(keep) = keeps_guard.get_mut(keep_id) {
            keep.mark_ready();
            
            // Add back to ready queue
            let mut ready_queue_guard = self.ready_queue.lock().unwrap();
            ready_queue_guard.push_back(keep_id.to_string());
            
            debug!("Returned keep {} to pool", keep_id);
            Ok(())
        } else {
            Err(EnarxError::KeepManagerError(format!("Keep {} not found", keep_id)))
        }
    }
    
    /// Execute a WebAssembly contract in an Enarx keep
    pub async fn execute<P: AsRef<Path>>(&self, contract_path: P, params: &[u8]) -> Result<Vec<u8>, TeeError> {
        // Validate params to prevent exploits
        if params.len() == 0 {
            return Err(TeeError::ValidationError("Empty parameters provided".to_string()));
        }
        
        const MAX_REASONABLE_PARAMS_SIZE: usize = 10 * 1024 * 1024; // 10MB
        if params.len() > MAX_REASONABLE_PARAMS_SIZE {
            return Err(TeeError::ValidationError(format!("Parameter size too large: {} bytes", params.len())));
        }
        
        // Support both direct and length-prefixed formats
        let validated_params = if params.len() >= 4 {
            let length_bytes = [params[0], params[1], params[2], params[3]];
            let length = u32::from_le_bytes(length_bytes) as usize;
            
            if length > 0 && length <= MAX_REASONABLE_PARAMS_SIZE && params.len() >= length + 4 {
                // Length-prefixed format
                &params[4..length+4]
            } else {
                // Direct format
                params
            }
        } else {
            // Direct format
            params
        };
        
        info!("Executing contract {} with {} bytes of parameters", contract_path.as_ref().display(), params.len());
        
        // Get a keep from the pool
        let keep_id = self.get_keep().await?;
        
        let keeps = self.keeps.read().await;
        let keep = keeps.get(&keep_id).ok_or_else(|| TeeError::ExecutionError("Keep not found".to_string()))?;
        
        // Determine appropriate backend based on keep's TEE type
        let backend = match keep.tee_type.as_str() {
            "SGX" => "sgx",
            "SEV" => "sev",
            "TDX" => "tdx",
            _ => "sgx" // Default to SGX for backward compatibility
        };
        
        // Execute the contract
        let result = self.execute_with_keep(&keep_id, contract_path, validated_params).await;
        
        // Return the keep to the pool
        if let Err(e) = self.return_keep(&keep_id).await {
            error!("Failed to return keep to pool: {}", e);
        }
        
        // Return the result
        result
    }

    /// Execute a WebAssembly contract in a specific keep with robust parameter handling
    /// 
    /// This method implements the security-first architecture with dual-format parameter handling
    /// supporting both length-prefixed and direct data formats
    async fn execute_with_keep<P: AsRef<Path>>(&self, keep_id: &str, contract_path: P, params: &[u8]) -> Result<Vec<u8>, TeeError> {
        let contract_path = contract_path.as_ref();
        debug!("Executing contract {} in keep {}", contract_path.display(), keep_id);
        
        // Validate the keep ID
        let keeps = self.keeps.read().await;
        let keep = keeps.get(keep_id).ok_or_else(|| TeeError::ExecutionError(format!("Keep {} not found", keep_id)))?;
        
        // Ensure the keep is in use (should be marked as such by get_keep)
        if !keep.in_use {
            return Err(TeeError::ExecutionError(format!("Keep {} is not marked as in use", keep_id)));
        }
        
        // Validate parameters again to ensure security
        let validated_params = if params.len() >= 4 {
            let length_bytes = [params[0], params[1], params[2], params[3]];
            let length = u32::from_le_bytes(length_bytes) as usize;
            
            const MAX_REASONABLE_PARAMS_SIZE: usize = 10 * 1024 * 1024; // 10MB
            if length > 0 && length <= MAX_REASONABLE_PARAMS_SIZE && params.len() >= length + 4 {
                // Length-prefixed format
                debug!("Using length-prefixed format with length {}", length);
                &params[4..length+4]
            } else {
                // Direct format (fallback)
                debug!("Using direct format, length prefix appears invalid");
                params
            }
        } else {
            // Direct format (too short for length prefix)
            debug!("Using direct format, too short for length prefix");
            params
        };
        
        // Create a WASI config file with the parameters
        let config_path = self.create_wasi_config(validated_params)?;
        
        // Determine the appropriate backend based on the keep's TEE type
        let backend = match keep.tee_type.as_str() {
            "SGX" => "sgx",
            "SEV" => "sev",
            "TDX" => "tdx",
            _ => "sgx" // Default to SGX for backward compatibility
        };
        
        // Build the command to execute
        let result = Command::new(&self.config.enarx_path)
            .arg("run")
            .arg("--backend")
            .arg(backend)
            .arg("--wasmcfgfile")
            .arg(&config_path)
            .arg(contract_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| TeeError::ExecutionError(format!("Failed to spawn Enarx: {}", e)))?;
        
        // Wait for the command to complete and collect output
        let output = result.wait_with_output()
            .map_err(|e| TeeError::ExecutionError(format!("Failed to wait for Enarx: {}", e)))?;
        
        // Check for successful execution
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Enarx execution failed: {}", stderr);
            return Err(TeeError::ExecutionError(format!("Enarx execution failed: {}", stderr)));
        }
        
        debug!("Enarx execution completed successfully, received {} bytes of output", output.stdout.len());
        Ok(output.stdout)
    }
    
    /// Determine if a WASM module is a WASI module
    pub fn is_wasi_module(&self, function_call: &str, params: &[u8]) -> bool {
        // Check for WASI module indicators in function call
        if function_call.starts_with("wasi_") || function_call.contains("::wasi::") {
            return true;
        }
        
        // Check file extension indicators in params if they represent a path
        if let Ok(param_str) = std::str::from_utf8(params) {
            if param_str.ends_with(".wasi") || param_str.contains(".wasi.") {
                return true;
            }
        }
        
        // Detect WASI by checking for common WASI imports in binary (simplified version)
        if params.len() > 16 {  // Minimum size for WebAssembly module
            // Simple check for WASI module - look for WASI imports section pattern
            // This is a simplified check; a real implementation would parse the WebAssembly binary
            let mut i = 0;
            while i < params.len() - 10 {
                // Look for common WASI imports like "wasi_snapshot_preview1"
                if params[i..].starts_with(b"wasi_") {
                    return true;
                }
                i += 1;
            }
        }
        
        false
    }
    
    /// Execute a WASI module in an Enarx keep with TDX support
    /// 
    /// This method specifically handles WASI modules with appropriate WASI runtime flags
    pub async fn execute_wasi<P: AsRef<Path>>(&self, contract_path: P, params: &[u8], tee_type: TeeType) -> Result<Vec<u8>, TeeError> {
        debug!("Executing WASI module {} with {} bytes of parameters", contract_path.as_ref().display(), params.len());
        
        // Do parameter validation to prevent exploits
        if params.len() > 10 * 1024 * 1024 { // 10MB max
            return Err(TeeError::ValidationError(format!("Parameter size too large: {} bytes", params.len())));
        }
        
        // Support both direct and length-prefixed parameter formats
        let validated_params = self.validate_parameters(params)?;
        
        // Get keep with appropriate backend type
        let keep_id = self.get_keep().await?;
        
        // Execute with WASI flags
        let result = self.execute_wasi_in_keep(&keep_id, contract_path, validated_params, tee_type).await;
        
        // Return the keep to the pool
        if let Err(e) = self.return_keep(&keep_id).await {
            warn!("Failed to return keep to pool: {}", e);
        }
        
        result
    }
    
    /// Execute a WASI module in a specific keep with TDX support
    async fn execute_wasi_in_keep<P: AsRef<Path>>(&self, keep_id: &str, contract_path: P, params: &[u8], tee_type: TeeType) -> Result<Vec<u8>, TeeError> {
        debug!("Executing WASI module in keep {}", keep_id);
        
        // Get the keep
        let mut keep_lock = self.keeps.write().await;
        let keep = match keep_lock.get_mut(keep_id) {
            Some(k) => k,
            None => return Err(TeeError::ExecutionError(format!("Keep not found: {}", keep_id))),
        };
        
        if keep.status != KeepStatus::Ready {
            return Err(TeeError::ExecutionError(format!("Keep not ready: {}", keep_id)));
        }
        
        // Mark the keep as in use
        keep.mark_in_use();
        
        // Store a temporary copy of the config for WASI execution
        let config_path = self.create_wasi_config(params)?;
        
        // Determine the appropriate backend based on TEE type
        let backend = match tee_type {
            TeeType::SGX => "sgx",
            TeeType::SEV => "sev",
            TeeType::TDX => "tdx",
        };
        
        // Execute with Enarx using the WASI runtime
        let output = Command::new(&self.config.enarx_path)
            .arg("run")
            .arg("--backend")
            .arg(backend)
            .arg("--wasmcfgfile")
            .arg(&config_path)
            .arg("--wasi")  // Enable WASI runtime
            .arg(contract_path.as_ref())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| TeeError::ExecutionError(format!("Failed to execute Enarx: {}", e)))?;
        
        // Clean up temporary config
        if let Err(e) = std::fs::remove_file(&config_path) {
            warn!("Failed to remove temporary config file: {}", e);
        }
        
        // Handle the result
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("Enarx WASI execution failed: {}", stderr);
            return Err(TeeError::ExecutionError(format!("Enarx WASI execution failed: {}", stderr)));
        }
        
        debug!("WASI execution completed successfully, received {} bytes of output", output.stdout.len());
        Ok(output.stdout)
    }
    
    /// Create a temporary WASI configuration file
    fn create_wasi_config(&self, params: &[u8]) -> Result<PathBuf, TeeError> {
        let config_dir = std::env::temp_dir().join("enarx-wasi-configs");
        std::fs::create_dir_all(&config_dir)
            .map_err(|e| TeeError::ExecutionError(format!("Failed to create temp config directory: {}", e)))?;
        
        let config_path = config_dir.join(format!("wasi-config-{}.json", uuid::Uuid::new_v4()));
        
        // Create a basic WASI config that includes parameters
        let config = serde_json::json!({
            "wasi": {
                "env": {
                    "PARAMETER_SIZE": params.len().to_string(),
                },
                "preopened_dirs": ["/tmp"],
                "mapped_dirs": {}
            }
        });
        
        std::fs::write(&config_path, config.to_string())
            .map_err(|e| TeeError::ExecutionError(format!("Failed to write config file: {}", e)))?;
        
        Ok(config_path)
    }
    
    /// Validate parameters to prevent exploits
    fn validate_parameters<'a>(&self, params: &'a [u8]) -> Result<&'a [u8], TeeError> {
        if params.is_empty() {
            return Err(TeeError::ValidationError("Empty parameters".to_string()));
        }
        
        const MAX_REASONABLE_PARAMS_SIZE: usize = 10 * 1024 * 1024; // 10MB
        if params.len() > MAX_REASONABLE_PARAMS_SIZE {
            return Err(TeeError::ValidationError(format!("Parameter size too large: {} bytes", params.len())));
        }
        
        // Support both direct and length-prefixed formats (with dual-format support)
        let validated_params = if params.len() >= 4 {
            let length_bytes = [params[0], params[1], params[2], params[3]];
            let length = u32::from_le_bytes(length_bytes) as usize;
            
            if length > 0 && length <= MAX_REASONABLE_PARAMS_SIZE && params.len() >= length + 4 {
                // Length-prefixed format
                &params[4..length+4]
            } else {
                // Direct format (fallback)
                params
            }
        } else {
            // Direct format (too short for length prefix)
            params
        };
        
        Ok(validated_params)
    }

    /// Execute a WebAssembly contract using the actual Enarx binary
    ///
    /// This is not used in the simulation, but shows how it would be implemented
    /// with the real Enarx binary
    pub async fn execute_with_enarx<P: AsRef<Path>>(&self, contract_path: P, params: &[u8]) -> Result<Vec<u8>, TeeError> {
        let contract_path = contract_path.as_ref();
        debug!("Executing contract {} with Enarx binary", contract_path.display());
        
        // Get a keep from the pool
        let keep_id = self.get_keep().await?;
        
        // Validate parameters with dual-format support
        let validated_params = self.validate_parameters(params)?;
        
        // Execute the contract using the keep
        let result = self.execute_with_keep(&keep_id, contract_path, validated_params).await;
        
        // Return the keep to the pool regardless of execution result
        if let Err(e) = self.return_keep(&keep_id).await {
            error!("Failed to return keep to pool: {}", e);
            // Don't fail the execution if we just couldn't return the keep
        }
        
        // Return the execution result
        result
    }
    
}
