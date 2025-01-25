use {
    super::store::Store,
    std::{sync::Arc, time::Duration},
    tokio::{select, sync::watch, time::sleep},
    tracing::{error, info},
};

/// Periodically delete subscriptions that haven't succeeded for `expiry_secs`.
/// If a shutdown signal arrives, exit gracefully.
pub async fn cleanup_inactive_subscriptions(
    store: Arc<dyn Store>,
    interval: Duration,
    expiry_secs: u64,
    mut shutdown_rx: watch::Receiver<()>,
) {
    info!("cleanup_inactive_subscriptions started");

    loop {
        select! {
            _ = sleep(interval) => {
                if let Err(e) = do_cleanup(&store, expiry_secs).await {
                    error!("cleanup error: {:?}", e);
                }
            }
            _ = shutdown_rx.changed() => {
                info!("cleanup_inactive_subscriptions received shutdown signal, exiting.");
                break;
            }
        }
    }

    info!("cleanup_inactive_subscriptions ended");
}

/// Do an actual iteration, removing stale subscriptions
async fn do_cleanup(
    store: &Arc<dyn Store>,
    expiry_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1) Get current epoch secs
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // 2) Retrieve and filter
    let subscriptions = store.get_subscriptions()?;
    for sub in subscriptions {
        // If no last_success, or last_success is > 24 hours old, remove sub
        if now_secs - sub.last_success_epoch_secs > expiry_secs {
            // It's inactive, remove it
            let _ = store.delete_subscription(&sub.id);
        }
    }

    Ok(())
}
