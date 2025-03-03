use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

use serde_json;
use thiserror::Error;
#[cfg(feature = "tcp_client")]
use titan_types::{Event, TcpSubscriptionRequest};
use tracing::{error, info, warn};

#[derive(Debug, Error)]
pub enum TcpClientError {
    #[error("io error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("serde error: {0}")]
    SerdeError(#[from] serde_json::Error),
}

/// Synchronous TCP subscription listener.
///
/// Connects to the TCP server at `addr` and sends the given subscription request
/// (encoded as JSON). It then spawns a dedicated thread that reads lines from the TCP
/// connection using non-blocking mode. If no data is available, it sleeps briefly and
/// then checks the shutdown flag again.
///
/// The listener will continue until either the TCP connection is closed or the provided
/// `shutdown` method is called.
///
/// # Arguments
///
/// * `addr` - The address of the TCP subscription server (e.g., "127.0.0.1:9000").
/// * `subscription_request` - The subscription request to send to the server.
///
/// # Returns
///
/// A `Result` containing a `std::sync::mpsc::Receiver<Event>` that will receive events from the server,
/// or an error.
#[cfg(feature = "tcp_client_blocking")]
pub struct TcpClient {
    shutdown_flag: Arc<AtomicBool>,
}

#[cfg(feature = "tcp_client_blocking")]
impl TcpClient {
    pub fn new() -> Self {
        Self {
            shutdown_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn subscribe(
        &self,
        addr: &str,
        subscription_request: TcpSubscriptionRequest,
    ) -> Result<mpsc::Receiver<Event>, TcpClientError> {
        let shutdown_flag = self.shutdown_flag.clone();
        subscribe(addr, subscription_request, shutdown_flag)
    }

    pub fn shutdown(&self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
    }
}


fn subscribe(
    addr: &str,
    subscription_request: TcpSubscriptionRequest,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<mpsc::Receiver<Event>, TcpClientError> {
    // Connect to the TCP server.
    let mut stream = TcpStream::connect(addr)?;
    
    // Set a read timeout instead of non-blocking mode
    stream.set_read_timeout(Some(Duration::from_millis(500)))?;

    // Clone the stream for reading.
    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);

    // Serialize the subscription request to JSON and send it.
    let req_json = serde_json::to_string(&subscription_request)?;
    stream.write_all(req_json.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    // Create a standard mpsc channel to forward events.
    let (tx, rx) = mpsc::channel::<Event>();

    // Spawn a thread to read events from the TCP connection.
    thread::spawn(move || {
        let mut line = String::new();
        let mut read_in_progress = false;
        
        loop {
            // Check if shutdown has been signaled.
            if shutdown_flag.load(Ordering::SeqCst) {
                info!("Shutdown flag set. Exiting subscription thread.");
                break;
            }

            // Only clear the line if we're not in the middle of a partial read
            if !read_in_progress {
                line.clear();
            }
            
            match reader.read_line(&mut line) {
                Ok(0) => {
                    // Connection closed.
                    warn!("TCP connection closed by server.");
                    break;
                }
                Ok(n) => {
                    // Successfully read a complete line
                    read_in_progress = false;
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    
                    // Deserialize the JSON line into an Event.
                    match serde_json::from_str::<Event>(trimmed) {
                        Ok(event) => {
                            if tx.send(event).is_err() {
                                error!("Receiver dropped. Exiting subscription thread.");
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Failed to parse event: {}. Line: {}", e, trimmed);
                        }
                    }
                }
                Err(e) => {
                    // Handle timeout errors differently than other errors
                    if e.kind() == std::io::ErrorKind::TimedOut || e.kind() == std::io::ErrorKind::WouldBlock {
                        // This means we're in the middle of reading a line
                        if !line.is_empty() {
                            read_in_progress = true;
                            info!("Partial read in progress ({} bytes so far), continuing...", line.len());
                        }
                        continue;
                    } else {
                        error!("Error reading from TCP socket: {}", e);
                        read_in_progress = false;
                        // For unexpected errors, add a small delay
                        thread::sleep(Duration::from_millis(100));
                    }
                }
            }
        }
        info!("Exiting TCP subscription thread.");
    });

    Ok(rx)
}
