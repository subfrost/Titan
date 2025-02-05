use std::sync::Arc;
use tokio::{
    sync::{watch, Mutex},
    task::JoinHandle,
};
use tracing::{error, info};

use super::zmq_listener;
use crate::index::updater::Updater;

/// Manages the async ZMQ listener task
pub struct ZmqManager {
    /// When set to true, the listener should gracefully stop
    shutdown_tx: watch::Sender<bool>,
    /// Receives updates of the shutdown flag
    shutdown_rx: watch::Receiver<bool>,

    /// Handle to the spawned ZMQ listener task
    zmq_task_handle: Mutex<Option<JoinHandle<()>>>,

    /// The ZMQ endpoint string (e.g., "tcp://127.0.0.1:28332")
    zmq_endpoint: String,
}

impl ZmqManager {
    /// Create a new manager.
    pub fn new(zmq_endpoint: String) -> Self {
        // Create the watch channel. Initial value = false => not shutting down yet.
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        Self {
            shutdown_tx,
            shutdown_rx,
            zmq_task_handle: Mutex::new(None),
            zmq_endpoint,
        }
    }

    /// Set the watch flag to `true`, telling the task to shut down.
    pub fn shutdown(&self) {
        // We ignore errors if the receiver is already dropped.
        let _ = self.shutdown_tx.send(true);
    }

    /// Start the ZMQ listener in a Tokio task
    pub async fn start_zmq_listener(&self, updater: Arc<Updater>) {
        let zmq_endpoint = self.zmq_endpoint.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        // Spawn the async listener on the Tokio runtime
        let handle = tokio::spawn(async move {
            if let Err(e) = zmq_listener(updater, zmq_endpoint, &mut shutdown_rx).await {
                error!("ZMQ listener error: {:?}", e);
            }
        });

        let mut guard = self.zmq_task_handle.lock().await;
        *guard = Some(handle);
    }

    /// Await the ZMQ listener task
    pub async fn join_zmq_listener(&self) {
        let mut guard = self.zmq_task_handle.lock().await;
        if let Some(handle) = guard.take() {
            match handle.await {
                Ok(_) => info!("ZMQ listener task shut down cleanly."),
                Err(e) => error!("ZMQ listener task join error: {:?}", e),
            }
        }
    }
}
