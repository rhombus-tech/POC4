use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use axum::{
    Json, Router, 
    extract::{Path, State},
    routing::{get, post},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use serde::ser::SerializeMap;
use serde_json::{json, Value};

// AppState to store our data
type AppState = Arc<RwLock<CoordinatorState>>;

// Standard response struct to ensure consistency
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoordinatorResponse {
    success: bool,
    error: Option<String>,
    data: Option<Value>,
}

struct CoordinatorState {
    workers: HashMap<String, WorkerInfo>,
    tee_pairs: HashMap<String, TeePair>,
    tasks: HashMap<String, InternalTaskInfo>,
    task_counter: usize,
}

struct WorkerInfo {
    id: String,
    attestation: String,
    last_heartbeat: u64,
}

#[derive(Debug, Clone)]
struct TeePair {
    region_id: String,
    primary_worker_id: String,
    secondary_worker_id: String,
    attestations: Option<Vec<Vec<u8>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InternalTaskInfo {
    id: String,
    payload: Value,
    status: String,
    result: Option<Vec<u8>>,
    error: Option<String>,
}

#[derive(Serialize)]
struct TaskInfo {
    task: Task,
    status: String,
    start_time: String,
    end_time: String,
    error: Option<String>,
    results: Vec<Vec<u8>>,
}

#[derive(Serialize)]
struct Task {
    id: String,
    worker_ids: Vec<String>,
    data: Vec<u8>,
    attestations: Vec<Vec<u8>>,
    timeout: u64,
    region_id: String,
}

// API Request/Response Types
#[derive(Debug, Deserialize)]
struct RegisterWorkerRequest {
    #[serde(rename = "id", alias = "worker_id")]
    worker_id: String,
    enclave_id: Vec<u8>,
    // For backward compatibility, make attestation optional
    attestation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RegisterTeePairRequest {
    primary_worker_id: String,
    secondary_worker_id: String,
    #[serde(default)]
    attestations: Vec<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
struct TaskSubmitRequest {
    id: String,
    worker_ids: Vec<String>,
    data: Vec<u8>,
    attestations: Vec<Vec<u8>>,
    timeout: u64,
    region_id: String,
}

#[derive(Serialize)]
struct TaskSubmitResponse {
    task_id: String,
}

#[derive(Serialize)]
struct TaskStatusResponse {
    task_id: String,
    status: String,
    result: Option<Vec<u8>>,
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkerListResponse {
    workers: Vec<String>,
}

// TeePair struct is now used from above (with proper serialization for use in handlers)
impl Serialize for TeePair {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("primary_worker_id", &self.primary_worker_id)?;
        map.serialize_entry("secondary_worker_id", &self.secondary_worker_id)?;
        map.serialize_entry("attestations", &self.attestations)?;
        map.serialize_entry("region_id", &self.region_id)?;
        map.end()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Worker {
    id: String,
    enclave_id: Vec<u8>,
    status: u8,
}

// API Handlers
async fn register_worker(
    State(state): State<AppState>,
    Json(req): Json<RegisterWorkerRequest>,
) -> impl IntoResponse {
    println!("Received register_worker request: {:#?}", req);
    
    let mut coordinator = state.write().await;
    
    let attestation = req.attestation.unwrap_or_else(|| "default-attestation".to_string());
    
    let worker_info = WorkerInfo {
        id: req.worker_id.clone(),
        attestation,
        last_heartbeat: 0, // Would use system time in real impl
    };
    
    coordinator.workers.insert(req.worker_id.clone(), worker_info);
    
    println!("Registered worker: {}", req.worker_id);
    
    let response = CoordinatorResponse {
        success: true,
        error: None,
        data: Some(json!({ "worker_id": req.worker_id }))
    };
    
    println!("Sending response: {:#?}", response);
    
    (StatusCode::OK, Json(response))
}

async fn register_tee_pair(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    Json(req): Json<RegisterTeePairRequest>,
) -> Json<CoordinatorResponse> {
    let mut coordinator = state.write().await;
    
    // For this endpoint, we assume the region is derived from the worker
    // We'll just use a mock region ID
    let region_id = "mock-region-1".to_string();
    
    let tee_pair = TeePair {
        region_id: region_id.clone(),
        primary_worker_id: req.primary_worker_id.clone(),
        secondary_worker_id: req.secondary_worker_id.clone(),
        attestations: Some(req.attestations.clone()),
    };
    
    // In a real implementation, would check both workers exist
    coordinator.tee_pairs.insert(region_id.clone(), tee_pair.clone());
    
    println!("Registered TEE pair for worker {}: primary={}, secondary={}", 
        worker_id, req.primary_worker_id, req.secondary_worker_id);
        
    Json(CoordinatorResponse {
        success: true,
        error: None,
        data: Some(json!({ "region_id": region_id }))
    })
}

async fn register_tee_pair_by_region(
    State(state): State<AppState>,
    Path(region_id): Path<String>,
    Json(req): Json<RegisterTeePairRequest>,
) -> Json<CoordinatorResponse> {
    let mut coordinator = state.write().await;
    
    let tee_pair = TeePair {
        region_id: region_id.clone(),
        primary_worker_id: req.primary_worker_id.clone(),
        secondary_worker_id: req.secondary_worker_id.clone(),
        attestations: Some(req.attestations.clone()),
    };
    
    // In a real implementation, would check both workers exist
    coordinator.tee_pairs.insert(region_id.clone(), tee_pair.clone());
    
    println!("Registered TEE pair for region {}: primary={}, secondary={}", 
        region_id, req.primary_worker_id, req.secondary_worker_id);
        
    Json(CoordinatorResponse {
        success: true,
        error: None,
        data: Some(json!({ "region_id": region_id }))
    })
}

async fn get_workers(
    State(state): State<AppState>,
) -> Json<CoordinatorResponse> {
    let coordinator = state.read().await;
    let workers = coordinator.workers.keys()
        .map(|id| Worker {
            id: id.clone(),
            enclave_id: vec![0, 1, 2, 3], // Mock enclave ID
            status: 1, // Active status
        })
        .collect::<Vec<_>>();
    
    println!("Returning workers: {:?}", workers);
    
    Json(CoordinatorResponse {
        success: true,
        error: None,
        data: Some(json!(workers))
    })
}

async fn submit_task(
    State(state): State<AppState>,
    Json(req): Json<TaskSubmitRequest>,
) -> Json<CoordinatorResponse> {
    let mut coordinator = state.write().await;
    
    // Generate a task ID
    let task_id = format!("task-{}", coordinator.task_counter);
    coordinator.task_counter += 1;
    
    // Convert the client request to an internal task info object
    let task_info = InternalTaskInfo {
        id: task_id.clone(),
        payload: json!({
            "id": req.id,
            "data": req.data,
            "worker_ids": req.worker_ids,
            "region_id": req.region_id,
        }),
        status: "pending".to_string(),
        result: None,
        error: None,
    };
    
    coordinator.tasks.insert(task_id.clone(), task_info);
    
    // In a real implementation, we would assign the task to a TEE pair
    // For this mock, we'll just complete it immediately
    tokio::spawn({
        let state = state.clone();
        let task_id = task_id.clone();
        
        async move {
            // Simulate processing time
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            let mut coordinator = state.write().await;
            if let Some(task) = coordinator.tasks.get_mut(&task_id) {
                task.status = "completed".to_string();
                task.result = Some(b"mock_result".to_vec());
                
                // Print a debug message to show task completion
                println!("Task {} completed with status: {}", task_id, task.status);
            }
        }
    });
    
    println!("Submitted task: {}", task_id);
    Json(CoordinatorResponse {
        success: true,
        error: None,
        data: Some(json!({ "task_id": task_id }))
    })
}

async fn get_task_status(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Json<CoordinatorResponse> {
    let coordinator = state.read().await;
    
    if let Some(internal_task) = coordinator.tasks.get(&task_id) {
        // Extract worker_ids and data from the payload
        let payload = &internal_task.payload;
        
        // Extract or default values from the payload for the Task structure
        let worker_ids = payload.get("worker_ids")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
            .unwrap_or_else(|| vec![]);
            
        let data = payload.get("data")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
            .unwrap_or_else(|| vec![]);
            
        let region_id = payload.get("region_id")
            .and_then(|v| v.as_str())
            .unwrap_or("default")
            .to_string();
            
        // Create the Task and TaskInfo structures
        let task = Task {
            id: internal_task.id.clone(),
            worker_ids,
            data,
            attestations: vec![],  // Default empty attestations
            timeout: 30000,         // Default timeout
            region_id,
        };
        
        let results = if let Some(result) = &internal_task.result {
            vec![result.clone()]
        } else {
            vec![]
        };
        
        let task_info = TaskInfo {
            task,
            status: internal_task.status.clone(),
            start_time: "2023-01-01T00:00:00Z".to_string(),  // Default timestamp
            end_time: "2023-01-01T00:01:00Z".to_string(),    // Default timestamp
            error: internal_task.error.clone(),
            results,
        };
        
        Json(CoordinatorResponse {
            success: true,
            error: None,
            data: Some(json!(task_info))
        })
    } else {
        Json(CoordinatorResponse {
            success: false,
            error: Some("Task not found".to_string()),
            data: None
        })
    }
}

async fn health_check() -> impl IntoResponse {
    println!("Health check requested");
    (StatusCode::OK, "ok")
}

#[tokio::main]
async fn main() {
    // Create initial state
    let coordinator_state = CoordinatorState {
        workers: HashMap::new(),
        tee_pairs: HashMap::new(),
        tasks: HashMap::new(),
        task_counter: 0,
    };
    
    let shared_state = Arc::new(RwLock::new(coordinator_state));
    
    // Build our application with routes
    let app = Router::new()
        .route("/workers", post(register_worker))
        .route("/workers", get(get_workers))
        .route("/workers/register", post(register_worker)) // Add /workers/register endpoint
        .route("/workers/:worker_id/tee_pairs", post(register_tee_pair))
        .route("/regions/:region_id/pairs/register", post(register_tee_pair_by_region)) // Add region-based registration
        .route("/tasks", post(submit_task))
        .route("/tasks/submit", post(submit_task))
        .route("/tasks/:task_id", get(get_task_status))
        .route("/tasks/:task_id/status", get(get_task_status))
        .route("/health", get(health_check))
        .with_state(shared_state);
        
    println!("Routes configuration complete!");
    
    // Run our app
    let addr = "0.0.0.0:8080";
    println!("Mock coordinator listening on {}", addr);
    
    axum::Server::bind(&addr.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
