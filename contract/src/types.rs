use wasmlanche::Address;
use tee_interface::prelude::*;
use borsh::{BorshSerialize, BorshDeserialize};

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, PartialEq)]
pub enum Phase {
    Registration,
    Active,
    Completed,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct ExecutorMetadata {
    pub address: Address,
    pub tee_type: TeeType,
    pub measurement: Vec<u8>,
    pub status: ExecutorStatus,
    pub last_attestation: u64,
    pub execution_count: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum ExecutorStatus {
    Active,
    Suspended,
    Terminated,
}

impl std::fmt::Debug for ExecutorMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutorMetadata")
            .field("tee_type", &self.tee_type)
            .field("status", &self.status)
            .field("last_attestation", &self.last_attestation)
            .field("execution_count", &self.execution_count)
            .finish()
    }
}
