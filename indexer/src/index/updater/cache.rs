use {
    super::{read_cache::ReadCache, store_lock::StoreWithLock},
    crate::{
        index::{store::StoreError, updater::read_cache::ReadCacheSettings, Chain, Settings},
        models::{
            BatchDelete, BatchUpdate, BlockId, Inscription, RuneEntry, TransactionStateChange,
        },
    },
    bitcoin::{consensus, BlockHash, OutPoint, ScriptBuf, Transaction, Txid},
    crossbeam_channel::{bounded, Sender},
    ordinals::{Rune, RuneId},
    rustc_hash::{FxHashMap, FxHashSet},
    std::{
        collections::{HashMap, HashSet},
        str::FromStr,
        sync::Arc,
        thread,
        time::{Duration, Instant},
    },
    titan_types::{
        Block, Event, InscriptionId, Location, MempoolEntry, SpenderReference, TxOutEntry,
    },
    tokio::sync::mpsc,
    tracing::info,
};

type Result<T> = std::result::Result<T, StoreError>;

pub(super) struct UpdaterCacheSettings {
    pub max_recoverable_reorg_depth: u64,
    pub mempool: bool,
    pub chain: Chain,
    pub read_cache: ReadCacheSettings,
    pub max_async_batches: usize,
}

impl UpdaterCacheSettings {
    pub fn new(settings: &Settings, mempool: bool) -> Self {
        Self {
            max_recoverable_reorg_depth: settings.max_recoverable_reorg_depth(),
            mempool,
            chain: settings.chain,
            read_cache: ReadCacheSettings::default(),
            max_async_batches: 8, // default batches in flight
        }
    }
}

pub(super) struct UpdaterCache {
    db: Arc<StoreWithLock>,
    update: BatchUpdate,
    delete: BatchDelete,
    events: Vec<Event>,
    first_block_height: u64,
    last_block_height: Option<u64>,
    pub settings: UpdaterCacheSettings,
    read_cache: ReadCache,

    // background flush
    bg_tx: Option<Sender<Arc<BatchUpdate>>>,
    pending_batches: Vec<Arc<BatchUpdate>>,
    // Handle to background writer thread so we can join on drop
    writer_handle: Option<std::thread::JoinHandle<()>>,
}

impl UpdaterCache {
    pub fn new(db: Arc<StoreWithLock>, settings: UpdaterCacheSettings) -> Result<Self> {
        let (rune_count, mut block_count, purged_blocks_count) = {
            let db = db.read();
            (
                db.get_runes_count()?,
                db.get_block_count()?,
                db.get_purged_blocks_count()?,
            )
        };

        // In regtest, the first block is not accessible via RPC and for testing purposes we
        // don't need to start at block 0.
        if block_count == 0 && settings.chain == Chain::Regtest {
            block_count = 1;
        }

        let read_cache_settings = settings.read_cache;

        Ok(Self {
            db,
            update: BatchUpdate::new(rune_count, block_count, purged_blocks_count),
            delete: BatchDelete::new(),
            events: vec![],
            first_block_height: block_count,
            last_block_height: None,
            read_cache: ReadCache::with_settings(read_cache_settings),
            settings,

            bg_tx: None,
            pending_batches: Vec::new(),
            writer_handle: None,
        })
    }

    pub fn get_runes_count(&self) -> u64 {
        self.update.rune_count
    }

    pub fn get_block_height_tip(&self) -> u64 {
        self.update.block_count.saturating_sub(1)
    }

    pub fn get_block_count(&self) -> u64 {
        self.update.block_count
    }

    pub fn get_purged_blocks_count(&self) -> u64 {
        self.update.purged_blocks_count
    }

    fn increment_block_count(&mut self) -> () {
        self.last_block_height = Some(self.update.block_count);
        self.update.block_count += 1;
    }

    pub fn get_block_hash(&mut self, height: u64) -> Result<BlockHash> {
        // 1. Check current in-memory update.
        if let Some(hash) = self.update.block_hashes.get(&height) {
            return Ok(hash.clone());
        }

        // 2. Check any pending batches that have been flushed but not yet persisted.
        if let Some(hash) = self.find_in_pending(|b| b.block_hashes.get(&height).cloned()) {
            return Ok(hash);
        }

        // 3. Check read-through cache.
        if let Some(hash) = self.read_cache.get_block_hash(&height) {
            return Ok(hash);
        }

        // 4. Fall back to the persistent store.
        let hash = self.db.read().get_block_hash(height)?;
        // Cache for future accesses.
        self.read_cache.insert_block_hash(height, hash.clone());
        Ok(hash)
    }

