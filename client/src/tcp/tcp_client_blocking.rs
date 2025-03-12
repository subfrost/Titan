use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use serde_json;
use thiserror::Error;
use titan_types::{Event, TcpSubscriptionRequest};
use tracing::{error, info, warn};

use crate::tcp::reconnection::ReconnectionManager;

use super::{
    connection_status::{ConnectionStatus, ConnectionStatusTracker},
    reconnection,
};

#[derive(Debug, Error)]
pub enum TcpClientError {
    #[error("io error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("serde error: {0}")]
    SerdeError(#[from] serde_json::Error),
    #[error("address parse error: {0}")]
    AddrParseError(#[from] std::net::AddrParseError),
}
/// Configuration for TCP client reconnection.
#[derive(Debug, Clone)]
pub struct TcpClientConfig {
    /// Base duration for reconnect interval (will be used with exponential backoff)
    pub base_reconnect_interval: Duration,
    /// Maximum reconnect interval (cap for exponential backoff)
    pub max_reconnect_interval: Duration,
    /// Maximum number of reconnection attempts.
    /// Use `None` for unlimited attempts.
    pub max_reconnect_attempts: Option<u32>,
    /// Connection timeout.
    pub connection_timeout: Duration,
    /// Initial capacity of the read buffer (in bytes)
    pub read_buffer_capacity: usize,
    /// Maximum allowed size for the read buffer (in bytes)
    pub max_buffer_size: usize,
}

impl Default for TcpClientConfig {
    fn default() -> Self {
        TcpClientConfig {
            base_reconnect_interval: Duration::from_secs(1),
            max_reconnect_interval: Duration::from_secs(60),
            max_reconnect_attempts: None,
            connection_timeout: Duration::from_secs(30),
            read_buffer_capacity: 4096,        // 4KB initial capacity
            max_buffer_size: 10 * 1024 * 1024, // 10MB max buffer size
        }
    }
}

/// Synchronous TCP subscription listener with reconnection support.
///
/// Connects to the TCP server at `addr` and sends the given subscription request
/// (encoded as JSON). It then spawns a dedicated thread that reads lines from the TCP
/// connection. If the connection drops or an error occurs, it will attempt to reconnect
/// according to the configuration settings.
///
/// # Thread Management
///
/// This client spawns a background thread to handle the TCP connection and event processing.
/// To ensure proper cleanup, you should call `shutdown_and_join()` when you're done with the
/// client. If you don't call this method, the background thread will be automatically
/// signaled to shut down when the `TcpClient` is dropped, but the thread may continue
/// running briefly after the client is dropped.
///
/// ```
/// # use client::tcp_client_blocking::{TcpClient, TcpClientConfig};
/// # fn main() {
/// let client = TcpClient::new(TcpClientConfig::default());
/// // Use the client...
///
/// // When done, ensure clean shutdown
/// client.shutdown_and_join();
/// # }
/// ```
#[cfg(feature = "tcp_client_blocking")]
pub struct TcpClient {
    shutdown_flag: Arc<AtomicBool>,
    config: TcpClientConfig,
    status_tracker: ConnectionStatusTracker,
    worker_thread: Mutex<Option<JoinHandle<()>>>,
}

#[cfg(feature = "tcp_client_blocking")]
impl TcpClient {
    /// Creates a new TCP client with the given configuration.
    pub fn new(config: TcpClientConfig) -> Self {
        Self {
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            config,
            status_tracker: ConnectionStatusTracker::new(),
            worker_thread: Mutex::new(None),
        }
    }

    /// Get the current connection status
    pub fn get_status(&self) -> ConnectionStatus {
        self.status_tracker.get_status()
    }

    /// Get whether the client was disconnected at any point in time
    pub fn was_disconnected(&self) -> bool {
        self.status_tracker.was_disconnected()
    }

    /// Reset the disconnected flag
    pub fn reset_disconnected(&self) {
        self.status_tracker.reset_disconnected();
    }

    /// Checks if there is an active worker thread.
    ///
    /// Returns true if a worker thread is currently running.
    pub fn has_active_thread(&self) -> bool {
        match self.worker_thread.lock() {
            Ok(lock) => lock.is_some(),
            Err(_) => {
                error!("Failed to acquire worker thread lock");
                false
            }
        }
    }

    /// Subscribe to events from the given address.
    ///
    /// This will spawn a background thread that connects to the server and
    /// listens for events. The events will be sent to the returned channel.
    ///
    /// If there's already an active worker thread, it will be shut down and
    /// a new one will be created.
    pub fn subscribe(
        &self,
        addr: String,
        subscription_request: TcpSubscriptionRequest,
    ) -> Result<mpsc::Receiver<Event>, TcpClientError> {
        // Check if we already have a worker thread running
        let mut worker_lock = self.worker_thread.lock().map_err(|_| {
            TcpClientError::IOError(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Failed to acquire worker thread lock",
            ))
        })?;

        // Reset shutdown flag in case it was previously set
        self.shutdown_flag.store(false, Ordering::SeqCst);

        let shutdown_flag = self.shutdown_flag.clone();
        let config = self.config.clone();
        let status_tracker = self.status_tracker.clone();

        // Call the subscribe function which returns both the receiver and thread handle
        let (rx, handle) = subscribe(
            addr,
            subscription_request,
            shutdown_flag,
            config,
            status_tracker,
        )?;

        // Store the thread handle for later joining
        *worker_lock = Some(handle);

        Ok(rx)
    }

    /// Signals the client to shut down and stop any reconnection attempts.
    /// Does not wait for the worker thread to complete.
    pub fn shutdown(&self) {
        self.status_tracker
            .update_status(ConnectionStatus::Disconnected);
        self.shutdown_flag.store(true, Ordering::SeqCst);
    }

    /// Signals the client to shut down and waits for the worker thread to complete.
    /// Returns true if the thread was successfully joined, false otherwise.
    pub fn shutdown_and_join(&self) -> bool {
        // Signal shutdown
        self.shutdown();

        // Try to join the thread
        self.join()
    }

    /// Waits for the worker thread to complete.
    /// Returns true if the thread was successfully joined, false otherwise.
    pub fn join(&self) -> bool {
        // Acquire the lock on the worker thread
        let mut worker_lock = match self.worker_thread.lock() {
            Ok(lock) => lock,
            Err(e) => {
                error!("Failed to acquire worker thread lock: {}", e);
                return false;
            }
        };

        // Take the thread handle out (replacing it with None)
        if let Some(handle) = worker_lock.take() {
            match handle.join() {
                Ok(_) => {
                    info!("Successfully joined worker thread");
                    true
                }
                Err(e) => {
                    error!("Failed to join worker thread: {:?}", e);
                    false
                }
            }
        } else {
            // No thread to join
            false
        }
    }
}

#[cfg(feature = "tcp_client_blocking")]
impl Drop for TcpClient {
    fn drop(&mut self) {
        // Signal thread to terminate
        self.shutdown();

        // Attempt to join the thread directly in the destructor
        // This is safe because we're taking ownership of the JoinHandle
        if let Ok(mut worker_lock) = self.worker_thread.lock() {
            if let Some(handle) = worker_lock.take() {
                // Don't block for too long in a destructor - it's generally not good practice
                // Just log that we're not waiting for the thread
                info!("TcpClient dropped, thread will continue running until shutdown completes");
            }
        }
        // The shutdown flag has been set, so the thread will terminate naturally
    }
}

fn subscribe(
    addr: String,
    subscription_request: TcpSubscriptionRequest,
    shutdown_flag: Arc<AtomicBool>,
    config: TcpClientConfig,
    status_tracker: ConnectionStatusTracker,
) -> Result<(mpsc::Receiver<Event>, JoinHandle<()>), TcpClientError> {
    // Create a standard mpsc channel to forward events.
    let (tx, rx) = mpsc::channel::<Event>();

    let address = addr.parse()?;

    // Set initial status to Connecting
    status_tracker.update_status(ConnectionStatus::Connecting);

    // Create the reconnection manager
    let reconnection_config = reconnection::from_tcp_client_config(&config);

    let handle = thread::spawn(move || {
        // Create a status updater for use in the thread
        let update_status = status_tracker.create_updater();

        // Create the reconnection manager
        let mut reconnection_manager = ReconnectionManager::new(reconnection_config);

        loop {
            if shutdown_flag.load(Ordering::SeqCst) {
                info!("Shutdown flag set. Exiting subscription thread.");
                // Set status to disconnected
                update_status(ConnectionStatus::Disconnected);
                break;
            }

            // Try to connect to the server.
            info!("Attempting to connect to {}...", addr);
            // Ensure status is set to Connecting
            update_status(ConnectionStatus::Connecting);

            let connect_result = TcpStream::connect_timeout(&address, config.connection_timeout);

            match connect_result {
                Ok(mut stream) => {
                    info!("Connected to server at {}", addr);
                    // Update status to Connected
                    update_status(ConnectionStatus::Connected);

                    // Reset reconnection attempts after successful connection
                    reconnection_manager.reset();

                    // Set read timeout
                    if let Err(e) = stream.set_read_timeout(Some(Duration::from_millis(500))) {
                        error!("Failed to set read timeout: {}", e);
                        continue;
                    }

                    // Set write timeout
                    if let Err(e) = stream.set_write_timeout(Some(Duration::from_secs(5))) {
                        error!("Failed to set write timeout: {}", e);
                        continue;
                    }

                    // Clone the stream for reading.
                    let reader_stream = match stream.try_clone() {
                        Ok(rs) => rs,
                        Err(e) => {
                            error!("Failed to clone stream: {}", e);
                            continue;
                        }
                    };
                    let mut reader = BufReader::new(reader_stream);

                    // Serialize and send the subscription request.
                    match serde_json::to_string(&subscription_request) {
                        Ok(req_json) => {
                            if let Err(e) = stream.write_all(req_json.as_bytes()) {
                                error!("Failed to send subscription request: {}", e);
                                continue;
                            }
                            if let Err(e) = stream.write_all(b"\n") {
                                error!("Failed to send newline: {}", e);
                                continue;
                            }
                            if let Err(e) = stream.flush() {
                                error!("Failed to flush stream: {}", e);
                                continue;
                            }
                        }
                        Err(e) => {
                            error!("Failed to serialize subscription request: {}", e);
                            break;
                        }
                    }

                    // Initialize the line buffer with the configured capacity
                    let mut line = String::with_capacity(config.read_buffer_capacity);
                    let mut read_in_progress = false;
                    // Inner loop: read events from the connection.
                    loop {
                        if shutdown_flag.load(Ordering::SeqCst) {
                            info!("Shutdown flag set. Exiting inner read loop.");
                            // Update status to disconnected
                            update_status(ConnectionStatus::Disconnected);
                            break;
                        }
                        if !read_in_progress {
                            line.clear();
                        }

                        // Check if the buffer has grown too large
                        if line.len() > config.max_buffer_size {
                            error!("Buffer capacity exceeded maximum allowed size ({}), resetting connection.", config.max_buffer_size);
                            break;
                        }

                        match reader.read_line(&mut line) {
                            Ok(0) => {
                                // Connection closed by server.
                                warn!("TCP connection closed by server. Attempting to reconnect.");
                                // Update status to reconnecting
                                update_status(ConnectionStatus::Reconnecting);
                                break;
                            }
                            Ok(_) => {
                                read_in_progress = false;
                                let trimmed = line.trim();
                                if trimmed.is_empty() {
                                    continue;
                                }
                                match serde_json::from_str::<Event>(trimmed) {
                                    Ok(event) => {
                                        if tx.send(event).is_err() {
                                            error!(
                                                "Receiver dropped. Exiting subscription thread."
                                            );
                                            return;
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to parse event: {}. Line: {}", e, trimmed);
                                    }
                                }
                            }
                            Err(e) => {
                                if e.kind() == std::io::ErrorKind::TimedOut
                                    || e.kind() == std::io::ErrorKind::WouldBlock
                                {
                                    // These are normal timeout errors during idle periods
                                    if !line.is_empty() {
                                        read_in_progress = true;
                                        info!(
                                            "Partial read in progress ({} bytes so far), continuing...",
                                            line.len()
                                        );
                                    }
                                    continue;
                                } else {
                                    error!("Error reading from TCP socket: {}", e);
                                    read_in_progress = false;
                                    // For unexpected errors, add a small delay.
                                    thread::sleep(Duration::from_millis(100));
                                    // Update status to reconnecting
                                    update_status(ConnectionStatus::Reconnecting);
                                    break;
                                }
                            }
                        }
                    } // end inner loop for current connection

                    // When we exit the inner loop (connection lost)
                    // Update status to reconnecting
                    if !shutdown_flag.load(Ordering::SeqCst) {
                        update_status(ConnectionStatus::Reconnecting);
                    }
                }
                Err(e) => {
                    error!("Failed to connect to {}: {}", addr, e);
                    // Set status to reconnecting since we're going to try again
                    update_status(ConnectionStatus::Reconnecting);
                }
            }

            // Get the next delay from the reconnection manager
            match reconnection_manager.next_delay() {
                Some(wait_time) => {
                    info!(
                        "Reconnecting in {:?}... (attempt {}/{:?})",
                        wait_time,
                        reconnection_manager.current_attempt(),
                        reconnection_manager.config().max_attempts
                    );
                    thread::sleep(wait_time);
                }
                None => {
                    error!(
                        "Reached maximum reconnection attempts ({}). Exiting.",
                        reconnection_manager.config().max_attempts.unwrap()
                    );
                    // Set status to disconnected when max attempts reached
                    update_status(ConnectionStatus::Disconnected);
                    break;
                }
            }
        }
        info!("Exiting TCP subscription thread.");
        // Ensure status is Disconnected when thread exits
        update_status(ConnectionStatus::Disconnected);
    });

    Ok((rx, handle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpListener};
    use std::thread;
    use std::time::Duration;
    use titan_types::EventType;

    // Helper function to create a test TCP server
    fn start_test_server(ready_tx: std::sync::mpsc::Sender<SocketAddr>) -> JoinHandle<()> {
        thread::spawn(move || {
            // Bind to a random available port
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();

            // Notify the test that we're ready and send the address
            ready_tx.send(addr).unwrap();

            // Accept one connection
            if let Ok((mut stream, _)) = listener.accept() {
                // Read the subscription request
                let mut buffer = [0; 1024];
                match stream.read(&mut buffer) {
                    Ok(n) => {
                        let request = String::from_utf8_lossy(&buffer[..n]);
                        println!("Server received request: {}", request);

                        // Add a small delay to ensure the client is ready to receive
                        thread::sleep(Duration::from_millis(50));

                        // Send a sample event - using correct format for Event
                        let event = r#"{"type":"TransactionsAdded","data": {"txids":["1111111111111111111111111111111111111111111111111111111111111111"]}}"#;
                        stream.write_all(event.as_bytes()).unwrap();
                        stream.write_all(b"\n").unwrap();
                        stream.flush().unwrap();

                        // Keep the connection open for a while to ensure the client can read the response
                        thread::sleep(Duration::from_millis(500));
                    }
                    Err(e) => println!("Test server read error: {}", e),
                }
            }
        })
    }

    #[test]
    fn test_connection_status_transitions() {
        // Create a channel to sync with the test server
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        // Start a test server
        let server_handle = start_test_server(ready_tx);

        // Wait for the server to be ready and get its address
        let server_addr = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        // Create a client with short timeout
        let config = TcpClientConfig {
            connection_timeout: Duration::from_secs(1),
            max_reconnect_attempts: Some(1),
            base_reconnect_interval: Duration::from_millis(100),
            ..TcpClientConfig::default()
        };
        let client = TcpClient::new(config);

        // Initially disconnected
        assert_eq!(client.get_status(), ConnectionStatus::Disconnected);

        // Subscribe - this should connect
        let subscription_request = TcpSubscriptionRequest {
            subscribe: vec![EventType::TransactionsAdded],
        };

        let rx = client
            .subscribe(format!("{}", server_addr), subscription_request)
            .unwrap();

        // Give it time to connect
        thread::sleep(Duration::from_millis(100));

        // Should be connected now
        assert_eq!(client.get_status(), ConnectionStatus::Connected);

        // Shutdown the client
        client.shutdown_and_join();

        // Check the client is disconnected
        assert_eq!(client.get_status(), ConnectionStatus::Disconnected);

        // Wait for the server to finish
        server_handle.join().unwrap();
    }

    #[test]
    fn test_receive_events() {
        // Create a channel to sync with the test server
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();

        // Start a test server
        let server_handle = start_test_server(ready_tx);

        // Wait for the server to be ready and get its address
        let server_addr = ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        // Create a client with short timeout
        let config = TcpClientConfig {
            connection_timeout: Duration::from_secs(1),
            max_reconnect_attempts: Some(1),
            base_reconnect_interval: Duration::from_millis(100),
            ..TcpClientConfig::default()
        };
        let client = TcpClient::new(config);

        // Subscribe to receive events
        let subscription_request = TcpSubscriptionRequest {
            subscribe: vec![EventType::TransactionsAdded],
        };

        let rx = client
            .subscribe(format!("{}", server_addr), subscription_request)
            .unwrap();

        // Give it time to establish connection
        thread::sleep(Duration::from_millis(200));

        // Try to receive an event with timeout
        let event = rx.recv_timeout(Duration::from_secs(2));
        assert!(event.is_ok(), "Should have received an event");

        match event.unwrap() {
            Event::TransactionsAdded { txids } => {
                assert_eq!(txids.len(), 1);
                assert_eq!(
                    txids[0].to_string(),
                    "1111111111111111111111111111111111111111111111111111111111111111"
                );
            }
            other => panic!("Received unexpected event type: {:?}", other),
        }

        // Shutdown the client
        client.shutdown_and_join();

        // Wait for the server to finish
        server_handle.join().unwrap();
    }

    #[test]
    fn test_connection_error_handling() {
        // Create a client with short timeout
        let config = TcpClientConfig {
            connection_timeout: Duration::from_secs(1),
            max_reconnect_attempts: Some(2),
            base_reconnect_interval: Duration::from_millis(100),
            ..TcpClientConfig::default()
        };
        let client = TcpClient::new(config);

        // Initially disconnected
        assert_eq!(client.get_status(), ConnectionStatus::Disconnected);

        // Try to connect to a non-existent server
        let subscription_request = TcpSubscriptionRequest {
            subscribe: vec![EventType::TransactionsAdded],
        };

        let rx = client
            .subscribe("127.0.0.1:1".to_string(), subscription_request)
            .unwrap();

        // Give it time to attempt connection and reconnection
        thread::sleep(Duration::from_millis(500));

        // Should be in reconnecting state or disconnected if it already gave up
        let status = client.get_status();
        assert!(
            status == ConnectionStatus::Reconnecting || status == ConnectionStatus::Disconnected,
            "Expected Reconnecting or Disconnected state, got {:?}",
            status
        );

        // Shutdown the client
        client.shutdown_and_join();

        // Check the client is disconnected
        assert_eq!(client.get_status(), ConnectionStatus::Disconnected);
    }

    #[test]
    fn test_resource_cleanup() {
        // Create a client
        let client = TcpClient::new(TcpClientConfig::default());

        // Subscribe to a non-existent server
        let subscription_request = TcpSubscriptionRequest {
            subscribe: vec![EventType::TransactionsAdded],
        };

        let rx = client
            .subscribe("127.0.0.1:1".to_string(), subscription_request)
            .unwrap();

        // Verify we have an active thread
        assert!(client.has_active_thread());

        // Drop the receiver channel
        drop(rx);

        // Shutdown and join the client
        let joined = client.shutdown_and_join();
        assert!(joined, "Should have successfully joined the worker thread");

        // Verify we no longer have an active thread
        assert!(!client.has_active_thread());
    }
}
