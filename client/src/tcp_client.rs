use crate::models::Event;
use crate::tcp_subscription::TcpSubscriptionRequest;
use serde_json;
use std::error::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
    sync::mpsc,
};

/// Connect to the TCP subscription server at `addr` and subscribe
/// using the given request. Returns a receiver for incoming events.
pub async fn subscribe(
    addr: &str,
    subscription_request: TcpSubscriptionRequest,
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
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // Connection closed.
                    eprintln!("TCP connection closed by server.");
                    break;
                }
                Ok(_) => {
                    // Attempt to deserialize the JSON line into an Event.
                    match serde_json::from_str::<Event>(line.trim()) {
                        Ok(event) => {
                            if let Err(e) = tx.send(event).await {
                                eprintln!("Failed to send event to channel: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to parse event: {}. Line: {}", e, line.trim());
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error reading from TCP socket: {}", e);
                    break;
                }
            }
        }
    });

    Ok(rx)
}
