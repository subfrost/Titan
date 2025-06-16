use {
    crate::bitcoin_rpc::{BitcoinCoreRpcResultExt, RpcClientPool},
    bitcoin::{Block, BlockHash},
    bitcoincore_rpc::{Client, RpcApi},
    crossbeam_channel,
    std::sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    std::{collections::BTreeMap, sync::mpsc, thread, time::Duration},
    threadpool::ThreadPool,
    tracing::{error, info, trace, warn},
};

pub enum BlockLocator {
    Height(u64),
    Hash(BlockHash),
}

/// Configuration knobs for block fetching. Implementors can build this
/// manually or rely on `Default`, which provides sensible defaults:
///   • num_threads  – worker-thread pool size (default 50)
///   • window_size  – look-ahead window of heights (default 300)
///   • batch_size   – consecutive heights fetched by a single task (default 20)
#[derive(Clone, Copy, Debug)]
pub struct FetchConfig {
    pub num_threads: usize,
    pub window_size: u64,
    pub batch_size: u64,
}

impl Default for FetchConfig {
    fn default() -> Self {
        Self {
            num_threads: 50,
            window_size: 300,
            batch_size: 20,
        }
    }
}

pub fn fetch_blocks_from(
    bitcoin_rpc_pool: RpcClientPool,
    start_height: u64,
    limit: u64,
    config: FetchConfig,
) -> Result<mpsc::Receiver<Block>, bitcoincore_rpc::Error> {
    // Final channel for ordered blocks.
    let (final_sender, final_rx) = mpsc::sync_channel(32);

    const CHANNEL_CAPACITY: usize = 1_000;

    // Intermediate channel for unordered (height, block) tuples. Using a bounded
    // crossbeam channel keeps the send side lock-free but still provides back-pressure.
    let (intermediate_sender, intermediate_receiver) =
        crossbeam_channel::bounded::<(u64, Block)>(CHANNEL_CAPACITY);

    // Clone receiver for the monitoring thread so we can query its length and closed state
    let intermediate_monitor_receiver = intermediate_receiver.clone();

    // Counter for successfully fetched blocks.
    let fetched_counter = Arc::new(AtomicU64::new(0));
    let total_blocks = limit.saturating_sub(start_height);

    // Spawn a thread to reorder blocks.
    thread::spawn({
        let final_sender = final_sender.clone();
        let fetched_counter = fetched_counter.clone();
        move || {
            let mut next_expected = start_height;
            let mut buffer = BTreeMap::new();
            while let Ok((height, block)) = intermediate_receiver.recv() {
                if height == next_expected {
                    if final_sender.send(block).is_err() {
                        trace!("Final receiver disconnected");
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

    // Monitoring thread – logs stats every second until the intermediate sender is closed and empty.
    {
        let fetched_counter = fetched_counter.clone();
        let total_blocks = total_blocks;
        thread::spawn(move || {
            loop {
                thread::sleep(Duration::from_secs(1));
                let queue_len = intermediate_monitor_receiver.len();
                let fetched = fetched_counter.load(Ordering::Relaxed);
                info!(
                    "block_fetcher: fetched={} queue_len={}/{}",
                    fetched, queue_len, CHANNEL_CAPACITY
                );

                // Exit when all blocks fetched and queue drained
                if fetched >= total_blocks as u64 && queue_len == 0 {
                    break;
                }
            }
        });
    }

    let num_threads = config.num_threads;
    let window_size = config.window_size;
    let batch_size = config.batch_size;

    let pool = ThreadPool::new(num_threads);
    let mut current_start = start_height;
    while current_start < limit {
        let current_end = std::cmp::min(current_start + window_size, limit);

        let mut chunk_start = current_start;
        while chunk_start < current_end {
            let chunk_end = std::cmp::min(chunk_start + batch_size, current_end);
            let heights: Vec<u64> = (chunk_start..chunk_end).collect();

            let bitcoin_rpc_pool = bitcoin_rpc_pool.clone();
            let intermediate_sender = intermediate_sender.clone();
            let fetched_counter = fetched_counter.clone();

            pool.execute(move || {
                // Acquire one RPC client per task and reuse it for the whole chunk.
                let client = match bitcoin_rpc_pool.get() {
                    Ok(c) => c,
                    Err(e) => {
                        error!("Failed to create RPC client: {}", e);
                        return;
                    }
                };

                for height in heights {
                    match get_block_with_retries(&client, BlockLocator::Height(height)) {
                        Ok(Some(block)) => {
                            if intermediate_sender.send((height, block)).is_err() {
                                trace!("Intermediate receiver disconnected");
                                return;
                            }
                            fetched_counter.fetch_add(1, Ordering::Relaxed);
                        }
                        Ok(None) => error!("Block not found for height {}", height),
                        Err(err) => error!("Failed to fetch block {}: {}", height, err),
                    }
                }
            });

            chunk_start = chunk_end;
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
    // Retry indefinitely with exponential back-off capped at 120 seconds.
    let mut attempts: u32 = 0;
    loop {
        match get_block_attempt(client, &locator) {
            // Success – return the block.
            Ok(Some(block)) => return Ok(Some(block)),

            // Block not yet available – wait and retry.
            Ok(None) => {
                attempts = attempts.saturating_add(1);
                let delay = (1u64 << attempts).min(120);
                let loc_str = match locator {
                    BlockLocator::Height(h) => h.to_string(),
                    BlockLocator::Hash(ref h) => h.to_string(),
                };
                warn!(
                    "Block {} not yet available: retrying in {}s (attempt #{})",
                    loc_str, delay, attempts
                );
                thread::sleep(Duration::from_secs(delay));
            }

            // Connectivity or RPC error – apply the same back-off policy.
            Err(err) => {
                attempts = attempts.saturating_add(1);
                let delay = (1u64 << attempts).min(120);
                let loc_str = match locator {
                    BlockLocator::Height(h) => h.to_string(),
                    BlockLocator::Hash(ref h) => h.to_string(),
                };
                warn!(
                    "Failed to fetch block {}: retrying in {}s (attempt #{}): {}",
                    loc_str, delay, attempts, err
                );
                thread::sleep(Duration::from_secs(delay));
            }
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
