use std::{sync::Arc, thread, time::Instant};

use crossbeam_channel::{bounded, Sender};
use tracing::{debug, error, info};

use crate::{
    index::updater::store_lock::StoreWithLock,
    models::{BatchDelete, BatchUpdate},
};

pub struct BatchDB {
    pub update: BatchUpdate,
    pub delete: BatchDelete,
}

pub struct BgWriterSettings {
    pub max_async_batches: usize,
}

pub struct BgWriter {
    handle: Option<std::thread::JoinHandle<()>>,
    sender: Option<Sender<Arc<BatchDB>>>,
    pending_batches: Vec<Arc<BatchDB>>,
}

impl BgWriter {
    pub fn start(db: Arc<StoreWithLock>, settings: BgWriterSettings) -> Self {
        let (tx, rx) = bounded::<Arc<BatchDB>>(settings.max_async_batches);

        let handle = thread::Builder::new()
            .name("rocksdb-writer".into())
            .spawn(move || {
                while let Ok(batch) = rx.recv() {
                    let store = db.write();
                    let start = Instant::now();

                    // Safety: Only background thread writes via batch_update.
                    if let Err(e) = store.batch_update(&batch.update, false) {
                        tracing::error!("Background RocksDB write failed: {:?}", e);
                    }

                    debug!("Flushed update: {} in {:?}", batch.update, start.elapsed());

                    let start = Instant::now();

                    if let Err(e) = store.batch_delete(&batch.delete) {
                        tracing::error!("Background RocksDB write failed: {:?}", e);
                    }

                    debug!("Flushed delete: {} in {:?}", batch.delete, start.elapsed());
                }
            })
            .expect("failed to spawn rocksdb-writer thread");

        Self {
            handle: Some(handle),
            sender: Some(tx),
            pending_batches: Vec::new(),
        }
    }

    /// Search a value inside the currently pending (still in–flight) batch updates.
    /// The provided closure will be executed on each `BatchUpdate` starting from the
    /// most-recent one (the last pushed into `pending_batches`). The first `Some` value
    /// returned will be forwarded. This is a generic utility that avoids repeating the
    /// same lookup logic for every data type we need to read while a background write
    /// is ongoing.
    pub fn find_in_pending<T, F>(&self, finder: F) -> Option<T>
    where
        F: Fn(&BatchUpdate) -> Option<T>,
    {
        // Iterate newest → oldest to make sure we prefer the most recently flushed batch.
        for batch in self.pending_batches.iter().rev() {
            if let Some(val) = finder(&batch.update) {
                return Some(val);
            }
        }
        None
    }

    pub fn save(&mut self, batch: Arc<BatchDB>) {
        self.cleanup_completed_batches();
        self.pending_batches.push(batch.clone());

        if let Some(sender) = &self.sender {
            if let Err(e) = sender.send(batch) {
                error!("Failed to send batch to background writer: {:?}", e);
            }
        }
    }

    fn cleanup_completed_batches(&mut self) {
        self.pending_batches.retain(|b| Arc::strong_count(b) > 1);
    }
}

impl Drop for BgWriter {
    fn drop(&mut self) {
        // Close the sender side of the channel first so the background
        // thread can finish processing and exit its loop.
        if let Some(sender) = self.sender.take() {
            drop(sender);
        }

        // Wait for the background writer thread to finish to guarantee that
        // all pending updates have been flushed before shutting down.
        if let Some(handle) = self.handle.take() {
            if let Err(err) = handle.join() {
                tracing::error!("Failed to join rocksdb-writer thread: {:?}", err);
            }
        }
    }
}
