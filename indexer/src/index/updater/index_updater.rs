use {
    super::{
        transaction_update::{TransactionChangeSet, TransactionUpdate},
        *,
    },
    crate::{
        index::{
            metrics::Metrics, store::Store, RpcClientError, RpcClientProvider, Settings, StoreError,
        },
        models::{BlockId, RuneEntry},
    },
    address::AddressUpdater,
    bitcoin::{
        constants::SUBSIDY_HALVING_INTERVAL, hashes::Hash, hex::HexToArrayError,
        Block as BitcoinBlock, Transaction, Txid,
    },
    bitcoincore_rpc::{Client, RpcApi},
    block_fetcher::fetch_blocks_from,
    cache::{UpdaterCache, UpdaterCacheSettings},
    indicatif::{ProgressBar, ProgressStyle},
    mempool::MempoolError,
    ordinals::{Rune, RuneId, SpacedRune, Terms},
    prometheus::HistogramVec,
    rollback::{Rollback, RollbackError},
    std::{
        collections::HashMap,
        fmt::{self, Display, Formatter},
        str::FromStr,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex, RwLock,
        },
        time::{SystemTime, UNIX_EPOCH},
    },
    store_lock::StoreWithLock,
    thiserror::Error,
    titan_types::{Block, Event},
    tokio::sync::mpsc::{error::SendError, Sender},
    tracing::{debug, error, info, trace},
    transaction_parser::TransactionParser,
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
    RuneParser(#[from] TransactionParserError),
    #[error("txid error {0}")]
    Txid(#[from] HexToArrayError),
    #[error("mempool error {0}")]
    Mempool(#[from] MempoolError),
    #[error("mutex error")]
    Mutex,
    #[error("event sender error {0}")]
    EventSender(#[from] SendError<Event>),
    #[error("invalid main chain tip")]
    InvalidMainChainTip,
}

type Result<T> = std::result::Result<T, UpdaterError>;

pub struct Updater {
    db: Arc<StoreWithLock>,
    settings: Settings,
    is_at_tip: AtomicBool,

    shutdown_flag: Arc<AtomicBool>,

    broadcast_lock: Mutex<()>,
    zmq_received_txs: RwLock<HashMap<Txid, Transaction>>,

    transaction_update: RwLock<TransactionUpdate>,

    sender: Option<Sender<Event>>,

    // monitoring
    latency: HistogramVec,
}

impl Updater {
    pub fn new(
        db: Arc<dyn Store + Send + Sync>,
        settings: Settings,
        metrics: &Metrics,
        shutdown_flag: Arc<AtomicBool>,
        sender: Option<Sender<Event>>,
    ) -> Self {
        Self {
            db: Arc::new(StoreWithLock::new(db)),
            settings,
            is_at_tip: AtomicBool::new(false),
            broadcast_lock: Mutex::new(()),
            zmq_received_txs: RwLock::new(HashMap::new()),
            shutdown_flag,
            transaction_update: RwLock::new(TransactionUpdate::default()),
            sender,
            latency: metrics.histogram_vec(
                prometheus::HistogramOpts::new("indexer_latency", "Indexer latency"),
                &["method"],
            ),
        }
    }

    pub fn is_at_tip(&self) -> bool {
        self.is_at_tip.load(Ordering::Relaxed)
    }

    pub fn update_to_tip(&self) -> Result<()> {
        debug!("Updating to tip");

        // Every 5000 blocks, commit the changes to the database
        let commit_interval = self.settings.commit_interval as usize;
        let mut address_updater = AddressUpdater::new();
        let mut cache = UpdaterCache::new(
            self.db.clone(),
            UpdaterCacheSettings::new(&self.settings, false),
        )?;

        // Get RPC client and get block height
        let bitcoin_block_client = self.settings.get_new_rpc_client()?;
        let mut bitcoin_block_count = bitcoin_block_client.get_block_count()?;

        let mut first_block = true;

        // Fetch new blocks if needed.
        while bitcoin_block_count > cache.get_block_height_tip() {
            let was_at_tip = self.is_at_tip.load(Ordering::Relaxed);
            self.is_at_tip.store(false, Ordering::Release);

            let progress_bar =
                self.open_progress_bar(cache.get_block_height_tip(), bitcoin_block_count);

            let rx = fetch_blocks_from(
                Arc::new(self.settings.clone()),
                cache.get_block_count(),
                bitcoin_block_count + 1,
            )?;

            let rpc_client = self.settings.get_new_rpc_client()?;

            while let Ok(block) = rx.recv() {
                if self.shutdown_flag.load(Ordering::SeqCst) {
                    info!("Updater received shutdown signal, stopping...");
                    break;
                }

                if was_at_tip || first_block {
                    match self.detect_reorg(
                        &block,
                        cache.get_block_count(),
                        &rpc_client,
                        self.settings.max_recoverable_reorg_depth(),
                    ) {
                        Ok(()) => (),
                        Err(ReorgError::Recoverable { height, depth }) => {
                            self.handle_reorg(height, depth)?;
                            if let Some(sender) = &self.sender {
                                sender.blocking_send(Event::Reorg { height, depth })?;
                            }
                            return Err(ReorgError::Recoverable { height, depth }.into());
                        }
                        Err(e) => {
                            return Err(e.into());
                        }
                    }
                }

                let block = self.index_block(
                    block,
                    cache.get_block_count() as u64,
                    &rpc_client,
                    &mut cache,
                    &mut address_updater,
                )?;

                cache.add_event(Event::NewBlock {
                    block_height: cache.get_block_count(),
                    block_hash: block.header.block_hash(),
                });

                cache.set_new_block(block);

                if cache.should_flush(commit_interval) {
                    if self.settings.index_addresses {
                        let _timer = self
                            .latency
                            .with_label_values(&["batch_update_script_pubkeys_for_commit"])
                            .start_timer();
                        address_updater.batch_update_script_pubkey(&mut cache)?;
                    }

                    cache.add_address_events(self.settings.chain);
                    cache.flush()?;
                    cache.send_events(&self.sender)?;
                }

                first_block = false;
                progress_bar.inc(1);

                if bitcoin_block_count - cache.get_block_height_tip()
                    > self.settings.max_recoverable_reorg_depth()
                {
                    // clear notifications.
                    self.notify_tx_updates(true)?;
                }
            }

            if self.shutdown_flag.load(Ordering::SeqCst) {
                info!("Updater received shutdown signal, stopping...");
                break;
            }

            info!("Synced to tip {}", bitcoin_block_count);
            bitcoin_block_count = bitcoin_block_client.get_block_count()?;
            progress_bar.finish_and_clear();
        }

        // Flush the cache to the database
        if self.settings.index_addresses {
            let _timer = self
                .latency
                .with_label_values(&["batch_update_script_pubkeys_for_block"])
                .start_timer();
            address_updater.batch_update_script_pubkey(&mut cache)?;
        }

        cache.add_address_events(self.settings.chain);
        cache.flush()?;
        cache.send_events(&self.sender)?;

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
        let lock = self.broadcast_lock.lock().unwrap();
        let current_mempool = client.get_raw_mempool_verbose()?;

        // Get our previously indexed mempool transactions
        let stored_mempool = {
            let db = self.db.read();
            db.get_mempool_txids()?
        };
        drop(lock);
        
        // Find new transactions to index
        let new_txs: Vec<Txid> = current_mempool
            .iter()
            .filter(|(txid, _)| !stored_mempool.contains(*txid))
            .map(|(txid, _)| txid.clone())
            .collect();

        // Find transactions to remove (they're no longer in mempool)
        let removed_txs: Vec<Txid> = stored_mempool
            .iter()
            .filter(|txid| !current_mempool.contains_key(*txid))
            .cloned()
            .collect();

        // Index new transactions
        let new_txs_len = new_txs.len();
        if new_txs_len > 0 {
            let tx_map = {
                let mut zmq_txns = self
                    .zmq_received_txs
                    .read()
                    .map_err(|_| UpdaterError::Mutex)?
                    .clone();

                let (to_index, to_fetch): (Vec<Txid>, Vec<Txid>) = new_txs
                    .iter()
                    .partition(|txid| zmq_txns.contains_key(*txid));

                if to_index.len() != zmq_txns.len() {
                    zmq_txns.retain(|txid, _| to_index.contains(txid));
                }

                if !to_fetch.is_empty() {
                    let fetched = mempool::fetch_transactions(
                        Arc::new(self.settings.clone()),
                        &to_fetch,
                        self.shutdown_flag.clone(),
                    );

                    zmq_txns.extend(fetched);
                }

                zmq_txns
            };

            let tx_order = mempool::sort_transaction_order(&current_mempool, &tx_map)?;
            let mut cache = UpdaterCache::new(
                self.db.clone(),
                UpdaterCacheSettings::new(&self.settings, true),
            )?;

            let mut address_updater = AddressUpdater::new();
            for txid in tx_order {
                let tx = tx_map.get(&txid).unwrap();
                self.index_tx(&txid, &tx, &mut cache, Some(&mut address_updater))?;
            }

            if self.settings.index_addresses {
                address_updater.batch_update_script_pubkey(&mut cache)?;
            }

            cache.add_address_events(self.settings.chain);
            cache.flush()?;
            cache.send_events(&self.sender)?;
        }

        let removed_len = removed_txs.len();
        if removed_txs.len() > 0 {
            self.remove_txs(&removed_txs, true)?;
        }

        if new_txs_len > 0 || removed_len > 0 {
            info!(
                "Mempool: New txs: {}. Removed txs: {}",
                new_txs_len, removed_len
            );
        }

        self.transaction_update
            .write()
            .map_err(|_| UpdaterError::Mutex)?
            .update_mempool(TransactionChangeSet {
                added: new_txs.into_iter().collect(),
                removed: removed_txs.into_iter().collect(),
            });

        self.zmq_received_txs
            .write()
            .map_err(|_| UpdaterError::Mutex)?
            .clear();

        Ok(())
    }

    fn index_block(
        &self,
        bitcoin_block: BitcoinBlock,
        height: u64,
        rpc_client: &Client,
        cache: &mut UpdaterCache,
        address_updater: &mut AddressUpdater,
    ) -> Result<Block> {
        let _timer = self
            .latency
            .with_label_values(&["index_block"])
            .start_timer();

        cache.precache_tx_outs(&bitcoin_block.txdata)?;

        let mut transaction_parser =
            TransactionParser::new(&rpc_client, self.settings.chain, height, false)?;

        let block_header: bitcoin::block::Header = bitcoin_block.header.clone();
        let block_height = height;

        let _block_tx_timer = self
            .latency
            .with_label_values(&["parse_block&index_block_txs"])
            .start_timer();

        let mut transaction_updater =
            TransactionUpdater::new(self.settings.clone().into(), Some(address_updater))?;

        let mut block = Block::empty_block(height, bitcoin_block.header);

        let mut transaction_update = self
            .transaction_update
            .write()
            .map_err(|_| UpdaterError::Mutex)?;

        for (i, tx) in bitcoin_block.txdata.iter().enumerate() {
            let txid = tx.compute_txid();
            match transaction_parser.parse(cache, u32::try_from(i).unwrap(), tx) {
                Ok(result) => {
                    transaction_updater.save(
                        cache,
                        block_header.time,
                        Some(BlockId {
                            hash: bitcoin_block.header.block_hash(),
                            height: block_height,
                        }),
                        txid,
                        &bitcoin_block.txdata[i],
                        &result,
                    )?;
                    block.tx_ids.push(txid.to_string());
                    transaction_update.add_block_tx(txid);
                    if let Some((id, ..)) = result.etched {
                        block.etched_runes.push(id);
                    }
                }
                Err(e) => {
                    panic!("Failed to index transaction {}: {}", txid, e);
                }
            }
        }

        Ok(block)
    }

    pub fn index_new_broadcasted_tx(&self, txid: &Txid, tx: &Transaction) -> Result<()> {
        let mut cache = UpdaterCache::new(
            self.db.clone(),
            UpdaterCacheSettings::new(&self.settings, true),
        )?;

        let _lock = self.broadcast_lock.lock().unwrap();
        let mut address_updater = AddressUpdater::new();
        if self.index_tx(txid, tx, &mut cache, Some(&mut address_updater))? {
            cache.add_event(Event::TransactionSubmitted { txid: *txid });

            if self.settings.index_addresses {
                address_updater.batch_update_script_pubkey(&mut cache)?;
            }

            cache.add_address_events(self.settings.chain);
            cache.flush()?;
            cache.send_events(&self.sender)?;
        }

        Ok(())
    }

    pub fn index_zmq_tx(&self, txid: Txid, tx: Transaction) -> Result<()> {
        self.zmq_received_txs
            .write()
            .map_err(|_| UpdaterError::Mutex)?
            .insert(txid, tx);
        Ok(())
    }

    fn index_tx(
        &self,
        txid: &Txid,
        tx: &Transaction,
        cache: &mut UpdaterCache,
        address_updater: Option<&mut AddressUpdater>,
    ) -> Result<bool> {
        if cache.does_tx_exist(*txid)? {
            trace!(
                "Skipping tx {} in {} because it already exists",
                txid,
                if cache.settings.mempool {
                    "mempool"
                } else {
                    "block"
                }
            );
            return Ok(false);
        }

        let _timer = self.latency.with_label_values(&["index_tx"]).start_timer();

        let height = cache.get_block_count();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let rpc_client = self.settings.get_new_rpc_client()?;
        let mut transaction_parser =
            TransactionParser::new(&rpc_client, self.settings.chain, height, true)?;

        let result = transaction_parser.parse(cache, 0, tx)?;
        info!("Indexing tx {}", txid);

        // Create a TransactionUpdater that references the optional address_updater
        let mut transaction_updater =
            TransactionUpdater::new(self.settings.clone().into(), address_updater)?;

        // The same "save" logic as before
        transaction_updater.save(cache, now as u32, None, *txid, tx, &result)?;

        if cache.settings.mempool {
            cache.set_mempool_tx(*txid);
        }

        Ok(true)
    }

    fn remove_txs(&self, txids: &Vec<Txid>, mempool: bool) -> Result<()> {
        let db = self.db.write();
        let mut rollback_updater = Rollback::new(&db, self.settings.clone().into(), mempool)?;
        rollback_updater.revert_transactions(txids)?;
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
        if height == 0 {
            return Ok(());
        }

        let db = self.db.read();
        let bitcoind_prev_blockhash = block.header.prev_blockhash;

        let prev_height = height.checked_sub(1).ok_or(ReorgError::Unrecoverable)?;
        match db.get_block_hash(prev_height as u64) {
            Ok(index_prev_blockhash) if index_prev_blockhash == bitcoind_prev_blockhash => Ok(()),
            Ok(index_prev_blockhash) if index_prev_blockhash != bitcoind_prev_blockhash => {
                for depth in 1..max_recoverable_reorg_depth {
                    let height_to_check = height.saturating_sub(depth);
                    let index_block_hash = db.get_block_hash(height_to_check)?;
                    let bitcoind_block_hash = client.get_block_hash(height_to_check)?;

                    if index_block_hash == bitcoind_block_hash {
                        info!("Reorg until height {}. Depth: {}", height_to_check, depth);
                        return Err(ReorgError::Recoverable { height, depth });
                    }
                }

                Err(ReorgError::Unrecoverable)
            }
            _ => Ok(()),
        }
    }

    fn handle_reorg(&self, height: u64, depth: u64) -> Result<()> {
        // we're not at tip anymore.
        self.is_at_tip.store(false, Ordering::Release);

        info!(
            "Reorg detected at height {}, rolling back {} blocks",
            height, depth
        );

        {
            let db = self.db.write();

            // rollback block count indexed.
            // +1 because this is the block count and not the block height, therefore block count is always height + 1.
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

        let mut rollback_updater = Rollback::new(&db, self.settings.clone().into(), false)?;

        let txids = block
            .tx_ids
            .iter()
            .map(|txid| Txid::from_str(txid).unwrap())
            .collect::<Vec<_>>();

        rollback_updater.revert_transactions(&txids)?;

        {
            let mut transaction_update = self
                .transaction_update
                .write()
                .map_err(|_| UpdaterError::Mutex)?;

            for txid in txids.iter().rev() {
                transaction_update.remove_block_tx(*txid);
            }
        }

        info!(
            "Reverted block {}:{} with {} txs",
            height,
            block.header.block_hash(),
            block.tx_ids.len()
        );

        // Delete block
        db.delete_block(&block.header.block_hash())?;
        db.delete_block_hash(height as u64)?;

        Ok(())
    }

    pub fn notify_tx_updates(&self, flush: bool) -> Result<()> {
        let _timer = self
            .latency
            .with_label_values(&["notify_tx_updates"])
            .start_timer();
        if self.settings.index_bitcoin_transactions {
            let categorized = {
                let transaction_update = self
                    .transaction_update
                    .read()
                    .map_err(|_| UpdaterError::Mutex)?;

                transaction_update.categorize_to_change_set()
            };

            if let Some(sender) = &self.sender {
                if !flush && (!categorized.removed.is_empty() || !categorized.added.is_empty()) {
                    let client = self.settings.get_new_rpc_client()?;

                    let chain_info = client.get_blockchain_info()?;

                    let block = {
                        let db = self.db.read();
                        db.get_block_by_hash(&chain_info.best_block_hash)
                    };

                    let Ok(block) = block else {
                        return Err(UpdaterError::InvalidMainChainTip);
                    };

                    if block.height != chain_info.blocks {
                        return Err(UpdaterError::InvalidMainChainTip);
                    }
                }

                if !categorized.removed.is_empty() {
                    let (_, not_exists) = {
                        let db = self.db.read();
                        db.partition_transactions_by_existence(
                            &categorized.removed.into_iter().collect(),
                        )?
                    };

                    sender.blocking_send(Event::TransactionsReplaced { txids: not_exists })?;
                }

                if !categorized.added.is_empty() {
                    sender.blocking_send(Event::TransactionsAdded {
                        txids: categorized.added.into_iter().collect(),
                    })?;
                }
            }
        }

        self.transaction_update
            .write()
            .map_err(|_| UpdaterError::Mutex)?
            .reset();

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
