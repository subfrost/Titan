use {
    crate::bitcoin_rpc::{BitcoinCoreRpcResultExt, RpcClientPool},
    bitcoin::{Block, BlockHash},
    bitcoincore_rpc::{Client, RpcApi},
    std::{
        collections::BTreeMap,
        sync::{
            atomic::{AtomicU64, Ordering},
            mpsc, Arc, Condvar, Mutex,
        },
        thread,
        time::Duration,
    },
    threadpool::ThreadPool,
    tracing::{error, trace, warn},
};

/// Simple counting semaphore for synchronizing buffer capacity between producer and consumer threads.
#[derive(Clone, Debug)]
struct Semaphore {
    inner: Arc<(Mutex<usize>, Condvar)>,
}

impl Semaphore {
    /// Create a new semaphore that allows `permits` concurrent acquisitions.
    fn new(permits: usize) -> Self {
        Self {
            inner: Arc::new((Mutex::new(permits), Condvar::new())),
        }
    }

    /// Block the current thread until a permit can be acquired.
    fn acquire(&self) {
        let (mutex, cvar) = &*self.inner;
        let mut remaining = mutex.lock().unwrap();
        while *remaining == 0 {
            remaining = cvar.wait(remaining).unwrap();
        }
        *remaining -= 1;
    }

    /// Release a previously-acquired permit, unblocking one waiting thread if any.
    fn release(&self) {
        let (mutex, cvar) = &*self.inner;
        let mut remaining = mutex.lock().unwrap();
        *remaining += 1;
        cvar.notify_one();
    }
}

#[derive(Debug, Default, Clone)]
pub struct BlockFetcherStats {
    /// Number of blocks currently being fetched (requested but not yet completed)
    pub blocks_being_fetched: Arc<AtomicU64>,
    /// Total number of blocks sent to the intermediate channel
    pub blocks_sent_to_intermediate: Arc<AtomicU64>,
    /// Total number of blocks sent to the final channel
    pub blocks_sent_to_final: Arc<AtomicU64>,
    /// Current number of blocks in the reordering buffer
    pub blocks_in_buffer: Arc<AtomicU64>,
}

impl BlockFetcherStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment the counter for blocks being fetched
    pub fn increment_fetching(&self) {
        self.blocks_being_fetched.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement the counter for blocks being fetched (when fetch completes)
    pub fn decrement_fetching(&self) {
        self.blocks_being_fetched.fetch_sub(1, Ordering::Relaxed);
    }

    /// Increment the counter for blocks sent to intermediate channel
    pub fn increment_intermediate(&self) {
        self.blocks_sent_to_intermediate
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_intermediate(&self) {
        self.blocks_sent_to_intermediate
            .fetch_sub(1, Ordering::Relaxed);
    }

    /// Increment the counter for blocks sent to final channel
    pub fn increment_final(&self) {
        self.blocks_sent_to_final.fetch_add(1, Ordering::Relaxed);
    }

    pub fn decrement_final(&self) {
        self.blocks_sent_to_final.fetch_sub(1, Ordering::Relaxed);
    }

    /// Increment the counter for blocks in buffer
    pub fn increment_buffer(&self) {
        self.blocks_in_buffer.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrement the counter for blocks in buffer
    pub fn decrement_buffer(&self) {
        self.blocks_in_buffer.fetch_sub(1, Ordering::Relaxed);
    }

    /// Get the current number of blocks being fetched
    pub fn get_blocks_being_fetched(&self) -> u64 {
        self.blocks_being_fetched.load(Ordering::Relaxed)
    }

    /// Get the total number of blocks sent to intermediate channel
    pub fn get_blocks_sent_to_intermediate(&self) -> u64 {
        self.blocks_sent_to_intermediate.load(Ordering::Relaxed)
    }

    /// Get the total number of blocks sent to final channel
    pub fn get_blocks_sent_to_final(&self) -> u64 {
        self.blocks_sent_to_final.load(Ordering::Relaxed)
    }

    /// Get the current number of blocks in buffer
    pub fn get_blocks_in_buffer(&self) -> u64 {
        self.blocks_in_buffer.load(Ordering::Relaxed)
    }

    /// Get a snapshot of all stats
    pub fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.get_blocks_being_fetched(),
            self.get_blocks_sent_to_intermediate(),
            self.get_blocks_sent_to_final(),
            self.get_blocks_in_buffer(),
        )
    }
}

pub enum BlockLocator {
    Height(u64),
    Hash(BlockHash),
}

