use wasmlanche::{
    public, Address, Context, 
    state_schema,
    borsh::{BorshSerialize, BorshDeserialize}
};
use sha2::{Sha256, Digest};
use tee_interface::prelude::*;

state_schema! {
    /// Current accumulator value
    AccumulatorValue => [u8; 32],
    /// Per-executor witness data
    Witness(Address) => AccumulatorWitness,
    /// Per-executor attestation record
    AttestationRecord(Address) => AttestationRecord,
    /// Total size of accumulator
    AccumulatorSize => u64,
    /// Contract parameters
    Parameters => AccumulatorParams,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct AccumulatorParams {
    pub max_size: u64,
    pub max_witness_age: u64,
    pub min_attestations: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct AccumulatorElement {
    pub executor: Address,
    pub measurement: [u8; 32],
    pub enclave_type: EnclaveType,
    pub timestamp: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct AccumulatorWitness {
    pub value: [u8; 32],
    pub last_accumulator: [u8; 32],
    pub element: AccumulatorElement,
    pub last_update: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct AttestationRecord {
    pub executor: Address,
    pub last_attestation: u64,
    pub attestation_count: u64,
    pub sgx_measurement: Option<[u8; 32]>,
    pub sev_measurement: Option<[u8; 32]>,
}

const MAX_WITNESS_AGE: u64 = 7 * 24 * 60 * 60; // 1 week

#[public]
pub fn init(context: &mut Context, params: AccumulatorParams) -> Result<(), TeeError> {
    if context.get(Parameters)?.is_some() {
        return Err(TeeError::InitializationError("Already initialized".into()));
    }

    context.store((
        (Parameters, params),
        (AccumulatorSize, 0u64),
        (AccumulatorValue, [0u8; 32]),
    ))?;

    Ok(())
}

#[public]
pub fn verify_attestation(
    context: &mut Context,
    sgx_attestation: AttestationReport,
    sev_attestation: AttestationReport,
) -> Result<bool, TeeError> {
    let executor = context.actor();
    
    // Get executor record
    let record = context.get(AttestationRecord(executor))?.ok_or(
        TeeError::AttestationError("Executor not registered".into())
    )?;

    // Verify attestation freshness
    let current_time = context.timestamp();
    if current_time - record.last_attestation > MAX_WITNESS_AGE {
        return Err(TeeError::AttestationError("Attestation too old".into()));
    }

    // Verify witness exists and is valid
    let witness = context.get(Witness(executor))?.ok_or(
        TeeError::AttestationError("No witness found".into())
    )?;

    // Verify measurements match records
    if let Some(sgx_meas) = record.sgx_measurement {
        if sgx_attestation.measurement != sgx_meas {
            return Err(TeeError::AttestationError("SGX measurement mismatch".into()));
        }
    }

    if let Some(sev_meas) = record.sev_measurement {
        if sev_attestation.measurement != sev_meas {
            return Err(TeeError::AttestationError("SEV measurement mismatch".into()));
        }
    }

    // Verify witness value is valid
    if !verify_witness(&witness, context.get(AccumulatorValue)?.unwrap_or([0; 32])) {
        return Err(TeeError::AttestationError("Invalid witness".into()));
    }

    Ok(true)
}

#[public]
pub fn register_attestation(
    context: &mut Context,
    attestation: AttestationReport,
) -> Result<(), TeeError> {
    let params = context.get(Parameters)?.ok_or(TeeError::InitializationError("Not initialized".into()))?;
    let executor = context.actor();

    // Get or create attestation record
    let mut record = context.get(AttestationRecord(executor))?.unwrap_or(AttestationRecord {
        executor,
        last_attestation: 0,
        attestation_count: 0,
        sgx_measurement: None,
        sev_measurement: None,
    });

    // Update measurements based on enclave type
    match attestation.enclave_type {
        EnclaveType::IntelSGX => {
            record.sgx_measurement = Some(attestation.measurement);
        },
        EnclaveType::AMDSEV => {
            record.sev_measurement = Some(attestation.measurement);
        }
    }

    // Create accumulator element
    let element = AccumulatorElement {
        executor,
        measurement: attestation.measurement,
        enclave_type: attestation.enclave_type,
        timestamp: attestation.timestamp,
    };

    // Update accumulator
    let mut acc_value = context.get(AccumulatorValue)?.unwrap_or([0; 32]);
    let mut acc_size = context.get(AccumulatorSize)?.unwrap_or(0);

    if acc_size >= params.max_size {
        return Err(TeeError::AttestationError("Accumulator full".into()));
    }

    acc_value = update_accumulator(&acc_value, &element);
    acc_size += 1;

    // Create/update witness
    let witness = AccumulatorWitness {
        value: compute_initial_witness(&acc_value, &element),
        last_accumulator: acc_value,
        element,
        last_update: attestation.timestamp,
    };

    // Update record
    record.last_attestation = attestation.timestamp;
    record.attestation_count += 1;

    // Store state
    context.store((
        (AccumulatorValue, acc_value),
        (AccumulatorSize, acc_size),
        (AttestationRecord(executor), record),
        (Witness(executor), witness),
    ))?;

    Ok(())
}

// Helper functions
fn update_accumulator(current: &[u8; 32], element: &AccumulatorElement) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(current);
    hasher.update(&borsh::to_vec(element).unwrap());
    hasher.finalize().into()
}

fn compute_initial_witness(acc_value: &[u8; 32], element: &AccumulatorElement) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(acc_value);
    hasher.update(b"witness");
    hasher.update(&borsh::to_vec(element).unwrap());
    hasher.finalize().into()
}

fn verify_witness(witness: &AccumulatorWitness, acc_value: [u8; 32]) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(&witness.value);
    hasher.update(&acc_value);
    hasher.update(&witness.last_accumulator);
    witness.value == hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmlanche::simulator::{Simulator, SimpleState};

    fn setup() -> (Simulator, Address) {
        let mut state = SimpleState::new();
        let mut sim = Simulator::new(&mut state);
        
        let params = AccumulatorParams {
            max_size: 1000,
            max_witness_age: MAX_WITNESS_AGE,
            min_attestations: 2,
        };

        let ctx = &mut sim;
        init(ctx, params).unwrap();

        let executor = Address::new([1; 33]);
        sim.set_actor(executor);

        (sim, executor)
    }

    #[test]
    fn test_dual_attestation_verification() {
        let (mut sim, executor) = setup();
        let ctx = &mut sim;

        let sgx_attestation = AttestationReport {
            enclave_type: EnclaveType::IntelSGX,
            measurement: [1; 32],
            timestamp: ctx.timestamp(),
            platform_data: vec![1],
        };

        let sev_attestation = AttestationReport {
            enclave_type: EnclaveType::AMDSEV,
            measurement: [2; 32],
            timestamp: ctx.timestamp(),
            platform_data: vec![2],
        };

        // Register both attestations
        register_attestation(ctx, sgx_attestation.clone()).unwrap();
        register_attestation(ctx, sev_attestation.clone()).unwrap();

        // Verify attestations
        assert!(verify_attestation(ctx, sgx_attestation, sev_attestation).unwrap());
    }
}