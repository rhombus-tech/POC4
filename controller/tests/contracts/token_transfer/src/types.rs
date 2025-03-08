use wasmlanche::borsh::{BorshDeserialize, BorshSerialize};
use wasmlanche::{state::StateKey, types::WasmlAddress};
use alloc::vec::Vec;

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct TransferParams {
    pub to: WasmlAddress,
    pub amount: u64,
}

impl StateKey for TransferParams {
    fn key(&self) -> Vec<u8> {
        // Use a prefix and the address
        let mut key = b"transfer_".to_vec();
        key.extend_from_slice(self.to.as_bytes());
        key
    }

    fn key_static() -> Vec<u8> {
        b"transfer_default".to_vec()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct BalanceParams {
    pub address: WasmlAddress,
}

impl StateKey for BalanceParams {
    fn key(&self) -> Vec<u8> {
        // Use a prefix and the address
        let mut key = b"balance_".to_vec();
        key.extend_from_slice(self.address.as_bytes());
        key
    }

    fn key_static() -> Vec<u8> {
        b"balance_default".to_vec()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct Balance {
    pub address: WasmlAddress,
    pub amount: u64,
}

impl StateKey for Balance {
    fn key(&self) -> Vec<u8> {
        // Use a prefix and the address
        let mut key = b"balance_".to_vec();
        key.extend_from_slice(self.address.as_bytes());
        key
    }

    fn key_static() -> Vec<u8> {
        b"balance_default".to_vec()
    }
}
