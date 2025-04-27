// Helper methods for mesh execution extension
impl HyperTeeController {
    /// Check if mesh execution should be used for the given target TEE and region
    pub async fn should_use_mesh_execution(&self, region_id: &str, target_tee: &str) -> bool {
        // Cross-region mesh might need special handling
        // For now, we'll be conservative and default to false
        if region_id != self.get_region_id() {
            return false;
        }
        
        // Check if there are any circuit breakers active for this target
        if let Some(policy_manager) = &self.policy_manager {
            let circuit_breaker_id = format!("mesh:{}:{}", region_id, target_tee);
            
            // Get circuit breaker status for the region
            let status = policy_manager.get_circuit_breaker_status(region_id).await;
            if status.get(&circuit_breaker_id).copied().unwrap_or(false) {
                // Circuit breaker is active, don't use mesh
                return false;
            }
        }
        
        // All checks passed, use mesh
        true
    }
}
