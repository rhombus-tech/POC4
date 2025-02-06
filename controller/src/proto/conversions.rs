use tee_interface::types::{ExecutionResult as InterfaceResult, TeeAttestation as InterfaceAttestation, TeeType};
use tee_interface::ExecutionStats;
use super::teeservice::{ExecutionResult as ProtoResult, TeeAttestation as ProtoAttestation};
use chrono::{DateTime, TimeZone, Utc};

pub fn to_interface_execution_result(proto: &ProtoResult) -> InterfaceResult {
    InterfaceResult {
        result: proto.result.clone(),
        state_hash: proto.state_hash.clone(),
        stats: ExecutionStats {
            execution_time: proto.execution_time,
            memory_used: proto.memory_used,
            syscall_count: proto.syscall_count,
        },
        attestations: proto.attestations.iter().map(to_interface_attestation).collect(),
        timestamp: proto.timestamp.clone(),
    }
}

pub fn to_proto_execution_result(interface: &InterfaceResult) -> ProtoResult {
    ProtoResult {
        result: interface.result.clone(),
        state_hash: interface.state_hash.clone(),
        execution_time: interface.stats.execution_time,
        memory_used: interface.stats.memory_used,
        syscall_count: interface.stats.syscall_count,
        attestations: interface.attestations.iter().map(to_proto_attestation).collect(),
        timestamp: interface.timestamp.clone(),
    }
}

pub fn to_interface_attestation(proto: &ProtoAttestation) -> InterfaceAttestation {
    InterfaceAttestation {
        enclave_id: proto.enclave_id.clone(),
        measurement: proto.measurement.clone(),
        timestamp: DateTime::parse_from_rfc3339(&proto.timestamp)
            .map(|dt| dt.timestamp())
            .unwrap_or_else(|_| Utc::now().timestamp()) as u64,
        data: proto.data.clone(),
        signature: proto.signature.clone(),
        region_proof: Some(proto.region_proof.clone()),
        enclave_type: match proto.enclave_type.as_str() {
            "SGX" => TeeType::SGX,
            "SEV" => TeeType::SEV,
            _ => TeeType::SGX,
        },
    }
}

pub fn to_proto_attestation(interface: &InterfaceAttestation) -> ProtoAttestation {
    ProtoAttestation {
        enclave_id: interface.enclave_id.clone(),
        measurement: interface.measurement.clone(),
        timestamp: Utc.timestamp_opt(interface.timestamp as i64, 0)
            .single()
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| Utc::now().to_rfc3339()),
        data: interface.data.clone(),
        signature: interface.signature.clone(),
        region_proof: interface.region_proof.clone().unwrap_or_default(),
        enclave_type: match interface.enclave_type {
            TeeType::SGX => "SGX".to_string(),
            TeeType::SEV => "SEV".to_string(),
        },
    }
}

use tee_interface::{ExecutionParams, TeeAttestation, Region};
use crate::proto::teeservice;

impl From<ExecutionParams> for teeservice::ExecutionRequest {
    fn from(params: ExecutionParams) -> Self {
        Self {
            id_to: params.id_to,
            function_call: params.function_call,
            parameters: vec![],
            region_id: String::new(),
            detailed_proof: params.detailed_proof,
            expected_hash: params.expected_hash,
        }
    }
}

impl From<teeservice::ExecutionRequest> for ExecutionParams {
    fn from(req: teeservice::ExecutionRequest) -> Self {
        Self {
            id_to: req.id_to,
            function_call: req.function_call,
            detailed_proof: req.detailed_proof,
            expected_hash: req.expected_hash,
        }
    }
}

impl From<TeeAttestation> for teeservice::TeeAttestation {
    fn from(att: TeeAttestation) -> Self {
        Self {
            enclave_id: att.enclave_id,
            measurement: att.measurement,
            timestamp: Utc.timestamp_opt(att.timestamp as i64, 0)
                .single()
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| Utc::now().to_rfc3339()),
            data: att.data,
            signature: att.signature,
            region_proof: att.region_proof.unwrap_or_default(),
            enclave_type: match att.enclave_type {
                TeeType::SGX => "SGX",
                TeeType::SEV => "SEV",
            }.to_string(),
        }
    }
}

impl From<Region> for teeservice::Region {
    fn from(region: Region) -> Self {
        Self {
            id: region.id,
            created_at: chrono::Utc::now().to_rfc3339(),
            worker_ids: region.worker_ids,
            supported_tee_types: vec!["SGX".to_string()],
            max_tasks: region.max_tasks,
        }
    }
}
