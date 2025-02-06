use std::sync::Arc;
use tokio::sync::RwLock;
use tee_interface::prelude::*;
use crate::simulator::WasmSimulator;
use futures::join;

pub struct TeeExecutorPair {
    pub id: String,
    pub sgx: Arc<RwLock<WasmSimulator>>,
    pub sev: Arc<RwLock<WasmSimulator>>,
}

impl TeeExecutorPair {
    pub fn new(id: String, _sgx_config: String, _sev_config: String) -> Self {
        let sgx = Arc::new(RwLock::new(WasmSimulator::new()));
        let sev = Arc::new(RwLock::new(WasmSimulator::new()));

        Self { id, sgx, sev }
    }

    pub async fn init(&mut self) -> Result<(), TeeError> {
        let mut sgx_lock = self.sgx.write().await;
        let mut sev_lock = self.sev.write().await;

        // Initialize both controllers
        sgx_lock.init().await?;
        sev_lock.init().await?;

        Ok(())
    }

    pub async fn execute(&mut self, payload: &ExecutionPayload) -> Result<PairedExecutionResult, TeeError> {
        println!("Executing in paired TEEs");
        println!("Payload execution_id: {}", payload.execution_id);
        println!("Payload input size: {}", payload.input.len());
        
        let mut sgx_lock = self.sgx.write().await;
        let mut sev_lock = self.sev.write().await;

        // Execute in both TEEs concurrently
        let (sgx_result, sev_result) = join!(
            sgx_lock.execute(payload),
            sev_lock.execute(payload)
        );

        let sgx_result = sgx_result?;
        let sev_result = sev_result?;

        println!("SGX result: {:?}", sgx_result.result);
        println!("SEV result: {:?}", sev_result.result);

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

        let (sgx_att, sev_att) = join!(
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
