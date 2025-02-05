use tee_interface::prelude::*;
use async_trait::async_trait;
use sha2::{Sha256, Digest};

pub struct TeeVerifier {
    config: TeeConfig,
}

impl TeeVerifier {
    pub fn new(config: TeeConfig) -> Self {
        Self { config }
    }

    fn verify_measurement(&self, tee_type: TeeType, measurement: &[u8]) -> bool {
        // Verify measurement size based on TEE type
        let expected_size = match tee_type {
            TeeType::Sgx => 32, // SGX measurement is SHA256
            TeeType::Sev => 48, // SEV measurement includes additional platform data
            _ => return false,
        };

        if measurement.len() != expected_size {
            return false;
        }

        // In a real implementation, we would:
        // 1. Verify against known good measurements
        // 2. Check revocation lists
        // 3. Validate measurement format based on TEE type
        // 4. For SEV: verify additional platform data
        true
    }

    fn verify_signature(&self, tee_type: TeeType, signature: &[u8], measurement: &[u8]) -> bool {
        // In a real implementation, we would:
        // 1. Get the public key for the TEE type
        // 2. Verify the signature using platform-specific logic
        // 3. For SEV: verify AMD certificate chain
        match tee_type {
            TeeType::Sgx => signature.len() == 64, // ECDSA signature
            TeeType::Sev => signature.len() == 512, // RSA-4096 signature
            _ => false,
        }
    }
}

#[async_trait]
impl TeeVerification for TeeVerifier {
    async fn verify_attestation(&self, attestation: &TeeAttestation) -> Result<bool, TeeError> {
        // Verify measurement is present
        if attestation.measurement.is_empty() {
            return Err(TeeError::AttestationError(
                "Empty measurement".to_string(),
            ));
        }

        // Verify signature
        if attestation.signature.is_empty() {
            return Err(TeeError::AttestationError(
                "Missing signature".to_string(),
            ));
        }

        // Verify measurement based on TEE type
        if !self.verify_measurement(attestation.tee_type, &attestation.measurement) {
            return Err(TeeError::AttestationError(
                format!("Invalid measurement for {:?}", attestation.tee_type),
            ));
        }

        // Verify signature
        if !self.verify_signature(attestation.tee_type, &attestation.signature, &attestation.measurement) {
            return Err(TeeError::AttestationError(
                format!("Invalid signature for {:?}", attestation.tee_type),
            ));
        }

        Ok(true)
    }

    async fn verify_result(&self, result: &ExecutionResult) -> Result<bool, TeeError> {
        // Verify we have attestations
        if result.attestations.is_empty() {
            return Err(TeeError::AttestationError(
                "No attestations".to_string(),
            ));
        }

        // Verify each attestation
        for attestation in &result.attestations {
            self.verify_attestation(attestation).await?;
        }

        // Verify output hash matches state hash
        let mut hasher = Sha256::new();
        hasher.update(&result.output);
        let output_hash = hasher.finalize();

        if output_hash.as_slice() != result.state_hash {
            return Err(TeeError::StateError(
                "State hash mismatch".to_string(),
            ));
        }

        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sgx_attestation() {
        let config = TeeConfig::default();
        let verifier = TeeVerifier::new(config);

        let attestation = TeeAttestation {
            tee_type: TeeType::Sgx,
            measurement: vec![1; 32], // SGX measurement size
            signature: vec![1; 64],   // ECDSA signature size
        };

        assert!(verifier.verify_attestation(&attestation).await.unwrap());
    }

    #[tokio::test]
    async fn test_sev_attestation() {
        let config = TeeConfig::default();
        let verifier = TeeVerifier::new(config);

        let attestation = TeeAttestation {
            tee_type: TeeType::Sev,
            measurement: vec![1; 48], // SEV measurement size
            signature: vec![1; 512],  // RSA-4096 signature size
        };

        assert!(verifier.verify_attestation(&attestation).await.unwrap());
    }

    #[tokio::test]
    async fn test_mixed_attestations() {
        let config = TeeConfig::default();
        let verifier = TeeVerifier::new(config);

        let mut hasher = Sha256::new();
        hasher.update(b"test output");
        let state_hash = hasher.finalize().into();

        let result = ExecutionResult {
            tx_id: vec![1],
            output: b"test output".to_vec(),
            state_hash,
            attestations: vec![
                TeeAttestation {
                    tee_type: TeeType::Sgx,
                    measurement: vec![1; 32],
                    signature: vec![1; 64],
                },
                TeeAttestation {
                    tee_type: TeeType::Sev,
                    measurement: vec![1; 48],
                    signature: vec![1; 512],
                },
            ],
            timestamp: 123456789,
            region_id: "test-region".to_string(),
        };

        assert!(verifier.verify_result(&result).await.unwrap());
    }
}