    pub fn get_block_by_height(&mut self, height: u64) -> Result<Arc<Block>> {
        let hash = self.get_block_hash(height)?;
        self.get_block(&hash)
    }

    pub fn get_block(&mut self, hash: &BlockHash) -> Result<Arc<Block>> {
        // 1. Check current in-memory update.
        if let Some(block) = self.update.blocks.get(hash) {
            return Ok(block.clone());
        }

        // 2. Check pending batches.
        if let Some(block) = self.find_in_pending(|b| b.blocks.get(hash).cloned()) {
            return Ok(block);
        }

        // 3. Check read cache.
        if let Some(block) = self.read_cache.get_block(hash) {
            return Ok(block);
        }

        // 4. Fetch from persistent store and cache.
        let block = self.db.read().get_block_by_hash(hash)?;
        let arc_block = Arc::new(block);
        self.read_cache
            .insert_block(hash.clone(), arc_block.clone());
        Ok(arc_block)
    }

    pub fn set_new_block(&mut self, block: Block) -> () {
        assert_eq!(
            self.get_block_count(),
            block.height,
            "Block height mismatch"
        );

        let hash: BlockHash = block.header.block_hash();
        self.update.blocks.insert(hash, Arc::new(block));
        self.update
            .block_hashes
            .insert(self.get_block_count(), hash);

        // Also cache in read cache for faster future reads
        self.read_cache
            .insert_block_hash(self.get_block_count(), hash);
        // Need the block Arc we stored
        if let Some(block_arc) = self.update.blocks.get(&hash) {
            self.read_cache.insert_block(hash, block_arc.clone());
        }

        self.increment_block_count();
    }

    pub fn increment_runes_count(&mut self) -> () {
        self.update.rune_count += 1;
    }

    pub fn decrement_runes_count(&mut self) -> () {
        self.update.rune_count -= 1;
    }

    pub fn get_transaction(&mut self, txid: Txid) -> Result<Arc<Transaction>> {
        // 1. Check current in-memory update.
        if let Some(transaction) = self.update.transactions.get(&txid) {
            return Ok(transaction.clone());
        }

        // 2. Check pending batches.
        if let Some(tx) = self.find_in_pending(|b| b.transactions.get(&txid).cloned()) {
            return Ok(tx);
        }

        // 3. Check read cache.
        if let Some(tx) = self.read_cache.get_transaction(&txid) {
            return Ok(tx);
        }

        // 4. Fallback to DB.
        let transaction_raw = self.db.read().get_transaction_raw(&txid, None)?;
        let tx: Transaction = consensus::deserialize(&transaction_raw)?;
        let arc_tx = Arc::new(tx);
        self.read_cache.insert_transaction(txid, arc_tx.clone());
        Ok(arc_tx)
    }

    pub fn get_transaction_confirming_block(&self, txid: Txid) -> Result<BlockId> {
        // 1. Check current in-memory update.
        if let Some(block_id) = self.update.transaction_confirming_block.get(&txid) {
            return Ok(block_id.clone());
        }

        // 2. Check pending batches.
        if let Some(block_id) =
            self.find_in_pending(|b| b.transaction_confirming_block.get(&txid).cloned())
        {
            return Ok(block_id);
        }

        // 3. Fallback to DB.
        let block_id = self.db.read().get_transaction_confirming_block(&txid)?;
        Ok(block_id)
    }

    pub fn precache_tx_outs(&mut self, txs: &Vec<Transaction>) -> Result<()> {
        // Collect unique input outpoints that are not yet cached.
        // Use FxHashSet for faster hashing performance and avoid allocating an
        // intermediate Vec of all outpoints.
        let mut to_fetch: FxHashSet<OutPoint> = FxHashSet::default();

        for tx in txs {
            for txin in &tx.input {
                let outpoint = txin.previous_output;

                // Skip coinbase/null outpoints – they do not have a corresponding TxOut.
                if outpoint.vout == u32::MAX {
                    continue;
                }

                if !self.update.txouts.contains_key(&outpoint)
                    && !self.read_cache.contains_tx_out(&outpoint)
                {
                    to_fetch.insert(outpoint);
                }
            }
        }

        if to_fetch.is_empty() {
            return Ok(());
        }

        let tx_outs = self
            .db
            .read()
            .get_tx_outs(&to_fetch.iter().cloned().collect::<Vec<_>>(), None)?;

        for (op, entry) in tx_outs {
            // Store in read cache for future fast reads (immutable reads only)
            self.read_cache.insert_tx_out(op.clone(), entry.clone());
        }

        Ok(())
    }

