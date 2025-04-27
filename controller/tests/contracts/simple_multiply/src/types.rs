use wasmlanche::borsh::{BorshDeserialize, BorshSerialize};
use wasmlanche::state::StateKey;
use alloc::vec::Vec;

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct MultiplyParams {
    pub a: u64,
    pub b: u64,
}

impl StateKey for MultiplyParams {
    fn key(&self) -> Vec<u8> {
        // Create a key that incorporates both values
        let mut key = b"multiply_".to_vec();
        key.extend_from_slice(&self.a.to_le_bytes());
        key.extend_from_slice(&self.b.to_le_bytes());
        key
    }

    fn key_static() -> Vec<u8> {
        b"multiply_default".to_vec()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct MultiplyResult {
    pub a: u64,
    pub b: u64,
    pub result: u64,
}

impl StateKey for MultiplyResult {
    fn key(&self) -> Vec<u8> {
        // Create a key that incorporates both input values
        let mut key = b"multiply_result_".to_vec();
        key.extend_from_slice(&self.a.to_le_bytes());
        key.extend_from_slice(&self.b.to_le_bytes());
        key
    }

    fn key_static() -> Vec<u8> {
        b"multiply_result_default".to_vec()
    }
}
