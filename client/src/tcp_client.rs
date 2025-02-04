use types::{Event, TcpSubscriptionRequest};
use serde_json;
use std::error::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::{mpsc, watch},
};
use tracing::{error, info, warn};

/// Connect to the TCP subscription server at `addr` and subscribe
/// using the given request. Returns a receiver for incoming events.
/// The provided shutdown_rx receiver is used to signal a graceful shutdown.
pub async fn subscribe(
    addr: &str,
    subscription_request: TcpSubscriptionRequest,
    mut shutdown_rx: watch::Receiver<()>,
) -> Result<mpsc::Receiver<Event>, Box<dyn Error>> {
    // Connect to the TCP server.
    let stream = TcpStream::connect(addr).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    // Serialize the subscription request to JSON and send it.
    let req_json = serde_json::to_string(&subscription_request)?;
    writer.write_all(req_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    // Create a channel to forward events.
    let (tx, rx) = mpsc::channel::<Event>(100);

    // Spawn a task to read events from the TCP connection.
    tokio::spawn(async move {
        let mut line = String::new();
        loop {
            line.clear();
            tokio::select! {
                result = reader.read_line(&mut line) => {
                    match result {
                        Ok(0) => {
                            // Connection closed.
                            warn!("TCP connection closed by server.");
                            break;
                        }
                        Ok(_) => {
                            // Attempt to deserialize the JSON line into an Event.
                            match serde_json::from_str::<Event>(line.trim()) {
                                Ok(event) => {
                                    if let Err(e) = tx.send(event).await {
                                        error!("Failed to send event to channel: {}", e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to parse event: {}. Line: {}", e, line.trim());
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error reading from TCP socket: {}", e);
                            break;
                        }
                    }
                }
                // If a shutdown signal is received, exit the loop.
                _ = shutdown_rx.changed() => {
                    info!("Shutdown signal received. Exiting TCP subscription task.");
                    break;
                }
            }
        }
    });

    Ok(rx)
}
