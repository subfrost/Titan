use {
    super::store_lock::StoreWithLock,
    crate::{
        index::{store::StoreError, Settings},
        models::{
            BatchDelete, BatchUpdate, Inscription, RuneEntry, ScriptPubkeyEntry,
            TransactionStateChange,
        },
    },
    bitcoin::{BlockHash, OutPoint, ScriptBuf, Transaction, Txid},
    ordinals::{Rune, RuneId},
    std::{
        cmp,
        collections::{HashMap, HashSet},
        str::FromStr,
        sync::Arc,
    },
    tracing::{info, trace},
    types::{Block, InscriptionId, TxOutEntry},
};

type Result<T> = std::result::Result<T, StoreError>;

pub(super) struct UpdaterCacheSettings {
    pub max_recoverable_reorg_depth: u64,
    pub index_spent_outputs: bool,
    pub mempool: bool,
}

impl UpdaterCacheSettings {
    pub fn new(settings: &Settings, mempool: bool) -> Self {
        Self {
            max_recoverable_reorg_depth: settings.max_recoverable_reorg_depth(),
            index_spent_outputs: settings.index_spent_outputs,
            mempool,
        }
    }
}

pub(super) struct UpdaterCache {
    db: Arc<StoreWithLock>,
    update: BatchUpdate,
    delete: BatchDelete,
    first_block_height: u64,
    last_block_height: Option<u64>,
    pub settings: UpdaterCacheSettings,
}

impl UpdaterCache {
    pub fn new(db: Arc<StoreWithLock>, settings: UpdaterCacheSettings) -> Result<Self> {
        let (rune_count, block_count, purged_blocks_count) = {
            let db = db.read();
            (
                db.get_runes_count()?,
                db.get_block_count()?,
                db.get_purged_blocks_count()?,
            )
        };

        Ok(Self {
            db,
            update: BatchUpdate::new(rune_count, block_count, purged_blocks_count),
            delete: BatchDelete::new(),
            first_block_height: block_count,
            last_block_height: None,
            settings,
        })
    }

