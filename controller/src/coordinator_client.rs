use reqwest;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tee_interface::{TeeError, ExecutionResult, ExecutionParams};
use reqwest::Client;
use tokio::time::timeout;
use tokio::sync::RwLock;
use std::sync::Arc;
use std::collections::HashMap;
use base64;
use uuid::Uuid;

// Import the ExecutionRequest from proto
use crate::proto::teeservice::ExecutionRequest;

// Define the types we need to interact with the coordinator
// These match the Go types in the coordinator

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub worker_ids: Vec<String>,
    pub data: Vec<u8>,
    pub attestations: Vec<Vec<u8>>,
    pub timeout: u64, // Duration in milliseconds
    pub region_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub task: Task,
    pub status: String,
    pub start_time: String,
    pub end_time: String,
    pub error: Option<String>,
    pub results: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub id: String,
    pub enclave_id: Vec<u8>,
    pub status: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TEEPair {
    pub primary_worker_id: String,
    pub secondary_worker_id: String,
    pub attestations: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorResponse {
    pub success: bool,
    pub error: Option<String>,
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoordinatorExecutionResult {
    pub success: bool,
    pub error: Option<String>,
    pub result: Option<ExecutionResult>, 
}

// Client to communicate with the coordinator
pub struct CoordinatorClient {
    base_url: String,
    client: reqwest::Client,
    worker_id: String,
    enclave_id: Vec<u8>,
    task_cache: Arc<RwLock<HashMap<String, TaskInfo>>>,
}

impl CoordinatorClient {
    pub fn new(
        coordinator_url: &str,
        worker_id: Option<&str>,
        enclave_id: Option<&[u8]>
    ) -> Self {
        // Create a client with reasonable timeout defaults
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        // If worker_id is not provided, generate a unique one
        let worker_id = worker_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| Uuid::new_v4().to_string());

        // If enclave_id is not provided, use a placeholder
        let enclave_id = enclave_id
            .map(|id| id.to_vec())
            .unwrap_or_else(|| b"mock-enclave-id".to_vec());

        Self {
            base_url: coordinator_url.to_string(),
            client,
            worker_id,
            enclave_id,
            task_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    // Register this worker with the coordinator
    pub async fn register_worker(&self) -> Result<(), String> {
        let url = format!("{}/workers/register", self.base_url);
        
        let response = self.client
            .post(&url)
            .json(&serde_json::json!({
                "id": self.worker_id,
                "enclave_id": self.enclave_id,
            }))
            .send()
            .await
            .map_err(|e| format!("Failed to register worker: {}", e))?;

        let response_body: CoordinatorResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if response_body.success {
            Ok(())
        } else {
            Err(response_body.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    // Register a TEE pair with the coordinator
    pub async fn register_tee_pair(
        &self,
        region_id: &str,
        secondary_worker_id: &str,
        attestations: Vec<Vec<u8>>,
    ) -> Result<(), String> {
        let url = format!("{}/regions/{}/pairs/register", self.base_url, region_id);
        
        let tee_pair = TEEPair {
            primary_worker_id: self.worker_id.clone(),
            secondary_worker_id: secondary_worker_id.to_string(),
            attestations,
        };
        
        let response = self.client
            .post(&url)
            .json(&tee_pair)
            .send()
            .await
            .map_err(|e| format!("Failed to register TEE pair: {}", e))?;

        let response_body: CoordinatorResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if response_body.success {
            Ok(())
        } else {
            Err(response_body.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    // Submit a task to the coordinator
    pub async fn submit_task(
        &self,
        data: Vec<u8>,
        region_id: &str,
        worker_ids: Vec<String>,
        attestations: Vec<Vec<u8>>,
    ) -> Result<String, String> {
        let url = format!("{}/tasks/submit", self.base_url);
        
        let task = Task {
            id: Uuid::new_v4().to_string(),
            worker_ids,
            data,
            attestations,
            timeout: 30000, // 30 seconds in milliseconds
            region_id: region_id.to_string(),
        };
        
        let response = self.client
            .post(&url)
            .json(&task)
            .send()
            .await
            .map_err(|e| format!("Failed to submit task: {}", e))?;

        let response_body: CoordinatorResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if response_body.success {
            // Parse the task ID from the response
            let task_id = response_body.data
                .as_ref()
                .and_then(|data| data.get("task_id"))
                .and_then(|id| id.as_str())
                .ok_or_else(|| "Missing task ID in response".to_string())?;
            
            Ok(task_id.to_string())
        } else {
            Err(response_body.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    // Get task status from the coordinator
    pub async fn get_task_status(&self, task_id: &str) -> Result<TaskInfo, String> {
        // First check the local cache
        {
            let cache = self.task_cache.read().await;
            if let Some(task_info) = cache.get(task_id) {
                // Only return cached result if task is complete
                if task_info.status == "complete" || task_info.status == "failed" {
                    return Ok(task_info.clone());
                }
            }
        }
        
        // If not in cache or not complete, fetch from coordinator
        let url = format!("{}/tasks/{}/status", self.base_url, task_id);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to get task status: {}", e))?;

        let response_body: CoordinatorResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if response_body.success {
            // Parse the task info from the response
            let task_info: TaskInfo = serde_json::from_value(
                response_body.data
                    .ok_or_else(|| "Missing data in response".to_string())?
            )
            .map_err(|e| format!("Failed to parse task info: {}", e))?;
            
            // Update the cache
            {
                let mut cache = self.task_cache.write().await;
                cache.insert(task_id.to_string(), task_info.clone());
            }
            
            Ok(task_info)
        } else {
            Err(response_body.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    // Set task result in the coordinator
    pub async fn set_task_result(
        &self,
        task_id: &str,
        result: Vec<u8>,
    ) -> Result<(), String> {
        let url = format!("{}/tasks/{}/result", self.base_url, task_id);
        
        let response = self.client
            .post(&url)
            .json(&serde_json::json!({
                "worker_id": self.worker_id,
                "result": result,
            }))
            .send()
            .await
            .map_err(|e| format!("Failed to set task result: {}", e))?;

        let response_body: CoordinatorResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if response_body.success {
            // Update the local cache
            {
                let mut cache = self.task_cache.write().await;
                if let Some(task_info) = cache.get_mut(task_id) {
                    task_info.status = "complete".to_string();
                    task_info.results.push(result);
                }
            }
            
            Ok(())
        } else {
            Err(response_body.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    // Get available workers from the coordinator
    pub async fn get_available_workers(&self) -> Result<Vec<Worker>, String> {
        let url = format!("{}/workers", self.base_url);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to get workers: {}", e))?;

        let response_body: CoordinatorResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if response_body.success {
            // Parse the workers from the response
            let workers: Vec<Worker> = serde_json::from_value(
                response_body.data
                    .ok_or_else(|| "Missing data in response".to_string())?
            )
            .map_err(|e| format!("Failed to parse workers: {}", e))?;
            
            Ok(workers)
        } else {
            Err(response_body.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    // Get TEE pair for a region
    pub async fn get_tee_pair(&self, region_id: &str) -> Result<(String, String), String> {
        let url = format!("{}/regions/{}/pair", self.base_url, region_id);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to get TEE pair: {}", e))?;

        let response_body: CoordinatorResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if response_body.success {
            // Parse the TEE pair from the response
            let pair: serde_json::Value = response_body.data
                .ok_or_else(|| "Missing data in response".to_string())?;
            
            let worker1 = pair.get("worker1")
                .and_then(|w| w.as_str())
                .ok_or_else(|| "Missing worker1 in response".to_string())?;
                
            let worker2 = pair.get("worker2")
                .and_then(|w| w.as_str())
                .ok_or_else(|| "Missing worker2 in response".to_string())?;
            
            Ok((worker1.to_string(), worker2.to_string()))
        } else {
            Err(response_body.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }

    // Get available workers in a specific region
    pub async fn get_workers_for_region(&self, region_id: &str) -> Result<Vec<Worker>, String> {
        let url = format!("{}/workers/region/{}", self.base_url, region_id);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to get workers for region: {}", e))?;
            
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(format!("Failed to get workers for region ({}): {}", status, error_text));
        }
        
        let coordinator_response: CoordinatorResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse coordinator response: {}", e))?;
            
        if !coordinator_response.success {
            return Err(coordinator_response.error.unwrap_or_else(|| "Unknown error".to_string()));
        }
        
        let workers = coordinator_response.data
            .ok_or_else(|| "No data in response".to_string())
            .and_then(|data| {
                serde_json::from_value(data)
                    .map_err(|e| format!("Failed to parse workers data: {}", e))
            })?;
            
        Ok(workers)
    }

    // Submit execution request to the coordinator 
    pub async fn submit_execution(
        &self,
        request: ExecutionRequest,
    ) -> Result<CoordinatorExecutionResult, String> {
        let json = serde_json::to_string(&request).map_err(|e| format!("Error serializing request: {}", e))?;
        let url = format!("{}/execution", self.base_url);
        let resp = self.client.post(&url)
            .header("Content-Type", "application/json")
            .body(json)
            .send()
            .await
            .map_err(|e| format!("Error submitting execution: {}", e))?;

        if resp.status().is_success() {
            let result = resp.json::<CoordinatorExecutionResult>().await
                .map_err(|e| format!("Error parsing execution result: {}", e))?;
            Ok(result)
        } else {
            Err(format!("Error submitting execution: {}", resp.status()))
        }
    }

    // Update state via coordinator
    pub async fn update_state(&self, contract_id: &str, key: &[u8], value: &[u8]) -> Result<(), String> {
        let url = format!("{}/contracts/{}/state", self.base_url, contract_id);
        
        // Convert the key to base64 for the URL
        let key_base64 = base64::encode(key);
        
        let response = self.client
            .post(&url)
            .json(&serde_json::json!({
                "key": key_base64,
                "value": base64::encode(value),
                "worker_id": self.worker_id,
            }))
            .send()
            .await
            .map_err(|e| format!("Failed to update state: {}", e))?;

        let response_body: CoordinatorResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if response_body.success {
            Ok(())
        } else {
            Err(response_body.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }
    
    // Get state via coordinator
    pub async fn get_state(&self, contract_id: &str, key: &[u8]) -> Result<Vec<u8>, String> {
        // Convert the key to base64 for the URL
        let key_base64 = base64::encode(key);
        let url = format!("{}/contracts/{}/state?key={}", self.base_url, contract_id, key_base64);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to get state: {}", e))?;

        let response_body: CoordinatorResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        if response_body.success {
            // Parse the value from the response
            let data = response_body.data
                .ok_or_else(|| "Missing data in response".to_string())?;
            let data_str = data.as_str()
                .ok_or_else(|| "Data is not a string".to_string())?;
                
            // Decode the base64 value
            base64::decode(data_str)
                .map_err(|e| format!("Failed to decode value: {}", e))
        } else {
            Err(response_body.error.unwrap_or_else(|| "Unknown error".to_string()))
        }
    }
}
