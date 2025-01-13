use {
    super::*,
    crate::{
        index::{
            metrics::Metrics, store::Store, RpcClientError, RpcClientProvider, Settings, StoreError,
        },
        models::{Block, RuneEntry},
    },
    bitcoin::{
        constants::SUBSIDY_HALVING_INTERVAL, hashes::Hash, hex::HexToArrayError,
        Block as BitcoinBlock, Transaction, Txid,
    },
    bitcoincore_rpc::{Client, RpcApi},
    block_fetcher::fetch_blocks_from,
    cache::{UpdaterCache, UpdaterCacheSettings},
    indicatif::{ProgressBar, ProgressStyle},
    ordinals::{Rune, RuneId, SpacedRune, Terms},
    prometheus::HistogramVec,
    rayon::{iter::IntoParallelRefIterator, prelude::*},
    rollback::{Rollback, RollbackError},
    rune_parser::RuneParser,
    std::{
        collections::HashSet,
        fmt::{self, Display, Formatter},
        str::FromStr,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::{SystemTime, UNIX_EPOCH},
    },
    store_lock::StoreWithLock,
    thiserror::Error,
    tracing::{debug, error, info, warn},
    transaction_updater::TransactionUpdater,
};

#[derive(Debug, Error)]
pub enum ReorgError {
    Recoverable { height: u64, depth: u64 },
    Unrecoverable,
    StoreError(#[from] StoreError),
    RPCError(#[from] bitcoincore_rpc::Error),
}

impl Display for ReorgError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Recoverable { height, depth } => {
                write!(f, "{depth} block deep reorg detected at height {height}")
            }
            Self::Unrecoverable => write!(f, "unrecoverable reorg detected"),
            Self::StoreError(e) => write!(f, "store error: {e}"),
            Self::RPCError(e) => write!(f, "RPC error: {e}"),
        }
    }
}