    pub fn get_tx_out(&mut self, outpoint: &OutPoint) -> Result<TxOutEntry> {
        // 1. Check current in-memory update.
        if let Some(tx_out) = self.update.txouts.get(outpoint) {
            return Ok(tx_out.clone());
        }

        // 2. Check pending batches.
        if let Some(tx_out) = self.find_in_pending(|b| b.txouts.get(outpoint).cloned()) {
            return Ok(tx_out);
        }

        // 3. Check read cache.
        if let Some(tx_out) = self.read_cache.get_tx_out(outpoint) {
            return Ok(tx_out);
        }

        // 4. Fallback to DB.
        let tx_out = self.db.read().get_tx_out(outpoint, None)?;
        // populate read cache for next time
        self.read_cache
            .insert_tx_out(outpoint.clone(), tx_out.clone());
        Ok(tx_out)
    }

    pub fn get_tx_outs(
        &mut self,
        outpoints: &Vec<OutPoint>,
    ) -> Result<HashMap<OutPoint, TxOutEntry>> {
        let mut results = HashMap::with_capacity(outpoints.len());
        let mut to_fetch = HashSet::with_capacity(outpoints.len());

        for outpoint in outpoints {
            if let Some(tx_out) = self.update.txouts.get(outpoint) {
                results.insert(*outpoint, tx_out.clone());
                continue;
            }

            if let Some(tx_out) = self.find_in_pending(|b| b.txouts.get(outpoint).cloned()) {
                results.insert(*outpoint, tx_out);
                continue;
            }

            if let Some(tx_out) = self.read_cache.get_tx_out(outpoint) {
                results.insert(*outpoint, tx_out);
            } else {
                to_fetch.insert(*outpoint);
            }
        }

        if !to_fetch.is_empty() {
            let tx_outs = self
                .db
                .read()
                .get_tx_outs(&to_fetch.iter().cloned().collect::<Vec<_>>(), None)?;

            for (op, entry) in &tx_outs {
                self.read_cache.insert_tx_out(op.clone(), entry.clone());
            }

            results.extend(tx_outs);
        }

        Ok(results)
    }

    pub fn set_tx_out(&mut self, outpoint: OutPoint, tx_out: TxOutEntry) -> () {
        self.update.txouts.insert(outpoint.clone(), tx_out.clone());
        self.read_cache.insert_tx_out(outpoint, tx_out);
    }

    pub fn does_tx_exist(&self, txid: Txid) -> Result<bool> {
        if self.update.tx_state_changes.contains_key(&txid) {
            return Ok(true);
        }

        // Check pending batches.
        if self
            .find_in_pending::<(), _>(|b| {
                if b.tx_state_changes.contains_key(&txid) {
                    Some(())
                } else {
                    None
                }
            })
            .is_some()
        {
            return Ok(true);
        }

        if self.settings.mempool {
            return self.db.read().is_tx_in_mempool(&txid);
        }

        let tx_state_changes = self
            .db
            .read()
            .get_tx_state_changes(&txid, Some(self.settings.mempool));

        Ok(tx_state_changes.is_ok())
    }

    pub fn set_tx_state_changes(
        &mut self,
        txid: Txid,
        tx_state_changes: TransactionStateChange,
    ) -> () {
        self.update.tx_state_changes.insert(txid, tx_state_changes);
    }

    pub fn set_transaction(&mut self, txid: Txid, transaction: Arc<Transaction>) -> () {
        self.update.transactions.insert(txid, transaction);
    }

    pub fn set_transaction_confirming_block(&mut self, txid: Txid, block_id: BlockId) -> () {
        self.update
            .transaction_confirming_block
            .insert(txid, block_id);
    }

    pub fn add_rune_transaction(&mut self, rune_id: RuneId, txid: Txid) -> () {
        self.update
            .rune_transactions
            .entry(rune_id)
            .or_insert(vec![])
            .push(txid);
    }

