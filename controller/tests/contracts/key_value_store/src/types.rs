use wasmlanche::borsh::{BorshDeserialize, BorshSerialize};
use wasmlanche::state::StateKey;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct StoreParams {
    pub key: String,
    pub value: Vec<u8>,
}

impl StateKey for StoreParams {
    fn key(&self) -> Vec<u8> {
        self.key.as_bytes().to_vec()
    }

    fn key_static() -> Vec<u8> {
        b"store_params_default".to_vec()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct GetParams {
    pub key: String,
}

impl StateKey for GetParams {
    fn key(&self) -> Vec<u8> {
        self.key.as_bytes().to_vec()
    }

    fn key_static() -> Vec<u8> {
        b"get_params_default".to_vec()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct DeleteParams {
    pub key: String,
}

impl StateKey for DeleteParams {
    fn key(&self) -> Vec<u8> {
        self.key.as_bytes().to_vec()
    }

    fn key_static() -> Vec<u8> {
        b"delete_params_default".to_vec()
    }
}
