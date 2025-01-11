use tee_interface::prelude::*;
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
        // Track execution
        self.state.total_executions += 1;

        // Process input based on computation type
        let result = match decode_computation_type(&payload.input)
            .map_err(|e| e.to_string())? 
        {
            ComputationType::Hash => self.compute_hash(payload.input),
            ComputationType::Sort => self.compute_sort(payload.input),
            ComputationType::Transform => self.compute_transform(payload.input),
        }?;

        // Update state hash
        self.state.current_hash = compute_state_hash(&result);

        Ok(result)
    }

    /// Compute hash of data
    fn compute_hash(&self, data: Vec<u8>) -> Result<Vec<u8>, String> {
        let mut hasher = Sha256::new();
        hasher.update(&data);
        Ok(hasher.finalize().to_vec())
    }

    /// Sort input data
    fn compute_sort(&self, mut data: Vec<u8>) -> Result<Vec<u8>, String> {
        data.sort();
        Ok(data)
    }

    /// Transform data (example: increment each byte)
    fn compute_transform(&self, data: Vec<u8>) -> Result<Vec<u8>, String> {
        Ok(data.into_iter().map(|b| b.wrapping_add(1)).collect())
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

#[derive(Debug)]
enum ComputationType {
    Hash,
    Sort,
    Transform,
}

fn decode_computation_type(input: &[u8]) -> Result<ComputationType, String> {
    if input.is_empty() {
        return Err("Empty input".into());
    }

    match input[0] {
        0 => Ok(ComputationType::Hash),
        1 => Ok(ComputationType::Sort),
        2 => Ok(ComputationType::Transform),
        _ => Err("Unknown computation type".into()),
    }
}

fn compute_state_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_computation() {
        let mut engine = ComputationEngine::new();
        let payload = ExecutionPayload {
            execution_id: 1,
            input: vec![0, 1, 2, 3],  // 0 for hash type
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: false,
            },
        };

        let result = engine.execute(payload);
        assert!(result.is_ok());
        assert_eq!(engine.get_execution_count(), 1);
    }

    #[test]
    fn test_sort_computation() {
        let mut engine = ComputationEngine::new();
        let payload = ExecutionPayload {
            execution_id: 1,
            input: vec![1, 3, 2, 1],  // 1 for sort type
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: false,
            },
        };

        let result = engine.execute(payload).unwrap();
        assert_eq!(&result[1..], &[1, 2, 3]);
    }

    #[test]
    fn test_transform_computation() {
        let mut engine = ComputationEngine::new();
        let payload = ExecutionPayload {
            execution_id: 1,
            input: vec![2, 1, 2, 3],  // 2 for transform type
            params: ExecutionParams {
                expected_hash: None,
                detailed_proof: false,
            },
        };

        let result = engine.execute(payload).unwrap();
        assert_eq!(&result[1..], &[2, 3, 4]);
    }
}