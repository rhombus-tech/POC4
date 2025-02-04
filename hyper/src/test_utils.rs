use crate::runtime::StateManager;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::RwLock;
use tee_interface::prelude::*;

#[derive(Debug, Default)]
pub struct MockStateManager {
    pub states: RwLock<HashMap<[u8; 32], Vec<u8>>>,
}

#[async_trait]
impl StateManager for MockStateManager {
    async fn get_state(&self, contract_id: [u8; 32]) -> Result<Vec<u8>> {
        Ok(self.states.read().unwrap().get(&contract_id).cloned().unwrap_or_default())
    }

    async fn set_state(&self, contract_id: [u8; 32], state: Vec<u8>) -> Result<()> {
        self.states.write().unwrap().insert(contract_id, state);
        Ok(())
    }
}
