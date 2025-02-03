use {
    crate::index::{bitcoin_rpc::BitcoinCoreRpcResultExt, RpcClientProvider},
    bitcoin::{Block, BlockHash},
    bitcoincore_rpc::{Client, RpcApi},
    rayon::{iter::IntoParallelIterator, prelude::*},
    std::{
        collections::BTreeMap,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        },
        thread,
        time::Duration,
    },
    tracing::{error, warn},
};

pub enum BlockLocator {
    Height(u64),
    Hash(BlockHash),
}

pub fn fetch_blocks_from(
    client_provider: Arc<dyn RpcClientProvider + Send + Sync>,
    start_height: u64,
    limit: u64,
) -> Result<mpsc::Receiver<Block>, bitcoincore_rpc::Error> {
    // Create the final channel for ordered blocks
    let (final_sender, final_rx) = mpsc::sync_channel(32);
    // Intermediate channel for unordered (height, block) tuples
    let (intermediate_sender, intermediate_receiver) = mpsc::channel();

    // Spawn a thread to reorder blocks and send them sequentially
    thread::spawn({
        let final_sender = final_sender.clone();
        move || {
            let mut next_expected = start_height;
            let mut buffer = BTreeMap::new();

            // Process incoming blocks from the intermediate channel
            while let Ok((height, block)) = intermediate_receiver.recv() {
                if height == next_expected {
                    // Send the next expected block immediately
                    if let Err(err) = final_sender.send(block) {
                        warn!("Final receiver disconnected: {err}");
                        return;
                    }
                    next_expected += 1;
                    // Check the buffer for subsequent blocks in order
                    while let Some(block) = buffer.remove(&next_expected) {
                        if let Err(err) = final_sender.send(block) {
                            warn!("Final receiver disconnected: {err}");
                            return;
                        }
                        next_expected += 1;
                    }
                } else {
                    // Buffer out-of-order blocks
                    buffer.insert(height, block);
                }
            }
        }
    });

    // Spawn a separate thread for the Rayon-based fetching to avoid blocking the main process.
    {
        let client_provider = client_provider.clone();
        let intermediate_sender = intermediate_sender.clone();
        thread::spawn(move || {
            let window_size = 300;
            let mut current_start = start_height;

            while current_start < limit {
                let current_end = std::cmp::min(current_start + window_size, limit);
                let range: Vec<u64> = (current_start..current_end).collect();
                let receiver_disconnected = Arc::new(AtomicBool::new(false));

                // Process the current window in parallel.
                range.into_par_iter().for_each(|height| {
                    let client = match client_provider.get_new_rpc_client() {
                        Ok(c) => c,
                        Err(e) => {
                            error!("Failed to create RPC client for height {height}: {e}");
                            return;
                        }
                    };

                    match get_block_with_retries(&client, BlockLocator::Height(height)) {
                        Ok(Some(block)) => {
                            if let Err(_) = intermediate_sender.send((height, block)) {
                                receiver_disconnected.store(true, Ordering::SeqCst);
                            }
                        }
                        Ok(None) => error!("Block not found for height {height}"),
                        Err(err) => error!("Failed to fetch block {height}: {err}"),
                    }
                });

                if receiver_disconnected.load(Ordering::SeqCst) {
                    break;
                }

                // Move to the next window.
                current_start = current_end;
            }

            // Signal that no more blocks will be sent by dropping the sender.
            drop(intermediate_sender);
        });
    }

    // Return the final receiver immediately to avoid blocking the main thread.
    Ok(final_rx)
}

pub fn get_block_with_retries(
    client: &Client,
    locator: BlockLocator,
) -> Result<Option<Block>, bitcoincore_rpc::Error> {
    let mut errors = 0;
    loop {
        match get_block_attempt(client, &locator) {
            Err(err) => {
                errors += 1;
                let seconds = 1 << errors;
                let loc_str = match locator {
                    BlockLocator::Height(h) => h.to_string(),
                    BlockLocator::Hash(h) => h.to_string(),
                };
                warn!("failed to fetch block {loc_str}, retrying in {seconds}s: {err}");

                if seconds > 120 {
                    error!("would sleep for more than 120s, giving up");
                    return Err(err.into());
                }

                thread::sleep(Duration::from_secs(seconds));
            }
            Ok(result) => return Ok(result),
        }
    }
}

fn get_block_attempt(
    client: &Client,
    locator: &BlockLocator,
) -> Result<Option<Block>, bitcoincore_rpc::Error> {
    match locator {
        BlockLocator::Height(height) => {
            let hash = client.get_block_hash((*height).into()).into_option()?;
            hash.map(|hash| Ok(client.get_block(&hash)?)).transpose()
        }
        BlockLocator::Hash(hash) => Ok(client.get_block(hash).into_option()?),
    }
}
