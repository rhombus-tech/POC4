use std::sync::Arc;
use tokio::sync::RwLock;
use tee_interface::prelude::*;
use crate::enarx::EnarxController;

pub struct TeeExecutorPair {
    pub id: String,
    pub sgx: Arc<RwLock<EnarxController>>,
    pub sev: Arc<RwLock<EnarxController>>,
}

impl TeeExecutorPair {
    pub fn new(id: String, sgx_config: String, sev_config: String) -> Self {
        let sgx = Arc::new(RwLock::new(EnarxController::new(
            TeeType::SGX,
            sgx_config,
        )));
        
        let sev = Arc::new(RwLock::new(EnarxController::new(
            TeeType::SEV,
            sev_config,
        )));

        Self { id, sgx, sev }
    }

    pub async fn execute(&self, payload: &ExecutionPayload) -> Result<PairedExecutionResult, TeeError> {
        let sgx_lock = self.sgx.write().await;
        let sev_lock = self.sev.write().await;

        // Execute in both TEEs concurrently
        let (sgx_result, sev_result) = tokio::join!(
            sgx_lock.execute(payload),
            sev_lock.execute(payload)
        );

        let sgx_result = sgx_result?;
        let sev_result = sev_result?;

        // Verify results match
        if sgx_result.result != sev_result.result {
            return Err(TeeError::ExecutionError("Results from SGX and SEV do not match".into()));
        }

        if sgx_result.state_hash != sev_result.state_hash {
            return Err(TeeError::ExecutionError("State hashes from SGX and SEV do not match".into()));
        }

        Ok(PairedExecutionResult {
            sgx: sgx_result,
            sev: sev_result,
        })
    }

    pub async fn get_attestations(&self) -> Result<(TeeAttestation, TeeAttestation), TeeError> {
        let sgx_lock = self.sgx.read().await;
        let sev_lock = self.sev.read().await;

        let (sgx_att, sev_att) = tokio::join!(
            sgx_lock.get_attestations(),
            sev_lock.get_attestations()
        );

        Ok((sgx_att?.remove(0), sev_att?.remove(0)))
    }
}

pub struct PairedExecutionResult {
    pub sgx: ExecutionResult,
    pub sev: ExecutionResult,
}
