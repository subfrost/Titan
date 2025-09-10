use metashrew_core::index_pointer::AtomicPointer;
use metashrew_support::index_pointer::KeyValuePointer;
use protorune_support::balance_sheet::ProtoruneStore;

#[derive(Clone)]
pub struct AlkanesProtoruneStore(pub AtomicPointer);

use std::sync::Arc;

impl ProtoruneStore for AlkanesProtoruneStore {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        let value = self.0.select(&key.to_vec()).get();
        if value.is_empty() {
            None
        } else {
            Some(value.to_vec())
        }
    }
}

impl KeyValuePointer for AlkanesProtoruneStore {
    fn wrap(word: &Vec<u8>) -> Self {
        Self(AtomicPointer::wrap(word))
    }
    fn unwrap(&self) -> Arc<Vec<u8>> {
        self.0.unwrap()
    }
    fn get(&self) -> Arc<Vec<u8>> {
        self.0.get()
    }
    fn set(&mut self, v: Arc<Vec<u8>>) {
        self.0.set(v)
    }
    fn inherits(&mut self, from: &Self) {
        self.0.inherits(&from.0)
    }
}

impl Default for AlkanesProtoruneStore {
    fn default() -> Self {
        Self(AtomicPointer::default())
    }
}