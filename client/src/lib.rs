mod error;
mod http;
mod tcp_client;

pub use error::*;

pub use http::{
    AsyncClient as TitanClient, SyncClient as TitanBlockingClient, TitanApiAsync as TitanApi,
    TitanApiSync as TitanApiBlocking,
};
pub use titan_types::*;

#[cfg(feature = "tcp_client")]
pub use tcp_client::subscribe as subscribe_to_titan;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use titan_types::{Event, TcpSubscriptionRequest};
    use tokio::{
        sync::watch,
        time::{sleep, Duration, Instant},
    };

    // Import the HTTP and TCP client functions.
    use crate::http::{AsyncClient as HttpClient, TitanApiAsync as TitanApi};
    #[cfg(feature = "tcp_client")]
    use crate::tcp_client::subscribe;

    /// End-to-end test for the TCP subscription client.
    ///
    /// This test:
    /// 1. Connects to the TCP subscription server at localhost:8080,
    ///    subscribing to "TransactionsAdded", "TransactionsReplaced", and "NewBlock".
    /// 2. Listens for events for 10 seconds, printing each received event.
    /// 3. Then signals shutdown.
    #[tokio::test]
    #[cfg(feature = "tcp_client")]
    async fn test_tcp_subscription_e2e() -> Result<(), Box<dyn Error>> {
        use tokio::time::timeout;

        let tcp_addr = "127.0.0.1:8080";
        let subscription_request = TcpSubscriptionRequest {
            subscribe: vec![
                EventType::TransactionsAdded,
                EventType::TransactionsReplaced,
                EventType::NewBlock,
            ],
        };

        // Create a watch channel to signal shutdown.
        let (shutdown_tx, shutdown_rx) = watch::channel(());

        // Connect to the TCP server and subscribe.
        let mut rx = subscribe(tcp_addr, subscription_request, shutdown_rx).await?;

        println!("Connected to TCP subscription server at {}.", tcp_addr);

        // Listen for events for 10 seconds.
        let listen_duration = Duration::from_secs(10);
        let start = Instant::now();
        let mut events = Vec::new();
        while Instant::now().duration_since(start) < listen_duration {
            match timeout(Duration::from_millis(500), rx.recv()).await {
                Ok(Some(event)) => {
                    // We got an event
                    println!("Received TCP event: {:?}", event);
                    events.push(event);
                }
                Ok(None) => {
                    // The sender side or the connection closed
                    println!("TCP subscription channel closed. Stopping early.");
                    break;
                }
                Err(_) => {
                    // Timed out waiting for an event
                    // This means no event arrived in the last 500 ms, but we can keep waiting until 10s is up
                }
            }
        }
        println!("Total events received in 10 seconds: {}", events.len());

        // Signal shutdown to the subscription task.
        let _ = shutdown_tx.send(());
        println!("Shutdown signal sent to TCP subscription task.");

        Ok(())
    }

    /// End-to-end test for the HTTP client.
    ///
    /// This test:
    /// 1. Connects to the HTTP server at http://localhost:3030.
    /// 2. Retrieves and prints the block status and tip.
    #[tokio::test]
    async fn test_http_status_tip_e2e() -> Result<(), Box<dyn Error>> {
        let base_url = "http://localhost:3030";
        let client = TitanClient::new(base_url);

        println!("Fetching HTTP status from {}...", base_url);
        match client.get_status().await {
            Ok(status) => {
                println!("HTTP Status: {:?}", status);
            }
            Err(e) => {
                eprintln!("Failed to get HTTP status: {}", e);
            }
        }

        println!("Fetching block tip from {}...", base_url);
        match client.get_tip().await {
            Ok(tip) => {
                println!("Block Tip: {:?}", tip);
            }
            Err(e) => {
                eprintln!("Failed to get block tip: {}", e);
            }
        }

        Ok(())
    }
}