    pub fn get_rune(&mut self, rune_id: &RuneId) -> Result<RuneEntry> {
        // 1. Current update.
        if let Some(rune) = self.update.runes.get(rune_id) {
            return Ok(rune.clone());
        }

        // 2. Pending batches.
        if let Some(rune) = self.find_in_pending(|b| b.runes.get(rune_id).cloned()) {
            return Ok(rune);
        }

        // 3. Read cache.
        if let Some(rune) = self.read_cache.get_rune(rune_id) {
            return Ok(rune);
        }

        // 4. DB.
        let rune = self.db.read().get_rune(rune_id)?;
        self.read_cache.insert_rune(*rune_id, rune.clone());
        Ok(rune)
    }

    pub fn set_rune(&mut self, rune_id: RuneId, rune: RuneEntry) -> () {
        self.update.runes.insert(rune_id, rune.clone());
        self.read_cache.insert_rune(rune_id, rune);
    }

    pub fn get_rune_id(&mut self, rune: &Rune) -> Result<RuneId> {
        // 1. Current update.
        if let Some(rune_id) = self.update.rune_ids.get(&rune.0) {
            return Ok(rune_id.clone());
        }

        // 2. Pending batches.
        if let Some(rune_id) = self.find_in_pending(|b| b.rune_ids.get(&rune.0).cloned()) {
            return Ok(rune_id);
        }

        // 3. Read cache.
        if let Some(rune_id) = self.read_cache.get_rune_id(&rune.0) {
            return Ok(rune_id);
        }

        // 4. DB.
        let rune_id = self.db.read().get_rune_id(rune)?;
        self.read_cache.insert_rune_id(rune.0, rune_id.clone());
        Ok(rune_id)
    }

    pub fn set_rune_id(&mut self, rune: Rune, rune_id: RuneId) -> () {
        self.update.rune_ids.insert(rune.0, rune_id.clone());
        self.read_cache.insert_rune_id(rune.0, rune_id);
    }

    pub fn set_rune_id_number(&mut self, number: u64, rune_id: RuneId) -> () {
        self.update.rune_numbers.insert(number, rune_id);
    }

    pub fn set_inscription(
        &mut self,
        inscription_id: InscriptionId,
        inscription: Inscription,
    ) -> () {
        self.update.inscriptions.insert(inscription_id, inscription);
    }

    pub fn set_mempool_tx(&mut self, txid: Txid, mempool_entry: MempoolEntry) -> () {
        self.update.mempool_txs.insert(txid, mempool_entry);
    }

    pub fn set_script_pubkey_entries(
        &mut self,
        script_pubkey_entry: FxHashMap<ScriptBuf, (Vec<OutPoint>, Vec<OutPoint>)>,
    ) -> () {
        self.update.script_pubkeys = script_pubkey_entry;
    }

    pub fn get_outpoints_to_script_pubkey(
        &self,
        outpoints: &Vec<OutPoint>,
        optimistic: bool,
    ) -> Result<HashMap<OutPoint, ScriptBuf>> {
        let mut results = HashMap::with_capacity(outpoints.len());
        let mut to_fetch: Vec<OutPoint> = Vec::with_capacity(outpoints.len());

        for op in outpoints {
            if let Some(spk) = self.update.script_pubkeys_outpoints.get(op) {
                results.insert(*op, spk.clone());
                continue;
            }

            // Check any pending batches.
            if let Some(spk) = self.find_in_pending(|b| b.script_pubkeys_outpoints.get(op).cloned())
            {
                results.insert(*op, spk);
                continue;
            }

            to_fetch.push(*op);
        }

        if !to_fetch.is_empty() {
            let fetched = self.db.read().get_outpoints_to_script_pubkey(
                &to_fetch,
                Some(self.settings.mempool),
                optimistic,
            )?;
            results.extend(fetched);
        }

        Ok(results)
    }

    pub fn batch_set_outpoints_to_script_pubkey(&mut self, items: FxHashMap<OutPoint, ScriptBuf>) {
        self.update.script_pubkeys_outpoints = items;
    }

    pub fn batch_set_spent_outpoints_in_mempool(
        &mut self,
        outpoints: FxHashMap<OutPoint, SpenderReference>,
    ) {
        self.update.spent_outpoints_in_mempool.extend(outpoints);
    }

