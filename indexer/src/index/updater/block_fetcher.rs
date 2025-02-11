use {
    crate::index::{bitcoin_rpc::BitcoinCoreRpcResultExt, RpcClientProvider},
    bitcoin::{Block, BlockHash},
    bitcoincore_rpc::{Client, RpcApi},
    std::{
        collections::BTreeMap,
        sync::{mpsc, Arc},
        thread,
        time::Duration,
    },
    threadpool::ThreadPool,
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
    // Final channel for ordered blocks.
    let (final_sender, final_rx) = mpsc::sync_channel(32);
    // Intermediate channel for unordered (height, block) tuples.
    let (intermediate_sender, intermediate_receiver) = mpsc::channel();

    // Spawn a thread to reorder blocks.
    thread::spawn({
        let final_sender = final_sender.clone();
        move || {
            let mut next_expected = start_height;
            let mut buffer = BTreeMap::new();
            while let Ok((height, block)) = intermediate_receiver.recv() {
                if height == next_expected {
                    if final_sender.send(block).is_err() {
                        warn!("Final receiver disconnected");
                        return;
                    }
                    next_expected += 1;
                    while let Some(block) = buffer.remove(&next_expected) {
                        if final_sender.send(block).is_err() {
                            warn!("Final receiver disconnected");
                            return;
                        }
                        next_expected += 1;
                    }
                } else {
                    buffer.insert(height, block);
                }
            }
        }
    });

    // Create a thread pool sized for I/O-bound tasks.
    let num_threads = 50;
    let pool = ThreadPool::new(num_threads);
    let window_size = 300;
    let mut current_start = start_height;
    while current_start < limit {
        let current_end = std::cmp::min(current_start + window_size, limit);
        for height in current_start..current_end {
            let client_provider = client_provider.clone();
            let intermediate_sender = intermediate_sender.clone();
            pool.execute(move || {
                let client = match client_provider.get_new_rpc_client() {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Failed to create RPC client for height {}: {}", height, e);
                        return;
                    }
                };

                match get_block_with_retries(&client, BlockLocator::Height(height)) {
                    Ok(Some(block)) => {
                        if intermediate_sender.send((height, block)).is_err() {
                            warn!("Intermediate receiver disconnected");
                        }
                    }
                    Ok(None) => error!("Block not found for height {}", height),
                    Err(err) => error!("Failed to fetch block {}: {}", height, err),
                }
            });
        }
        current_start = current_end;
    }

    // Signal that no more blocks will be sent.
    drop(intermediate_sender);
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
                    BlockLocator::Hash(ref h) => h.to_string(),
                };
                warn!(
                    "Failed to fetch block {}: retrying in {}s: {}",
                    loc_str, seconds, err
                );
                if seconds > 120 {
                    error!("Would sleep for more than 120s, giving up");
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
