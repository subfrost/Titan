use std::sync::{Arc, RwLock};
use tracing::error;

/// Represents the current state of the TCP connection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Initial state or deliberately disconnected
    Disconnected,
    /// Currently attempting to connect
    Connecting,
    /// Successfully connected to the server
    Connected,
    /// Connection was lost, attempting to reconnect
    Reconnecting,
}

/// Thread-safe wrapper for tracking and updating connection status
#[derive(Debug)]
pub struct ConnectionStatusTracker {
    status: Arc<RwLock<ConnectionStatus>>,
}

impl ConnectionStatusTracker {
    /// Creates a new ConnectionStatusTracker with the initial status set to Disconnected
    pub fn new() -> Self {
        Self {
            status: Arc::new(RwLock::new(ConnectionStatus::Disconnected)),
        }
    }

    /// Creates a new ConnectionStatusTracker with the specified initial status
    pub fn with_status(initial_status: ConnectionStatus) -> Self {
        Self {
            status: Arc::new(RwLock::new(initial_status)),
        }
    }

    /// Get the current connection status
    pub fn get_status(&self) -> ConnectionStatus {
        match self.status.read() {
            Ok(status) => *status,
            Err(_) => {
                // If the lock is poisoned, default to Disconnected
                error!("Failed to read connection status due to poisoned lock");
                ConnectionStatus::Disconnected
            }
        }
    }

    /// Update the connection status
    pub fn update_status(&self, new_status: ConnectionStatus) {
        if let Ok(mut status_guard) = self.status.write() {
            *status_guard = new_status;
        } else {
            error!("Failed to update connection status due to poisoned lock");
        }
    }

    /// Create a helper closure that can be used to update the status
    /// This is useful for passing to functions that need to update the status
    pub fn create_updater<'a>(&'a self) -> impl Fn(ConnectionStatus) + 'a {
        let status = Arc::clone(&self.status);
        move |new_status| {
            if let Ok(mut status_guard) = status.write() {
                *status_guard = new_status;
            } else {
                error!("Failed to update connection status due to poisoned lock");
            }
        }
    }

    /// Get a clone of the inner Arc<RwLock<ConnectionStatus>>
    /// This is useful when you need to share the status across threads
    pub fn get_inner(&self) -> Arc<RwLock<ConnectionStatus>> {
        Arc::clone(&self.status)
    }
}

impl Default for ConnectionStatusTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ConnectionStatusTracker {
    fn clone(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }
} 