    pub fn add_event(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn should_flush(&self, max_size: usize) -> bool {
        self.update.blocks.len() >= max_size
    }

    pub fn flush(&mut self, at_tip: bool) -> Result<()> {
        // Spawn writer if needed
        self.ensure_writer();

        let db = self.db.write();

        if !self.settings.mempool && at_tip {
            self.prepare_to_delete(self.settings.max_recoverable_reorg_depth)?;
        }

        if !self.settings.mempool {
            // Async path
            if !self.update.is_empty() {
                if let Some(tx) = &self.bg_tx {
                    // Move current update into Arc and start a fresh empty one.
                    let new_update = BatchUpdate::new(
                        self.update.rune_count,
                        self.update.block_count,
                        self.update.purged_blocks_count,
                    );
                    let batch_arc = Arc::new(std::mem::replace(&mut self.update, new_update));
                    self.pending_batches.push(batch_arc.clone());
                    tx.send(batch_arc).expect("writer thread dropped");
                } else {
                    let start = Instant::now();
                    db.batch_update(&self.update, false)?;
                    info!("Flushed update: {} in {:?}", self.update, start.elapsed());
                    self.update.clear();
                }
            }
        } else {
            if !self.update.is_empty() {
                let start = Instant::now();
                db.batch_update(&self.update, true)?;
                info!("Flushed update: {} in {:?}", self.update, start.elapsed());
            }
        }

        if !self.delete.is_empty() {
            let start = Instant::now();
            db.batch_delete(&self.delete)?;
            info!("Flushed delete: {} in {:?}", self.delete, start.elapsed());
        }

        // Manage clearing policy
        if self.settings.mempool {
            self.update.clear();
        } else {
            // If this flush is final (at_tip) wait until background writes finished
            if at_tip {
                while !self.pending_batches.is_empty() {
                    self.cleanup_completed_batches();
                    thread::sleep(Duration::from_millis(50));
                }
                self.update.clear();
            } else {
                // If no pending writes, safe to clear now
                if self.pending_batches.is_empty() {
                    self.update.clear();
                }
            }
        }

        // Always clear delete batch
        self.delete.clear();
        self.first_block_height = self.update.block_count;
        self.last_block_height = None;

        Ok(())
    }

    pub fn add_address_events(&mut self, chain: Chain) {
        for script_pubkey in self.update.script_pubkeys.keys() {
            let address = chain.address_from_script(script_pubkey);
            if let Ok(address) = address {
                self.events.push(Event::AddressModified {
                    address: address.to_string(),
                    location: if self.settings.mempool {
                        Location::mempool()
                    } else {
                        Location::block(self.get_block_height_tip())
                    },
                });
            }
        }
    }

    pub fn send_events(
        &mut self,
        event_sender: &Option<mpsc::Sender<Event>>,
    ) -> std::result::Result<(), mpsc::error::SendError<Event>> {
        if let Some(sender) = event_sender {
            for event in self.events.drain(..) {
                sender.blocking_send(event)?;
            }
        } else {
            self.events.clear();
        }

        Ok(())
    }

    fn prepare_to_delete(&mut self, max_recoverable_reorg_depth: u64) -> Result<()> {
        if let Some(last_block_height) = self.last_block_height {
            let from_block_height_to_purge = self.update.purged_blocks_count;

            let to_block_height_to_purge =
                last_block_height.checked_sub(max_recoverable_reorg_depth);

            if let Some(to_block_height_to_purge) = to_block_height_to_purge {
                info!(
                    "Purging blocks from {} to {}",
                    from_block_height_to_purge, to_block_height_to_purge,
                );

                // Use the new batched purge implementation to minimize DB round-trips
                self.purge_block_range(from_block_height_to_purge, to_block_height_to_purge)?;
            }
        }

        Ok(())
    }

    fn get_txs_state_changes(
        &self,
        txids: &Vec<Txid>,
    ) -> Result<HashMap<Txid, TransactionStateChange>> {
        let mut total_tx_state_changes = HashMap::with_capacity(txids.len());
        let mut to_fetch = HashSet::with_capacity(txids.len());
        for txid in txids {
            if let Some(tx_state_changes) = self.update.tx_state_changes.get(txid) {
                total_tx_state_changes.insert(txid.clone(), tx_state_changes.clone());
            } else if let Some(tx_state_changes) =
                self.find_in_pending(|b| b.tx_state_changes.get(txid).cloned())
            {
                total_tx_state_changes.insert(txid.clone(), tx_state_changes);
            } else {
                to_fetch.insert(txid.clone());
            }
        }

        if !to_fetch.is_empty() {
            let tx_state_changes = self.db.read().get_txs_state_changes(
                &to_fetch.iter().cloned().collect(),
                self.settings.mempool,
            )?;

            total_tx_state_changes.extend(tx_state_changes);
        }

        Ok(total_tx_state_changes)
    }

    /// Purge a contiguous range of block heights `[from, to)` in batch to reduce
    /// RocksDB look-ups. Requires `from < to`.
    fn purge_block_range(&mut self, from: u64, to: u64) -> Result<()> {
        if from >= to {
            return Ok(());
        }

        // 1. Load all blocks and gather txids
        let mut blocks: Vec<(u64, Arc<Block>)> = Vec::with_capacity((to - from) as usize);
        let mut all_txids: Vec<Txid> = Vec::new();

        for height in from..to {
            let block = self.get_block_by_height(height)?;

            for txid_str in &block.tx_ids {
                all_txids.push(Txid::from_str(txid_str).unwrap());
            }

            blocks.push((height, block));
        }

        // 2. Fetch all tx state changes in one DB call
        let tx_state_changes = self.get_txs_state_changes(&all_txids)?;

        // 3. Apply deletions
        for (height, block) in blocks {
            for txid_str in &block.tx_ids {
                let txid = Txid::from_str(txid_str).unwrap();

                let changes = tx_state_changes.get(&txid).unwrap_or_else(|| {
                    panic!(
                        "Tx state changes not found for txid: {} in block {}",
                        txid, height
                    );
                });

                for txin in &changes.inputs {
                    self.delete.script_pubkeys_outpoints.insert(txin.clone());
                }

                self.delete.tx_state_changes.insert(txid);
            }
        }

        // Update purged count
        self.update.purged_blocks_count = to;

        Ok(())
    }

    fn ensure_writer(&mut self) {
        if self.bg_tx.is_some() || self.settings.mempool {
            return;
        }

        let (tx, rx) = bounded::<Arc<BatchUpdate>>(self.settings.max_async_batches);
        let db = self.db.clone();

        let handle = thread::Builder::new()
            .name("rocksdb-writer".into())
            .spawn(move || {
                while let Ok(batch) = rx.recv() {
                    let store = db.write();
                    // Safety: Only background thread writes via batch_update.
                    if let Err(e) = store.batch_update(&batch, false) {
                        tracing::error!("Background RocksDB write failed: {:?}", e);
                    }
                }
            })
            .expect("failed to spawn rocksdb-writer thread");

        self.bg_tx = Some(tx);
        self.writer_handle = Some(handle);
    }

    fn cleanup_completed_batches(&mut self) {
        self.pending_batches.retain(|b| Arc::strong_count(b) > 1);
    }

    // Reserve capacity in frequently-written structures to avoid repeated
    // re-allocations during large blocks. These are cheap no-ops if the map
    // already has enough free slots.
    pub fn reserve_txouts(&mut self, additional: usize) {
        self.update.txouts.reserve(additional);
    }

    pub fn reserve_tx_state_changes(&mut self, additional: usize) {
        self.update.tx_state_changes.reserve(additional);
    }

    /// Search a value inside the currently pending (still in–flight) batch updates.
    /// The provided closure will be executed on each `BatchUpdate` starting from the
    /// most-recent one (the last pushed into `pending_batches`). The first `Some` value
    /// returned will be forwarded. This is a generic utility that avoids repeating the
    /// same lookup logic for every data type we need to read while a background write
    /// is ongoing.
    fn find_in_pending<T, F>(&self, finder: F) -> Option<T>
    where
        F: Fn(&BatchUpdate) -> Option<T>,
    {
        // Iterate newest → oldest to make sure we prefer the most recently flushed batch.
        for batch in self.pending_batches.iter().rev() {
            if let Some(val) = finder(batch.as_ref()) {
                return Some(val);
            }
        }
        None
    }
}

impl Drop for UpdaterCache {
    fn drop(&mut self) {
        // Close the sender side of the channel first so the background
        // thread can finish processing and exit its loop.
        self.bg_tx.take();

        // Wait for the background writer thread to finish to guarantee that
        // all pending updates have been flushed before shutting down.
        if let Some(handle) = self.writer_handle.take() {
            if let Err(err) = handle.join() {
                tracing::error!("Failed to join rocksdb-writer thread: {:?}", err);
            }
        }
    }
}
