use tee_interface::types::ExecutionPayload;
use sha2::{Sha256, Digest};

/// Computation engine that runs inside the TEE
pub struct ComputationEngine {
    // State/context for computation if needed
    state: ComputationState,
}

#[derive(Default)]
struct ComputationState {
    total_executions: u64,
    current_hash: [u8; 32],
}

impl ComputationEngine {
    pub fn new() -> Self {
        Self {
            state: ComputationState::default(),
        }
    }

    /// Execute computation on input data
    /// Changed to return String error to match Hypersdk error handling
    pub fn execute(&mut self, payload: ExecutionPayload) -> Result<Vec<u8>, String> {
        // For testing, we'll just sum the input bytes
        let sum: u32 = payload.input.iter().map(|&x| x as u32).sum();
        
        // Update state
        self.state.total_executions += 1;
        self.state.current_hash = compute_state_hash(&payload.input);
        
        Ok(vec![sum as u8])
    }

    /// Get current state hash
    pub fn get_state_hash(&self) -> [u8; 32] {
        self.state.current_hash
    }

    /// Get execution count
    pub fn get_execution_count(&self) -> u64 {
        self.state.total_executions
    }
}

/// Compute state hash using SHA-256
fn compute_state_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_computation() {
        let mut engine = ComputationEngine::new();
        
        let payload = ExecutionPayload {
            input: vec![1, 2, 3, 4],
            ..Default::default()
        };
        
        let result = engine.execute(payload).unwrap();
        assert_eq!(result, vec![10]); // 1 + 2 + 3 + 4 = 10
        assert_eq!(engine.get_execution_count(), 1);
    }
}