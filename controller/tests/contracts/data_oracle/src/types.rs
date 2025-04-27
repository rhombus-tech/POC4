use wasmlanche::borsh::{BorshDeserialize, BorshSerialize};
use wasmlanche::state::StateKey;
use alloc::{string::String, vec::Vec};

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct QueryParams {
    pub asset: String,
}

impl StateKey for QueryParams {
    fn key(&self) -> Vec<u8> {
        // Use a prefix and the asset name
        let mut key = b"query_".to_vec();
        key.extend_from_slice(self.asset.as_bytes());
        key
    }

    fn key_static() -> Vec<u8> {
        b"query_default".to_vec()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct PriceData {
    pub price: u64,
    pub timestamp: u64,
    pub source: String,
}

impl StateKey for PriceData {
    fn key(&self) -> Vec<u8> {
        // Use a prefix and the source
        let mut key = b"price_".to_vec();
        key.extend_from_slice(self.source.as_bytes());
        key
    }

    fn key_static() -> Vec<u8> {
        b"price_default".to_vec()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Default, Clone)]
pub struct AssetPrice {
    pub asset: String,
    pub price_data: PriceData,
}

impl StateKey for AssetPrice {
    fn key(&self) -> Vec<u8> {
        // Use a prefix and the asset
        let mut key = b"asset_price_".to_vec();
        key.extend_from_slice(self.asset.as_bytes());
        key
    }

    fn key_static() -> Vec<u8> {
        b"asset_price_default".to_vec()
    }
}
