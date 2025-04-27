use borsh::{BorshDeserialize, BorshSerialize};

#[derive(BorshDeserialize, BorshSerialize)]
pub struct AddParams {
    pub a: u64,
    pub b: u64,
}