pub fn fetch_blocks_from(
    bitcoin_rpc_pool: RpcClientPool,
    start_height: u64,
    limit: u64,
) -> Result<(mpsc::Receiver<Block>, BlockFetcherStats), bitcoincore_rpc::Error> {
    fetch_blocks_from_with_buffer_limit(bitcoin_rpc_pool, start_height, limit, 500)
}

pub fn fetch_blocks_from_with_buffer_limit(
    bitcoin_rpc_pool: RpcClientPool,
    start_height: u64,
    limit: u64,
    max_buffer_size: u64,
) -> Result<(mpsc::Receiver<Block>, BlockFetcherStats), bitcoincore_rpc::Error> {
    let stats = BlockFetcherStats::new();
    let buffer_semaphore = Semaphore::new(max_buffer_size as usize); // Limit buffer size

    // Final channel for ordered blocks.
    let (final_sender, final_rx) = mpsc::sync_channel(32);

    // Intermediate channel for unordered (height, block) tuples.
    let (intermediate_sender, intermediate_receiver) = mpsc::sync_channel(1000);

    // Spawn a logging thread to report stats every second
    thread::spawn({
        let stats = stats.clone();
        move || loop {
            thread::sleep(Duration::from_secs(1));
            let (fetching, intermediate, final_count, buffer) = stats.snapshot();
            tracing::info!(
                "Block fetcher stats - Fetching: {}, Intermediate: {}, Final: {}, Buffer: {}",
                fetching,
                intermediate,
                final_count,
                buffer
            );
        }
    });

    // Spawn a thread to reorder blocks.
    thread::spawn({
        let final_sender = final_sender.clone();
        let stats = stats.clone();
        let buffer_semaphore = buffer_semaphore.clone();
        move || {
            let mut next_expected = start_height;
            let mut buffer = BTreeMap::new();
            while let Ok((height, block)) = intermediate_receiver.recv() {
                stats.decrement_intermediate();

                if height == next_expected {
                    // Release the buffer permit that was acquired by the producer for this block.
                    buffer_semaphore.release();

                    if final_sender.send(block).is_err() {
                        trace!("Final receiver disconnected");
                        return;
                    }
                    stats.increment_final();
                    next_expected += 1;
                    while let Some(block) = buffer.remove(&next_expected) {
                        stats.decrement_buffer();
                        buffer_semaphore.release(); // Release buffer space held for this buffered block
                        if final_sender.send(block).is_err() {
                            warn!("Final receiver disconnected");
                            return;
                        }
                        stats.increment_final();
                        next_expected += 1;
                    }
                } else {
                    // Insert out-of-order block into the buffer. Capacity for this block was already
                    // reserved by the producer before the network fetch, so we can safely insert
                    // without acquiring here.
                    buffer.insert(height, block);
                    stats.increment_buffer();
                }
            }
        }
    });

    // Create a thread pool sized for I/O-bound tasks.
    let num_threads = 50;
    let pool = ThreadPool::new(num_threads);
    for height in start_height..limit {
        let bitcoin_rpc_pool = bitcoin_rpc_pool.clone();
        let intermediate_sender = intermediate_sender.clone();
        let stats = stats.clone();
        let buffer_semaphore = buffer_semaphore.clone();

        stats.increment_fetching();
        pool.execute(move || {
            // Reserve buffer capacity BEFORE starting the expensive RPC call. This guarantees that
            // once the block is fetched there will always be space for it in the in-memory buffer
            // or the downstream channels, preventing deadlocks when an earlier block is delayed.
            buffer_semaphore.acquire();

            let client = match bitcoin_rpc_pool.get() {
                Ok(c) => c,
                Err(e) => {
                    error!("Failed to create RPC client for height {}: {}", height, e);
                    stats.decrement_fetching();
                    buffer_semaphore.release(); // Release reserved buffer slot on error
                    return;
                }
            };

            match get_block_with_retries(&client, BlockLocator::Height(height)) {
                Ok(Some(block)) => {
                    if intermediate_sender.send((height, block)).is_err() {
                        trace!("Intermediate receiver disconnected");
                        // Intermediate receiver hung up – free the reserved buffer slot.
                        buffer_semaphore.release();
                    } else {
                        stats.increment_intermediate();
                    }
                }
                Ok(None) => {
                    error!("Block not found for height {}", height);
                    buffer_semaphore.release(); // Release reserved buffer slot on error
                }
                Err(err) => {
                    error!("Failed to fetch block {}: {}", height, err);
                    buffer_semaphore.release(); // Release reserved buffer slot on error
                }
            }

            stats.decrement_fetching();
        });
    }

    Ok((final_rx, stats))
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
                    thread::sleep(Duration::from_secs(120));
                } else {
                    thread::sleep(Duration::from_secs(seconds));
                }
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
