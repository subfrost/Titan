use serde_json;
use thiserror::Error;
use titan_types::{Event, TcpSubscriptionRequest};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::{mpsc, watch},
};
use tracing::{error, info, warn};

#[derive(Debug, Error)]
pub enum TcpClientError {
    #[error("io error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("serde error: {0}")]
    SerdeError(#[from] serde_json::Error),
}

/// Asynchronous TCP client that encapsulates the shutdown signal.
pub struct AsyncTcpClient {
    shutdown_tx: watch::Sender<()>,
    shutdown_rx: watch::Receiver<()>,
}

impl AsyncTcpClient {
    /// Creates a new instance of the async TCP client.
    pub fn new() -> Self {
        // Create a watch channel with an initial value.
        let (shutdown_tx, shutdown_rx) = watch::channel(());
        Self {
            shutdown_tx,
            shutdown_rx,
        }
    }

    /// Subscribes to the TCP server at `addr` with the given subscription request.
    /// Returns a channel receiver for incoming events.
    pub async fn subscribe(
        &self,
        addr: &str,
        subscription_request: TcpSubscriptionRequest,
    ) -> Result<mpsc::Receiver<Event>, TcpClientError> {
        // Connect to the TCP server.
        let stream = TcpStream::connect(addr).await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Serialize the subscription request and send it.
        let req_json = serde_json::to_string(&subscription_request)?;
        writer.write_all(req_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        // Create a channel to forward events.
        let (tx, rx) = mpsc::channel::<Event>(100);

        // Clone the shutdown receiver for the spawned task.
        let mut shutdown_rx = self.shutdown_rx.clone();

        // Spawn a task to continuously read from the TCP stream.
        tokio::spawn(async move {
            // Keep writer in scope (if needed later).
            let _writer_guard = writer;
            let mut line = String::new();
            let mut read_in_progress = false;
            
            loop {
                // Only clear line if we're not in the middle of a partial read
                if !read_in_progress {
                    line.clear();
                }
                
                tokio::select! {
                    // Read a line from the TCP connection.
                    result = reader.read_line(&mut line) => {
                        match result {
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
                                
                                info!("Received complete line with {} bytes", n);
                                
                                match serde_json::from_str::<Event>(trimmed) {
                                    Ok(event) => {
                                        if let Err(e) = tx.send(event).await {
                                            error!("Failed to send event to channel: {}", e);
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        error!("Failed to parse event: {}. Line: {}", e, trimmed);
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error reading from TCP socket: {}", e);
                                // If this is a would-block or similar temporary error and we have partial data,
                                // mark as in progress to preserve the buffer
                                if !line.is_empty() {
                                    read_in_progress = true;
                                    info!("Partial read in progress ({} bytes so far), will continue", line.len());
                                } else {
                                    read_in_progress = false;
                                }
                                // Add a small delay before retrying on error
                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            }
                        }
                    }
                    // Check for shutdown signal.
                    _ = shutdown_rx.changed() => {
                        info!("Shutdown signal received. Exiting TCP subscription task.");
                        break;
                    }
                }
            }
            info!("Exiting TCP subscription task.");
        });

        Ok(rx)
    }

    /// Signals the client to shut down by sending a signal through the watch channel.
    pub fn shutdown(&self) {
        if let Err(e) = self.shutdown_tx.send(()) {
            error!("Failed to send shutdown signal: {:?}", e);
        }
    }
}