#[derive(Error, Debug)]
pub enum UpdaterError {
    #[error("db error {0}")]
    DB(#[from] StoreError),
    #[error("bitcoin rpc error {0}")]
    BitcoinRpc(#[from] bitcoincore_rpc::Error),
    #[error("bitcoin reorg error {0}")]
    BitcoinReorg(#[from] ReorgError),
    #[error("bitcoin rpc client error {0}")]
    BitcoinRpcClient(#[from] RpcClientError),
    #[error("transaction updater error {0}")]
    TransactionUpdater(#[from] TransactionUpdaterError),
    #[error("rollback error {0}")]
    Rollback(#[from] RollbackError),
    #[error("rune parser error {0}")]
    RuneParser(#[from] RuneParserError),
    #[error("txid error {0}")]
    Txid(#[from] HexToArrayError),
}

impl UpdaterError {
    pub fn is_halted(&self) -> bool {
        matches!(self, UpdaterError::BitcoinReorg(ReorgError::Unrecoverable))
    }
}

type Result<T> = std::result::Result<T, UpdaterError>;

pub struct Updater {
    db: Arc<StoreWithLock>,
    settings: Settings,
    is_at_tip: AtomicBool,
    unrecoverable_reorg: AtomicBool,
    shutdown_flag: Arc<AtomicBool>,

    // monitoring
    latency: HistogramVec,
}

impl Updater {
    pub fn new(
        db: Arc<dyn Store + Send + Sync>,
        settings: Settings,
        metrics: &Metrics,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            db: Arc::new(StoreWithLock::new(db)),
            settings,
            is_at_tip: AtomicBool::new(false),
            unrecoverable_reorg: AtomicBool::new(false),
            shutdown_flag,
            latency: metrics.histogram_vec(
                prometheus::HistogramOpts::new("indexer_latency", "Indexer latency"),
                &["method"],
            ),
        }
    }

    pub fn is_halted(&self) -> bool {
        self.unrecoverable_reorg.load(Ordering::Relaxed)
    }

    pub fn is_at_tip(&self) -> bool {
        self.is_at_tip.load(Ordering::Relaxed)
    }

    pub fn update_to_tip(&self) -> Result<()> {
        info!("Updating to tip");

        // Every 1000 blocks, commit the changes to the database
        let commit_interval = 1000;
        let mut cache = UpdaterCache::new(
            self.db.clone(),
            UpdaterCacheSettings::new(&self.settings, false),
        )?;

        // Get RPC client and get block height
        let bitcoin_block_client = self.settings.get_new_rpc_client()?;
        let bitcoin_block_count = bitcoin_block_client.get_block_count()?;

        // Fetch new blocks if needed.
        if bitcoin_block_count > cache.get_block_count() {
            let progress_bar =
                self.open_progress_bar(cache.get_block_count() - 1, bitcoin_block_count - 1);
            let min_height = self.settings.chain.first_rune_height() as u64;

            let rx = fetch_blocks_from(
                Arc::new(self.settings.clone()),
                cache.get_block_count(),
                bitcoin_block_count - 1,
            )?;

            let rpc_client = self.settings.get_new_rpc_client()?;

            while let Ok(block) = rx.recv() {
                if self.shutdown_flag.load(Ordering::SeqCst) {
                    info!("Updater received shutdown signal, stopping...");
                    break;
                }

                let block = if cache.get_block_count() < min_height {
                    Block::empty_block(cache.get_block_count() as u32, block.header)
                } else {
                    self.index_block(
                        block,
                        cache.get_block_count() as u64,
                        &rpc_client,
                        &mut cache,
                    )?
                };

                cache.set_new_block(block);

                if cache.should_flush(commit_interval) {
                    cache.flush()?;
                }

                progress_bar.inc(1);

                if progress_bar.position() > progress_bar.length().unwrap() {
                    if let Ok(count) = bitcoin_block_client.get_block_count() {
                        progress_bar.set_length(count + 1);
                    } else {
                        warn!("Failed to fetch latest block height");
                    }
                }
            }

            // Flush the cache to the database
            cache.flush()?;

            debug!("Finished fetching blocks");
            progress_bar.finish_and_clear();
        }

        if !self.shutdown_flag.load(Ordering::SeqCst) {
            self.is_at_tip.store(true, Ordering::Release);
        }

        Ok(())
    }

    pub fn index_mempool(&self) -> Result<()> {
        let _timer = self
            .latency
            .with_label_values(&["index_mempool"])
            .start_timer();

        let client = self.settings.get_new_rpc_client()?;

        // Get current mempool transactions
        let current_mempool: HashSet<Txid> = client.get_raw_mempool()?.into_iter().collect();

        // Get our previously indexed mempool transactions
        let stored_mempool = {
            let db = self.db.read();
            db.get_mempool_txids()?
        };

        // Find new transactions to index
        let new_txs: Vec<Txid> = current_mempool
            .difference(&stored_mempool)
            .cloned()
            .collect();

        // Find transactions to remove (they're no longer in mempool)
        let removed_txs: Vec<Txid> = stored_mempool
            .difference(&current_mempool)
            .cloned()
            .collect();

        let mut cache = UpdaterCache::new(
            self.db.clone(),
            UpdaterCacheSettings::new(&self.settings, true),
        )?;

        // Index new transactions
        let new_txs_len = new_txs.len();
        for txid in new_txs {
            let tx = client.get_raw_transaction(&txid, None)?;
            self.index_tx(&txid, &tx, &mut cache)?;
        }

        cache.flush()?;

        let removed_len = removed_txs.len();
        self.remove_txs(removed_txs, true)?;

        debug!(
            "Mempool: New txs: {}. Removed txs: {}",
            new_txs_len, removed_len
        );

        Ok(())
    }

    pub fn index_new_block(&self, bitcoin_block: BitcoinBlock, height: Option<u64>) -> Result<()> {
        let height = height.unwrap_or(self.settings.get_new_rpc_client()?.get_block_count()?);

        let rpc_client = self.settings.get_new_rpc_client()?;

        match self.detect_reorg(
            &bitcoin_block,
            height,
            &rpc_client,
            self.settings.max_recoverable_reorg_depth(),
        ) {
            Ok(()) => (),
            Err(ReorgError::Recoverable { height, depth }) => {
                self.handle_reorg(&bitcoin_block, height, depth)?;
                return Err(ReorgError::Recoverable { height, depth }.into());
            }
            Err(ReorgError::Unrecoverable) => {
                error!("Unrecoverable reorg detected");

                self.unrecoverable_reorg.store(true, Ordering::Release);
                return Err(ReorgError::Unrecoverable.into());
            }
            Err(e) => {
                return Err(e.into());
            }
        }

        let rpc_client = self.settings.get_new_rpc_client()?;
        let mut cache = UpdaterCache::new(
            self.db.clone(),
            UpdaterCacheSettings::new(&self.settings, false),
        )?;

        let block = self.index_block(bitcoin_block.clone(), height, &rpc_client, &mut cache)?;
        cache.set_new_block(block);
        cache.flush()?;

        info!("New tip {}:{}", height, bitcoin_block.header.block_hash());

        Ok(())
    }

    fn index_block(
        &self,
        bitcoin_block: BitcoinBlock,
        height: u64,
        rpc_client: &Client,
        cache: &mut UpdaterCache,
    ) -> Result<Block> {
        let _timer = self
            .latency
            .with_label_values(&["index_block"])
            .start_timer();

        let rune_parser = RuneParser::new(
            &rpc_client,
            self.settings.chain,
            height as u32,
            false,
            cache,
        )?;

        let block_data = bitcoin_block
            .txdata
            .par_iter()
            .enumerate()
            .filter_map(|(i, tx)| {
                let txid = tx.compute_txid();
                match rune_parser.index_runes(u32::try_from(i).unwrap(), tx, txid) {
                    Ok(result) => {
                        if result.has_rune_updates() {
                            Some((i, txid, result))
                        } else {
                            None
                        }
                    }
                    Err(e) => {
                        error!("Failed to index transaction {}: {}", txid, e);
                        None
                    }
                }
            })
            .collect::<Vec<_>>();

        let mut transaction_updater = TransactionUpdater::new(cache, None, false)?;

        let block_header: bitcoin::block::Header = bitcoin_block.header.clone();
        let block_height = height as u32;

        let mut block = Block::empty_block(height as u32, bitcoin_block.header);

        for (i, txid, result) in block_data {
            transaction_updater.save(
                block_header.time,
                block_height,
                txid,
                &bitcoin_block.txdata[i],
                &result,
            )?;
            block.tx_ids.push(txid.to_string());
            if let Some((id, ..)) = result.etched {
                block.etched_runes.push(id);
            }
        }

        Ok(block)
    }

    pub fn index_new_tx(&self, txid: &Txid, tx: &Transaction) -> Result<()> {
        let mut cache = UpdaterCache::new(
            self.db.clone(),
            UpdaterCacheSettings::new(&self.settings, true),
        )?;
        self.index_tx(txid, tx, &mut cache)?;
        cache.flush()?;
        Ok(())
    }

    fn index_tx(&self, txid: &Txid, tx: &Transaction, cache: &mut UpdaterCache) -> Result<()> {
        let _timer = self.latency.with_label_values(&["index_tx"]).start_timer();

        let height = cache.get_block_count();

        // now
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let rpc_client = self.settings.get_new_rpc_client()?;
        let rune_parser =
            RuneParser::new(&rpc_client, self.settings.chain, height as u32, true, cache)?;

        let result = rune_parser.index_runes(0, tx, *txid)?;
        if result.has_rune_updates() {
            let mut transaction_updater = TransactionUpdater::new(cache, None, true)?;
            transaction_updater.save(now as u32, height as u32, txid.clone(), tx, &result)?;
        }

        cache.set_mempool_tx(txid.clone());

        Ok(())
    }

    fn remove_txs(&self, txids: Vec<Txid>, mempool: bool) -> Result<()> {
        let db = self.db.write();
        let mut rollback_updater = Rollback::new(&db, mempool)?;

        // Remove transactions that are no longer in mempool
        for txid in txids {
            db.remove_mempool_tx(&txid)?;
            let tx_state_changes: crate::models::TransactionStateChange =
                db.get_tx_state_changes(&txid, Some(mempool))?;
            rollback_updater.revert_transaction(&txid, &tx_state_changes)?;
        }

        Ok(())
    }

    fn open_progress_bar(&self, current_height: u64, total_height: u64) -> ProgressBar {
        let progress_bar: ProgressBar = ProgressBar::new(total_height.into());
        progress_bar.set_position(current_height.into());
        progress_bar.set_style(
            ProgressStyle::with_template("[indexing blocks] {wide_bar} {pos}/{len}").unwrap(),
        );

        progress_bar
    }

    fn detect_reorg(
        &self,
        block: &BitcoinBlock,
        height: u64,
        client: &Client,
        max_recoverable_reorg_depth: u64,
    ) -> std::result::Result<(), ReorgError> {
        let db = self.db.read();
        let bitcoind_prev_blockhash = block.header.prev_blockhash;

        let mut prev_height = height.checked_sub(1).ok_or(ReorgError::Unrecoverable)?;
        match db.get_block_hash(prev_height as u64) {
            Ok(index_prev_blockhash) if index_prev_blockhash == bitcoind_prev_blockhash => Ok(()),
            Ok(index_prev_blockhash) if index_prev_blockhash != bitcoind_prev_blockhash => {
                for depth in 1..max_recoverable_reorg_depth {
                    prev_height = prev_height
                        .checked_sub(1)
                        .ok_or(ReorgError::Unrecoverable)?;
                    let index_block_hash = db.get_block_hash(prev_height as u64)?;
                    let bitcoind_block_hash =
                        client.get_block_hash(u64::from(prev_height.saturating_sub(depth)))?;

                    if index_block_hash == bitcoind_block_hash {
                        return Err(ReorgError::Recoverable { height, depth });
                    }
                }

                Err(ReorgError::Unrecoverable)
            }
            _ => Ok(()),
        }
    }

    fn handle_reorg(&self, _block: &BitcoinBlock, height: u64, depth: u64) -> Result<()> {
        // we're not at tip anymore.
        self.is_at_tip.store(false, Ordering::Release);

        info!(
            "Reorg detected at height {}, rolling back {} blocks",
            height, depth
        );

        {
            let db = self.db.write();

            // rollback block count indexed.
            db.set_block_count(height - depth + 1)?;
        }

        // Find rolled back blocks and revert those txs.
        for i in 1..depth {
            let block_height_rolled_back = height - i;
            let block = self.get_block_by_height(block_height_rolled_back)?;
            self.revert_block(block_height_rolled_back as u32, &block)?;
        }

        Ok(())
    }

    fn get_block_by_height(&self, height: u64) -> Result<Block> {
        let db = self.db.read();
        let block_hash = db.get_block_hash(height)?;
        let block = db.get_block_by_hash(&block_hash)?;
        Ok(block)
    }

    fn revert_block(&self, height: u32, block: &Block) -> Result<()> {
        let db = self.db.write();

        let mut rollback_updater = Rollback::new(&db, false)?;

        for tx in block.tx_ids.iter().rev() {
            let txid = Txid::from_str(tx)?;
            let tx_state_changes = db.get_tx_state_changes(&txid, Some(false))?;
            rollback_updater.revert_transaction(&txid, &tx_state_changes)?;
        }

        info!(
            "Reverted block {} with {} txs",
            block.header.block_hash(),
            block.tx_ids.len()
        );

        // Delete block
        db.delete_block(&block.header.block_hash())?;
        db.delete_block_hash(height as u64)?;

        Ok(())
    }

    fn insert_genesis_rune(&self) -> Result<()> {
        let rune = Rune(2055900680524219742);

        let id = RuneId { block: 1, tx: 0 };
        let etching = Txid::all_zeros();

        let rune = RuneEntry {
            block: id.block,
            burned: 0,
            divisibility: 0,
            etching,
            terms: Some(Terms {
                amount: Some(1),
                cap: Some(u128::MAX),
                height: (
                    Some((SUBSIDY_HALVING_INTERVAL * 4).into()),
                    Some((SUBSIDY_HALVING_INTERVAL * 5).into()),
                ),
                offset: (None, None),
            }),
            mints: 0,
            number: 0,
            premine: 0,
            spaced_rune: SpacedRune { rune, spacers: 128 },
            symbol: Some('\u{29C9}'),
            timestamp: 0,
            turbo: true,
            pending_burns: 0,
            pending_mints: 0,
            inscription_id: None,
        };

        Ok(())
    }
}
