use tee_interface::{ExecutionResult as InterfaceResult, ExecutionStats, TeeAttestation as InterfaceAttestation, TeeType, ExecutionPayload};
use super::teeservice::{ExecutionResult as ProtoResult, TeeAttestation as ProtoAttestation, ExecutionRequest, GetAttestationsRequest};
use chrono::{DateTime, Utc};

pub fn to_interface_result(proto: ProtoResult) -> InterfaceResult {
    InterfaceResult {
        result: proto.result,
        state_hash: proto.state_hash,
        stats: ExecutionStats {
            execution_time: proto.execution_time,
            memory_used: proto.memory_used,
            syscall_count: proto.syscall_count,
        },
        attestation: proto.attestations.first()
            .map(to_interface_attestation)
            .unwrap_or_else(|| InterfaceAttestation {
                enclave_id: [0u8; 32],
                measurement: vec![],
                data: vec![],
                signature: vec![],
                region_proof: None,
                timestamp: 0,
                enclave_type: TeeType::SGX,
            }),
    }
}

pub fn to_interface_attestation(proto: &ProtoAttestation) -> InterfaceAttestation {
    let mut enclave_id = [0u8; 32];
    if proto.enclave_id.len() >= 32 {
        enclave_id.copy_from_slice(&proto.enclave_id[..32]);
    }

    InterfaceAttestation {
        enclave_id,
        measurement: proto.measurement.clone(),
        data: proto.data.clone(),
        signature: proto.signature.clone(),
        region_proof: Some(proto.region_proof.clone()),
        timestamp: DateTime::parse_from_rfc3339(&proto.timestamp)
            .map(|dt| dt.timestamp())
            .unwrap_or_default() as u64,
        enclave_type: match proto.enclave_type.as_str() {
            "SGX" => TeeType::SGX,
            "SEV" => TeeType::SEV,
            _ => TeeType::SGX,
        },
    }
}

pub fn to_proto_attestation(interface: &InterfaceAttestation) -> ProtoAttestation {
    ProtoAttestation {
        enclave_id: interface.enclave_id.to_vec(),
        measurement: interface.measurement.clone(),
        data: interface.data.clone(),
        signature: interface.signature.clone(),
        region_proof: interface.region_proof.clone().unwrap_or_default(),
        timestamp: Utc::now().to_rfc3339(),
        enclave_type: match interface.enclave_type {
            TeeType::SGX => "SGX".to_string(),
            TeeType::SEV => "SEV".to_string(),
        },
    }
}

pub fn to_proto_result(interface: InterfaceResult) -> ProtoResult {
    ProtoResult {
        timestamp: Utc::now().to_rfc3339(),
        attestations: vec![to_proto_attestation(&interface.attestation)],
        state_hash: interface.state_hash,
        result: interface.result,
        execution_time: interface.stats.execution_time,
        memory_used: interface.stats.memory_used,
        syscall_count: interface.stats.syscall_count,
    }
}

pub fn create_execution_request(payload: &ExecutionPayload, id_to: &str) -> ExecutionRequest {
    ExecutionRequest {
        id_to: id_to.to_string(),
        function_call: "execute".to_string(),
        parameters: payload.input.clone(),
        region_id: "default".to_string(),
        detailed_proof: true,
        expected_hash: vec![],
    }
}

pub fn create_get_attestations_request() -> GetAttestationsRequest {
    GetAttestationsRequest {
        region_id: "default".to_string(),
    }
}
