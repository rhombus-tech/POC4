use tee_interface::prelude::*;
use thiserror::Error;
use async_trait::async_trait;

/// Errors that can occur during verification
#[derive(Error, Debug)]
pub enum VerificationError {
    #[error("Attestation verification failed: {0}")]
    AttestationError(String),

    #[error("Result mismatch: {0}")]
    ResultMismatch(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

impl From<VerificationError> for TeeError {
    fn from(err: VerificationError) -> Self {
        TeeError::VerificationError(err.to_string())
    }
}

/// Verifier for TEE execution results
pub struct TeeVerifier;

impl TeeVerifier {
    pub fn new() -> Self {
        Self
    }

    fn verify_attestation(&self, attestation: &TeeAttestation) -> Result<(), VerificationError> {
        match attestation.tee_type {
            TeeType::Sgx => self.verify_sgx_attestation(attestation),
            TeeType::Sev => self.verify_sev_attestation(attestation),
        }
    }

    fn verify_sgx_attestation(&self, _attestation: &TeeAttestation) -> Result<(), VerificationError> {
        // TODO: Implement actual SGX attestation verification
        // For now just return Ok
        Ok(())
    }

    fn verify_sev_attestation(&self, _attestation: &TeeAttestation) -> Result<(), VerificationError> {
        // TODO: Implement actual SEV attestation verification
        // For now just return Ok
        Ok(())
    }
}

#[async_trait]
impl TeeVerification for TeeVerifier {
    async fn verify_execution(
        &self,
        sgx_result: &ExecutionResult,
        sev_result: &ExecutionResult,
    ) -> Result<VerificationResult, TeeError> {
        // Verify outputs match
        if sgx_result.output != sev_result.output {
            return Err(VerificationError::ResultMismatch("Output mismatch".into()).into());
        }

        // Verify state hashes match
        if sgx_result.state_hash != sev_result.state_hash {
            return Err(VerificationError::ResultMismatch("State hash mismatch".into()).into());
        }

        // Verify attestations
        if sgx_result.attestations.is_empty() || sev_result.attestations.is_empty() {
            return Err(VerificationError::InvalidInput("Missing attestations".into()).into());
        }

        // Get first attestation from each
        let sgx_attestation = &sgx_result.attestations[0];
        let sev_attestation = &sev_result.attestations[0];

        // Verify attestations
        self.verify_attestation(sgx_attestation)?;
        self.verify_attestation(sev_attestation)?;

        Ok(VerificationResult {
            verified: true,
            result_hash: sgx_result.state_hash,
            attestations: vec![sgx_attestation.clone(), sev_attestation.clone()],
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_verification() {
        let verifier = TeeVerifier;

        // Create test attestation
        let attestation = TeeAttestation {
            tee_type: TeeType::Sgx,
            measurement: vec![0u8; 32],
            signature: vec![1, 2, 3],
        };

        // Create test result
        let result = ExecutionResult {
            tx_id: vec![1, 2, 3],
            state_hash: [0u8; 32],
            output: vec![4, 5, 6],
            attestations: vec![attestation.clone(), attestation.clone()],
            timestamp: 12345,
            region_id: String::from("test"),
        };

        // Verify result matches itself
        let verification = verifier.verify_execution(&result, &result).await.unwrap();
        assert!(verification.verified);
        assert_eq!(verification.attestations.len(), 2);
    }
}