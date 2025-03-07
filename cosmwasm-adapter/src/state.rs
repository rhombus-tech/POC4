use cosmwasm_std::{Storage, Record};
use cosmwasm_storage::{PrefixedStorage, ReadonlyPrefixedStorage};
use std::ops::Deref;

/// Wraps CosmWasm storage to provide a unified interface
pub struct StorageAdapter<'a> {
    storage: &'a mut dyn Storage,
    prefix: Vec<u8>,
}

impl<'a> StorageAdapter<'a> {
    pub fn new(storage: &'a mut dyn Storage, prefix: Vec<u8>) -> Self {
        Self { storage, prefix }
    }

    pub fn with_prefix(&mut self, prefix: Vec<u8>) -> StorageAdapter {
        let mut new_prefix = self.prefix.clone();
        new_prefix.extend(prefix);
        StorageAdapter {
            storage: self.storage,
            prefix: new_prefix,
        }
    }

    pub fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let mut prefixed = ReadonlyPrefixedStorage::new(self.storage, &self.prefix);
        prefixed.get(key)
    }

    pub fn set(&mut self, key: &[u8], value: &[u8]) {
        let mut prefixed = PrefixedStorage::new(self.storage, &self.prefix);
        prefixed.set(key, value);
    }

    pub fn remove(&mut self, key: &[u8]) {
        let mut prefixed = PrefixedStorage::new(self.storage, &self.prefix);
        prefixed.remove(key);
    }

    pub fn range<'b>(
        &'b self,
        start: Option<&[u8]>,
        end: Option<&[u8]>,
        order: cosmwasm_std::Order,
    ) -> Box<dyn Iterator<Item = Record> + 'b> {
        let prefixed = ReadonlyPrefixedStorage::new(self.storage, &self.prefix);
        Box::new(prefixed.range(start, end, order))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Add tests here
}
