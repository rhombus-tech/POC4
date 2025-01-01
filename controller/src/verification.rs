use tee_interface::prelude::*;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VerificationError {
    #[error("Results don't match between TEEs")]
    ResultMismatch,
    
    #[error("Invalid attestation: {0}")]
    InvalidAttestation(String),
    
    #[error("Verification failed: {0}")]
    VerificationFailed(String),
    
    #[error("Platform error: {0}")]
    PlatformError(String),
}

/// Verify results match between SGX and SEV executions
pub fn verify_results(
    sgx_result: &ExecutionResult,
    sev_result: &ExecutionResult,
) -> Result<VerificationResult, VerificationError> {
    // Verify attestations first
    verify_attestation(&sgx_result.attestation)?;
    verify_attestation(&sev_result.attestation)?;

    // Check result hashes match
    if sgx_result.result_hash != sev_result.result_hash {
        return Err(VerificationError::ResultMismatch);
    }

    // Verify actual results match if present
    if sgx_result.result != sev_result.result {
        return Err(VerificationError::ResultMismatch);
    }

    Ok(VerificationResult {
        verified: true,
        result_hash: sgx_result.result_hash,
        sgx_attestation: sgx_result.attestation.clone(),
        sev_attestation: sev_result.attestation.clone(),
    })
}

/// Verify a single attestation
fn verify_attestation(attestation: &AttestationReport) -> Result<(), VerificationError> {
    // Verify timestamp is reasonable
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| VerificationError::VerificationFailed(e.to_string()))?
        .as_secs();

    if attestation.timestamp > current_time {
        return Err(VerificationError::InvalidAttestation(
            "Attestation timestamp is in the future".into()
        ));
    }

    // Check age of attestation (shouldn't be more than 1 hour old)
    const MAX_AGE: u64 = 3600;
    if current_time - attestation.timestamp > MAX_AGE {
        return Err(VerificationError::InvalidAttestation(
            "Attestation is too old".into()
        ));
    }

    // Verify measurement is not empty
    if attestation.measurement.iter().all(|&x| x == 0) {
        return Err(VerificationError::InvalidAttestation(
            "Invalid measurement (all zeros)".into()
        ));
    }

    // Verify platform-specific data
    match attestation.enclave_type {
        EnclaveType::IntelSGX => verify_sgx_attestation(attestation),
        EnclaveType::AMDSEV => verify_sev_attestation(attestation),
    }
}

fn verify_sgx_attestation(attestation: &AttestationReport) -> Result<(), VerificationError> {
    // Here we would verify SGX-specific measurements and platform data
    // For example:
    // - Check MRENCLAVE
    // - Verify quote structure
    // - Check TCB level
    // For now, just a basic check
    if attestation.platform_data.is_empty() {
        return Err(VerificationError::InvalidAttestation(
            "Missing SGX platform data".into()
        ));
    }

    Ok(())
}

fn verify_sev_attestation(attestation: &AttestationReport) -> Result<(), VerificationError> {
    // Here we would verify SEV-specific measurements and platform data
    // For example:
    // - Check measurement structure
    // - Verify platform certificates
    // - Check policy
    // For now, just a basic check
    if attestation.platform_data.is_empty() {
        return Err(VerificationError::InvalidAttestation(
            "Missing SEV platform data".into()
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_attestation(enclave_type: EnclaveType) -> AttestationReport {
        AttestationReport {
            enclave_type,
            measurement: [1u8; 32], // Non-zero measurement
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            platform_data: vec![1, 2, 3], // Some platform data
        }
    }

    fn create_test_result(enclave_type: EnclaveType) -> ExecutionResult {
        ExecutionResult {
            result_hash: [1u8; 32],
            result: vec![1, 2, 3],
            attestation: create_test_attestation(enclave_type),
        }
    }

    #[test]
    fn test_matching_results() {
        let sgx_result = create_test_result(EnclaveType::IntelSGX);
        let sev_result = create_test_result(EnclaveType::AMDSEV);

        let verification = verify_results(&sgx_result, &sev_result);
        assert!(verification.is_ok());
        assert!(verification.unwrap().verified);
    }

    #[test]
    fn test_mismatched_results() {
        let mut sgx_result = create_test_result(EnclaveType::IntelSGX);
        let sev_result = create_test_result(EnclaveType::AMDSEV);

        // Modify SGX result to create mismatch
        sgx_result.result_hash = [2u8; 32];

        let verification = verify_results(&sgx_result, &sev_result);
        assert!(matches!(verification, Err(VerificationError::ResultMismatch)));
    }

    #[test]
    fn test_invalid_attestation() {
        let mut sgx_result = create_test_result(EnclaveType::IntelSGX);
        let sev_result = create_test_result(EnclaveType::AMDSEV);

        // Create invalid attestation
        sgx_result.attestation.measurement = [0u8; 32];

        let verification = verify_results(&sgx_result, &sev_result);
        assert!(matches!(verification, Err(VerificationError::InvalidAttestation(_))));
    }
}