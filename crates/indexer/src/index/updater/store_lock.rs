use {
    crate::index::store::Store,
    std::sync::{Arc, RwLock},
};

pub struct StoreWithLock(RwLock<Arc<dyn Store + Send + Sync>>);

impl StoreWithLock {
    pub fn new(db: Arc<dyn Store + Send + Sync>) -> Self {
        Self(RwLock::new(db))
    }

    pub fn read(&self) -> Arc<dyn Store + Send + Sync> {
        let result = self.0.read();
        match result {
            Ok(db) => db.clone(),
            Err(e) => panic!("failed to read db: {e}"),
        }
    }

    pub fn write(&self) -> Arc<dyn Store + Send + Sync> {
        let result = self.0.write();
        match result {
            Ok(db) => db.clone(),
            Err(e) => panic!("failed to write db: {e}"),
        }
    }
}