    pub fn get_runes_count(&self) -> u64 {
        self.update.rune_count
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

    pub fn get_block_by_height(&self, height: u64) -> Result<Block> {
        let hash = self.update.block_hashes.get(&height);

        if let Some(hash) = hash {
            return self.get_block(hash);
        } else {
            let hash = self.db.read().get_block_hash(height)?;
            return self.get_block(&hash);
        }
    }

    pub fn get_block(&self, hash: &BlockHash) -> Result<Block> {
        if let Some(block) = self.update.blocks.get(hash) {
            return Ok(block.clone());
        } else {
            let block = self.db.read().get_block_by_hash(hash)?;
            return Ok(block);
        }
    }

    pub fn set_new_block(&mut self, block: Block) -> () {
        let hash: BlockHash = block.header.block_hash();
        self.update.blocks.insert(hash, block);
        self.update
            .block_hashes
            .insert(self.get_block_count(), hash);
        self.increment_block_count();
    }

    pub fn increment_runes_count(&mut self) -> () {
        self.update.rune_count += 1;
    }

    pub fn decrement_runes_count(&mut self) -> () {
        self.update.rune_count -= 1;
    }

    pub fn get_tx_out(&self, outpoint: &OutPoint) -> Result<TxOutEntry> {
        if let Some(tx_out) = self.update.txouts.get(outpoint) {
            return Ok(tx_out.clone());
        } else {
            let tx_out = self.db.read().get_tx_out(outpoint, None)?;
            return Ok(tx_out);
        }
    }

    pub fn get_tx_outs(&self, outpoints: &Vec<OutPoint>) -> Result<HashMap<OutPoint, TxOutEntry>> {
        let mut results = HashMap::new();
        let mut to_fetch = HashSet::new();
        for outpoint in outpoints.iter() {
            if let Some(tx_out) = self.update.txouts.get(outpoint) {
                results.insert(outpoint.clone(), tx_out.clone());
            } else {
                to_fetch.insert(outpoint.clone());
            }
        }

        if !to_fetch.is_empty() {
            let tx_outs = self
                .db
                .read()
                .get_tx_outs(&to_fetch.iter().cloned().collect(), None)?;
            for (outpoint, tx_out) in tx_outs.iter() {
                results.insert(outpoint.clone(), tx_out.clone());
            }
        }

        Ok(results)
    }

    pub fn set_tx_out(&mut self, outpoint: OutPoint, tx_out: TxOutEntry) -> () {
        self.update.txouts.insert(outpoint, tx_out);
    }

    pub fn get_tx_state_changes(&self, txid: Txid) -> Result<TransactionStateChange> {
        if let Some(tx_state_changes) = self.update.tx_state_changes.get(&txid) {
            return Ok(tx_state_changes.clone());
        } else {
            let tx_state_changes = self
                .db
                .read()
                .get_tx_state_changes(&txid, Some(self.settings.mempool))?;
            return Ok(tx_state_changes);
        }
    }

    pub fn does_tx_exist(&self, txid: Txid) -> Result<bool> {
        if self.update.tx_state_changes.contains_key(&txid) {
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

    pub fn set_transaction(&mut self, txid: Txid, transaction: Transaction) -> () {
        self.update.transactions.insert(txid, transaction);
    }

    pub fn add_rune_transaction(&mut self, rune_id: RuneId, txid: Txid) -> () {
        self.update
            .rune_transactions
            .entry(rune_id)
            .or_insert(vec![])
            .push(txid);
    }

    pub fn get_rune(&self, rune_id: &RuneId) -> Result<RuneEntry> {
        if let Some(rune) = self.update.runes.get(rune_id) {
            return Ok(rune.clone());
        } else {
            let rune = self.db.read().get_rune(rune_id)?;
            return Ok(rune);
        }
    }

    pub fn set_rune(&mut self, rune_id: RuneId, rune: RuneEntry) -> () {
        self.update.runes.insert(rune_id, rune);
    }

    pub fn get_rune_id(&self, rune: &Rune) -> Result<RuneId> {
        if let Some(rune_id) = self.update.rune_ids.get(&rune.0) {
            return Ok(rune_id.clone());
        } else {
            let rune_id = self.db.read().get_rune_id(rune)?;
            return Ok(rune_id);
        }
    }

    pub fn set_rune_id(&mut self, rune: Rune, rune_id: RuneId) -> () {
        self.update.rune_ids.insert(rune.0, rune_id);
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

    pub fn set_mempool_tx(&mut self, txid: Txid) -> () {
        self.update.mempool_txs.insert(txid);
    }

    pub fn get_script_pubkey_entries(
        &self,
        script_pubkeys: &Vec<ScriptBuf>,
    ) -> Result<HashMap<ScriptBuf, ScriptPubkeyEntry>> {
        let mut results = HashMap::new();
        let mut to_fetch = HashSet::new();
        for script_pubkey in script_pubkeys.iter() {
            if let Some(script_pubkey_entry) = self.update.script_pubkeys.get(script_pubkey) {
                results.insert(script_pubkey.clone(), script_pubkey_entry.clone());
            } else {
                to_fetch.insert(script_pubkey.clone());
            }
        }

        if !to_fetch.is_empty() {
            let script_pubkeys_entries = self.db.read().get_script_pubkey_entries(
                &to_fetch.iter().cloned().collect(),
                self.settings.mempool,
            )?;

            for (script_pubkey, script_pubkey_entry) in script_pubkeys_entries.iter() {
                results.insert(script_pubkey.clone(), script_pubkey_entry.clone());
            }
        }

        return Ok(results);
    }

    pub fn set_script_pubkey_entry(
        &mut self,
        script_pubkey: ScriptBuf,
        script_pubkey_entry: ScriptPubkeyEntry,
    ) -> () {
        self.update
            .script_pubkeys
            .insert(script_pubkey, script_pubkey_entry);
    }

    pub fn get_outpoints_to_script_pubkey(
        &self,
        outpoints: Vec<OutPoint>,
        optimistic: bool,
    ) -> Result<HashMap<OutPoint, ScriptBuf>> {
        let mut results = HashMap::new();
        let mut to_fetch = HashSet::new();
        for outpoint in outpoints.iter() {
            if let Some(script_pubkey) = self.update.script_pubkeys_outpoints.get(outpoint) {
                results.insert(outpoint.clone(), script_pubkey.clone());
            } else {
                to_fetch.insert(outpoint.clone());
            }
        }

        if !to_fetch.is_empty() {
            let script_pubkeys = self.db.read().get_outpoints_to_script_pubkey(
                &to_fetch.iter().cloned().collect(),
                None,
                optimistic,
            )?;

            for (outpoint, script_pubkey) in script_pubkeys.iter() {
                results.insert(outpoint.clone(), script_pubkey.clone());
            }
        }

        return Ok(results);
    }

    pub fn batch_set_outpoints_to_script_pubkey(&mut self, items: &[(OutPoint, ScriptBuf)]) {
        for (outpoint, script_pubkey) in items {
            self.update
                .script_pubkeys_outpoints
                .insert(*outpoint, script_pubkey.clone());
        }
    }

    pub fn batch_set_spent_outpoints_in_mempool(&mut self, outpoints: Vec<OutPoint>) {
        self.update.spent_outpoints_in_mempool.extend(outpoints);
    }

    pub fn should_flush(&self, max_size: usize) -> bool {
        self.update.blocks.len() >= max_size
    }

    pub fn flush(&mut self) -> Result<()> {
        let db = self.db.write();

        if !self.settings.mempool {
            self.prepare_to_delete(
                self.settings.max_recoverable_reorg_depth,
                self.settings.index_spent_outputs,
            )?;
        }

        if !self.update.is_empty() {
            db.batch_update(self.update.clone(), self.settings.mempool)?;
            trace!("Flushed update: {}", self.update);
        }

        if !self.delete.is_empty() {
            db.batch_delete(self.delete.clone())?;
            trace!("Flushed delete: {}", self.delete);
        }

        // Clear the cache
        self.update = BatchUpdate::new(
            self.update.rune_count,
            self.update.block_count,
            self.update.purged_blocks_count,
        );
        self.delete = BatchDelete::new();
        self.first_block_height = self.update.block_count;
        self.last_block_height = None;

        Ok(())
    }

    fn prepare_to_delete(
        &mut self,
        max_recoverable_reorg_depth: u64,
        index_spent_outputs: bool,
    ) -> Result<()> {
        if let Some(last_block_height) = self.last_block_height {
            let mut from_block_height_to_purge = self
                .first_block_height
                .checked_sub(max_recoverable_reorg_depth + 1)
                .unwrap_or(0);

            let to_block_height_to_purge =
                last_block_height.checked_sub(max_recoverable_reorg_depth);

            if let Some(to_block_height_to_purge) = to_block_height_to_purge {
                from_block_height_to_purge = cmp::max(
                    from_block_height_to_purge,
                    self.update.purged_blocks_count + 1,
                );

                info!(
                    "Purging blocks from {} to {}",
                    from_block_height_to_purge, to_block_height_to_purge,
                );

                for i in from_block_height_to_purge..to_block_height_to_purge {
                    self.purge_block(i, index_spent_outputs)?;
                }
            }
        }

        Ok(())
    }

    fn purge_block(&mut self, height: u64, index_spent_outputs: bool) -> Result<()> {
        let block = self.get_block_by_height(height)?;

        for txid in block.tx_ids.iter() {
            let txid = Txid::from_str(txid)?;
            let tx_state_changes = self.get_tx_state_changes(txid)?;

            for txin in tx_state_changes.inputs.iter() {
                if !index_spent_outputs {
                    self.delete.tx_outs.insert(txin.clone());
                }

                self.delete.script_pubkeys_outpoints.insert(txin.clone());
            }

            self.delete.tx_state_changes.insert(txid);
        }

        self.update.purged_blocks_count = height;

        Ok(())
    }
}
