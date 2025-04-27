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
    /// Batch accumulation buffer
    BatchBuffer => Vec<AccumulatorElement>,
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
    pub batch_size: u64,          // Maximum batch size
    pub batch_timeout_secs: u64,  // Maximum time to wait before processing a batch
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
    pub batch_id: Option<u64>,    // ID of the batch this witness belongs to (if any)
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct AttestationRecord {
    pub executor: Address,
    pub last_attestation: u64,
    pub attestation_count: u64,
    pub sgx_measurement: Option<[u8; 32]>,
    pub sev_measurement: Option<[u8; 32]>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone)]
pub struct BatchInfo {
    pub batch_id: u64,
    pub elements: Vec<AccumulatorElement>,
    pub timestamp: u64,
}

const MAX_WITNESS_AGE: u64 = 7 * 24 * 60 * 60; // 1 week
const DEFAULT_BATCH_SIZE: u64 = 100;
const DEFAULT_BATCH_TIMEOUT: u64 = 10; // seconds

#[public]
pub fn init(context: &mut Context, params: Option<AccumulatorParams>) -> Result<(), TeeError> {
    if context.get(Parameters)?.is_some() {
        return Err(TeeError::InitializationError("Already initialized".into()));
    }

    // Use provided params or defaults
    let params = params.unwrap_or(AccumulatorParams {
        max_size: 1000000,
        max_witness_age: MAX_WITNESS_AGE,
        min_attestations: 2,
        batch_size: DEFAULT_BATCH_SIZE,
        batch_timeout_secs: DEFAULT_BATCH_TIMEOUT,
    });

    context.store((
        (Parameters, params),
        (AccumulatorSize, 0u64),
        (AccumulatorValue, [0u8; 32]),
        (BatchBuffer, Vec::new()),
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

    // Add to batch buffer
    let mut batch_buffer = context.get(BatchBuffer)?.unwrap_or_default();
    batch_buffer.push(element.clone());
    
    // Update record
    record.last_attestation = attestation.timestamp;
    record.attestation_count += 1;
    
    // Check if we need to process the batch
    let process_batch = batch_buffer.len() >= params.batch_size as usize || 
                        (batch_buffer.len() > 0 && 
                         batch_buffer[0].timestamp + params.batch_timeout_secs < context.timestamp());
    
    // Create/update witness immediately for this executor
    // This allows the executor to continue without waiting for batch completion
    let mut acc_value = context.get(AccumulatorValue)?.unwrap_or([0; 32]);
    let witness = AccumulatorWitness {
        value: compute_initial_witness(&acc_value, &element),
        last_accumulator: acc_value,
        element,
        last_update: attestation.timestamp,
        batch_id: None, // Will be updated during batch processing if needed
    };
    
    // Store state
    context.store((
        (AttestationRecord(executor), record),
        (Witness(executor), witness),
        (BatchBuffer, batch_buffer.clone()),
    ))?;
    
    // Process batch if needed
    if process_batch {
        process_batch_update(context)?;
    }

    Ok(())
}

#[public]
pub fn force_batch_update(context: &mut Context) -> Result<(), TeeError> {
    process_batch_update(context)
}

fn process_batch_update(context: &mut Context) -> Result<(), TeeError> {
    let mut batch_buffer = context.get(BatchBuffer)?.unwrap_or_default();
    if batch_buffer.is_empty() {
        return Ok(());
    }
    
    let params = context.get(Parameters)?.ok_or(TeeError::InitializationError("Not initialized".into()))?;
    let mut acc_value = context.get(AccumulatorValue)?.unwrap_or([0; 32]);
    let mut acc_size = context.get(AccumulatorSize)?.unwrap_or(0);
    
    if acc_size + batch_buffer.len() as u64 > params.max_size {
        return Err(TeeError::AttestationError("Accumulator would exceed maximum size".into()));
    }
    
    // Create a batch update
    let batch_id = context.timestamp(); // Use timestamp as batch ID
    
    // Update accumulator with entire batch at once (more efficient)
    acc_value = update_accumulator_batch(&acc_value, &batch_buffer);
    acc_size += batch_buffer.len() as u64;
    
    // Update all witnesses for elements in the batch
    for element in &batch_buffer {
        let executor = element.executor;
        if let Some(mut witness) = context.get(Witness(executor))? {
            witness.batch_id = Some(batch_id);
            witness.last_accumulator = acc_value;
            context.store((Witness(executor), witness))?;
        }
    }
    
    // Clear batch buffer and update accumulator state
    context.store((
        (AccumulatorValue, acc_value),
        (AccumulatorSize, acc_size),
        (BatchBuffer, Vec::new()),
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

fn update_accumulator_batch(current: &[u8; 32], elements: &[AccumulatorElement]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(current);
    
    // Hash all elements together rather than one by one
    // This is more efficient for batch processing
    let mut batch_bytes = Vec::with_capacity(elements.len() * 100); // Estimate size
    for element in elements {
        batch_bytes.extend_from_slice(&borsh::to_vec(element).unwrap());
    }
    hasher.update(&batch_bytes);
    
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
    // If witness belongs to a batch, verify against the batch accumulator
    // Otherwise fall back to standard verification
    if witness.batch_id.is_some() {
        let mut hasher = Sha256::new();
        hasher.update(&witness.value);
        hasher.update(&acc_value);
        hasher.update(&witness.last_accumulator);
        witness.value == hasher.finalize().into()
    } else {
        // Standard verification for non-batched witnesses
        let mut hasher = Sha256::new();
        hasher.update(&witness.value);
        hasher.update(&acc_value);
        hasher.update(&witness.last_accumulator);
        witness.value == hasher.finalize().into()
    }
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
            batch_size: 10,
            batch_timeout_secs: 5,
        };

        let ctx = &mut sim;
        init(ctx, Some(params)).unwrap();

        let executor = Address::new([1; 33]);
        sim.set_actor(executor);

        (sim, executor)
    }

    #[test]
    fn test_batch_processing() {
        let (mut sim, executor) = setup();
        let ctx = &mut sim;
        
        // Create multiple attestations
        for i in 0..15 {
            let attestation = AttestationReport {
                enclave_type: if i % 2 == 0 { EnclaveType::IntelSGX } else { EnclaveType::AMDSEV },
                measurement: [i as u8; 32],
                timestamp: ctx.timestamp() + i,
                platform_data: vec![i as u8],
            };
            
            register_attestation(ctx, attestation).unwrap();
        }
        
        // Verify that batching occurred
        let batch_buffer = ctx.get(BatchBuffer).unwrap().unwrap();
        assert!(batch_buffer.len() < 15, "Should have processed at least one batch");
        
        // Force batch processing of remaining items
        force_batch_update(ctx).unwrap();
        
        // Verify that all items were processed
        let batch_buffer = ctx.get(BatchBuffer).unwrap().unwrap();
        assert_eq!(batch_buffer.len(), 0, "All items should be processed");
        
        // Check accumulator size
        let acc_size = ctx.get(AccumulatorSize).unwrap().unwrap();
        assert_eq!(acc_size, 15, "Accumulator should have 15 items");
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
        
        // Force batch processing
        force_batch_update(ctx).unwrap();

        // Verify attestations
        assert!(verify_attestation(ctx, sgx_attestation, sev_attestation).unwrap());
    }
    
    #[test]
    fn test_performance() {
        let (mut sim, executor) = setup();
        let ctx = &mut sim;
        
        // Batch size for performance test
        const BATCH_SIZE: usize = 100;
        
        // Prepare batch of attestations
        let mut attestations = Vec::with_capacity(BATCH_SIZE);
        for i in 0..BATCH_SIZE {
            attestations.push(AttestationReport {
                enclave_type: if i % 2 == 0 { EnclaveType::IntelSGX } else { EnclaveType::AMDSEV },
                measurement: [i as u8; 32],
                timestamp: ctx.timestamp() + i as u64,
                platform_data: vec![i as u8],
            });
        }
        
        // Measure time for batch processing
        let start = std::time::Instant::now();
        
        for attestation in attestations {
            register_attestation(ctx, attestation).unwrap();
        }
        
        // Force process any remaining items
        force_batch_update(ctx).unwrap();
        
        let duration = start.elapsed();
        println!("Processing {} attestations took: {:?}", BATCH_SIZE, duration);
        
        // Check accumulator size
        let acc_size = ctx.get(AccumulatorSize).unwrap().unwrap();
        assert_eq!(acc_size, BATCH_SIZE as u64, "Accumulator should have correct size");
    }
}
