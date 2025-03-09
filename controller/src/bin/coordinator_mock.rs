use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use axum::{
    Json, Router, 
    extract::{Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// AppState to store our data
type AppState = Arc<RwLock<CoordinatorState>>;

struct CoordinatorState {
    workers: HashMap<String, WorkerInfo>,
    tee_pairs: HashMap<String, TeePair>,
    tasks: HashMap<String, TaskInfo>,
    task_counter: usize,
}

struct WorkerInfo {
    id: String,
    attestation: String,
    last_heartbeat: u64,
}

struct TeePair {
    region_id: String,
    primary_worker_id: String,
    secondary_worker_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskInfo {
    id: String,
    payload: Value,
    status: String,
    result: Option<Vec<u8>>,
    error: Option<String>,
}

// API Request/Response Types
#[derive(Deserialize)]
struct RegisterWorkerRequest {
    worker_id: String,
    attestation: String,
}

#[derive(Deserialize)]
struct RegisterTeePairRequest {
    region_id: String,
    secondary_worker_id: String,
}

#[derive(Deserialize)]
struct TaskSubmitRequest {
    payload: Value,
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

#[derive(Serialize)]
struct WorkerListResponse {
    workers: Vec<String>,
}

// API Handlers
async fn register_worker(
    State(state): State<AppState>,
    Json(req): Json<RegisterWorkerRequest>,
) -> Json<Value> {
    let mut coordinator = state.write().await;
    
    let worker_info = WorkerInfo {
        id: req.worker_id.clone(),
        attestation: req.attestation,
        last_heartbeat: 0, // Would use system time in real impl
    };
    
    coordinator.workers.insert(req.worker_id.clone(), worker_info);
    
    println!("Registered worker: {}", req.worker_id);
    Json(json!({ "status": "ok" }))
}

async fn register_tee_pair(
    State(state): State<AppState>,
    Path(worker_id): Path<String>,
    Json(req): Json<RegisterTeePairRequest>,
) -> Json<Value> {
    let mut coordinator = state.write().await;
    
    // Create a unique pair ID
    let pair_id = format!("{}_{}", req.region_id, worker_id);
    
    let tee_pair = TeePair {
        region_id: req.region_id,
        primary_worker_id: worker_id,
        secondary_worker_id: req.secondary_worker_id,
    };
    
    coordinator.tee_pairs.insert(pair_id.clone(), tee_pair);
    
    println!("Registered TEE pair: {}", pair_id);
    Json(json!({ "status": "ok", "pair_id": pair_id }))
}

async fn get_workers(
    State(state): State<AppState>,
) -> Json<WorkerListResponse> {
    let coordinator = state.read().await;
    
    let workers = coordinator.workers.keys()
        .cloned()
        .collect();
    
    Json(WorkerListResponse { workers })
}

async fn submit_task(
    State(state): State<AppState>,
    Json(req): Json<TaskSubmitRequest>,
) -> Json<TaskSubmitResponse> {
    let mut coordinator = state.write().await;
    
    let task_id = format!("task_{}", coordinator.task_counter);
    coordinator.task_counter += 1;
    
    let task_info = TaskInfo {
        id: task_id.clone(),
        payload: req.payload,
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
            }
        }
    });
    
    println!("Submitted task: {}", task_id);
    Json(TaskSubmitResponse { task_id })
}

async fn get_task_status(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Json<TaskStatusResponse> {
    let coordinator = state.read().await;
    
    if let Some(task) = coordinator.tasks.get(&task_id) {
        Json(TaskStatusResponse {
            task_id: task.id.clone(),
            status: task.status.clone(),
            result: task.result.clone(),
            error: task.error.clone(),
        })
    } else {
        Json(TaskStatusResponse {
            task_id,
            status: "not_found".to_string(),
            result: None,
            error: Some("Task not found".to_string()),
        })
    }
}

async fn health_check() -> &'static str {
    "ok"
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
        .route("/workers/:worker_id/tee_pairs", post(register_tee_pair))
        .route("/tasks", post(submit_task))
        .route("/tasks/:task_id", get(get_task_status))
        .route("/health", get(health_check))
        .with_state(shared_state);
    
    // Run our app
    let addr = "0.0.0.0:8080";
    println!("Mock coordinator listening on {}", addr);
    
    axum::Server::bind(&addr.parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
