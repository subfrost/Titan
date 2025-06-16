use {
    crate::models::RuneEntry,
    bitcoin::{BlockHash, OutPoint, Transaction, Txid},
    clru::CLruCache,
    ordinals::RuneId,
    std::{num::NonZeroUsize, sync::Arc},
    titan_types::{Block, TxOutEntry},
};

/// Capacities for each segment of the read cache.
#[derive(Debug, Clone, Copy)]
pub struct ReadCacheSettings {
    pub txouts: usize,
    pub blocks: usize,
    pub block_hashes: usize,
    pub transactions: usize,
    pub runes: usize,
    pub rune_ids: usize,
}

impl Default for ReadCacheSettings {
    fn default() -> Self {
        Self {
            txouts: 100_000,
            blocks: 4_096,
            block_hashes: 4_096,
            transactions: 16_384,
            runes: 4_096,
            rune_ids: 4_096,
        }
    }
}

pub struct ReadCache {
    blocks: CLruCache<BlockHash, Arc<Block>>,
    block_hashes: CLruCache<u64, BlockHash>,
    txouts: CLruCache<OutPoint, TxOutEntry>,
    transactions: CLruCache<Txid, Arc<Transaction>>,
    runes: CLruCache<RuneId, RuneEntry>,
    rune_ids: CLruCache<u128, RuneId>, // key is raw rune value
}

impl ReadCache {
    pub fn with_settings(settings: ReadCacheSettings) -> Self {
        Self {
            blocks: CLruCache::new(
                NonZeroUsize::new(settings.blocks).unwrap_or_else(|| NonZeroUsize::new(1).unwrap()),
            ),
            block_hashes: CLruCache::new(
                NonZeroUsize::new(settings.block_hashes)
                    .unwrap_or_else(|| NonZeroUsize::new(1).unwrap()),
            ),
            txouts: CLruCache::new(
                NonZeroUsize::new(settings.txouts).unwrap_or_else(|| NonZeroUsize::new(1).unwrap()),
            ),
            transactions: CLruCache::new(
                NonZeroUsize::new(settings.transactions)
                    .unwrap_or_else(|| NonZeroUsize::new(1).unwrap()),
            ),
            runes: CLruCache::new(
                NonZeroUsize::new(settings.runes).unwrap_or_else(|| NonZeroUsize::new(1).unwrap()),
            ),
            rune_ids: CLruCache::new(
                NonZeroUsize::new(settings.rune_ids)
                    .unwrap_or_else(|| NonZeroUsize::new(1).unwrap()),
            ),
        }
    }

    /// Convenience constructor using default capacities.
    pub fn new() -> Self {
        Self::with_settings(ReadCacheSettings::default())
    }

    /* ----------------------------- TxOutEntry ----------------------------- */

    pub fn get_tx_out(&mut self, outpoint: &OutPoint) -> Option<TxOutEntry> {
        self.txouts.get(outpoint).cloned()
    }

    pub fn insert_tx_out(&mut self, outpoint: OutPoint, tx_out: TxOutEntry) {
        self.txouts.put(outpoint, tx_out);
    }

    pub fn contains_tx_out(&self, outpoint: &OutPoint) -> bool {
        self.txouts.contains(outpoint)
    }

    /* ------------------------------ Blocks -------------------------------- */

    pub fn get_block(&mut self, hash: &BlockHash) -> Option<Arc<Block>> {
        self.blocks.get(hash).cloned()
    }

    pub fn insert_block(&mut self, hash: BlockHash, block: Arc<Block>) {
        self.blocks.put(hash, block);
    }

    pub fn contains_block(&self, hash: &BlockHash) -> bool {
        self.blocks.contains(hash)
    }

    /* ---------------------------- Block Hashes --------------------------- */

    pub fn get_block_hash(&mut self, height: &u64) -> Option<BlockHash> {
        self.block_hashes.get(height).cloned()
    }

    pub fn insert_block_hash(&mut self, height: u64, hash: BlockHash) {
        self.block_hashes.put(height, hash);
    }

    /* --------------------------- Transactions ---------------------------- */

    pub fn get_transaction(&mut self, txid: &Txid) -> Option<Arc<Transaction>> {
        self.transactions.get(txid).cloned()
    }

    pub fn insert_transaction(&mut self, txid: Txid, tx: Arc<Transaction>) {
        self.transactions.put(txid, tx);
    }

    /* ------------------------------- Runes ------------------------------- */

    pub fn get_rune(&mut self, rune_id: &RuneId) -> Option<RuneEntry> {
        self.runes.get(rune_id).cloned()
    }

    pub fn insert_rune(&mut self, rune_id: RuneId, rune: RuneEntry) {
        self.runes.put(rune_id, rune);
    }

    /* ------------------------------ RuneIds ------------------------------ */

    pub fn get_rune_id(&mut self, raw: &u128) -> Option<RuneId> {
        self.rune_ids.get(raw).cloned()
    }

    pub fn insert_rune_id(&mut self, raw: u128, rune_id: RuneId) {
        self.rune_ids.put(raw, rune_id);
    }
}
