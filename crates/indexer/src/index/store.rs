use {
    crate::{
        db::{RocksDB, RocksDBError},
        models::{
            BatchDelete, BatchRollback, BatchUpdate, BlockId, Inscription, RuneEntry,
            TransactionStateChange,
            protorune::ProtoruneBalanceSheet,
        },
    },
    bitcoin::{consensus, hex::HexToArrayError, BlockHash, ScriptBuf},
    ordinals::{Rune, RuneId},
    rustc_hash::FxHashMap as HashMap,
    thiserror::Error,
    titan_types::{
        Block, InscriptionId, MempoolEntry, Pagination, PaginationResponse, SerializedOutPoint,
        SerializedTxid, SpenderReference, SpentStatus, Transaction, TransactionStatus, TxOut,
    },
};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("db error {0}")]
    DB(RocksDBError),
    #[error("hex array error {0}")]
    HexToArray(#[from] HexToArrayError),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("deserialize error {0}")]
    Deserialize(#[from] consensus::encode::Error),
}

impl StoreError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, StoreError::NotFound(_))
    }
}

impl From<RocksDBError> for StoreError {
    fn from(error: RocksDBError) -> Self {
        match error {
            RocksDBError::NotFound(msg) => StoreError::NotFound(msg),
            other => StoreError::DB(other),
        }
    }
}

pub trait Store {
    fn get(&self, cf: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError>;
    // settings
    fn is_index_addresses(&self) -> Result<Option<bool>, StoreError>;
    fn set_index_addresses(&self, value: bool) -> Result<(), StoreError>;
    fn is_index_bitcoin_transactions(&self) -> Result<Option<bool>, StoreError>;
    fn set_index_bitcoin_transactions(&self, value: bool) -> Result<(), StoreError>;
    fn is_index_spent_outputs(&self) -> Result<Option<bool>, StoreError>;
    fn set_index_spent_outputs(&self, value: bool) -> Result<(), StoreError>;

    // status
    fn get_is_at_tip(&self) -> Result<bool, StoreError>;
    fn set_is_at_tip(&self, value: bool) -> Result<(), StoreError>;

    // block
    fn get_block_count(&self) -> Result<u64, StoreError>;
    fn set_block_count(&self, count: u64) -> Result<(), StoreError>;
    fn get_purged_blocks_count(&self) -> Result<u64, StoreError>;

    fn get_block_hash(&self, height: u64) -> Result<BlockHash, StoreError>;
    fn get_block_hashes_by_height(
        &self,
        from_height: u64,
        to_height: u64,
    ) -> Result<Vec<BlockHash>, StoreError>;

    fn delete_block_hash(&self, height: u64) -> Result<(), StoreError>;

    fn get_block_by_hash(&self, hash: &BlockHash) -> Result<Block, StoreError>;
    fn get_blocks_by_hashes(
        &self,
        hashes: &Vec<BlockHash>,
    ) -> Result<HashMap<BlockHash, Block>, StoreError>;
    fn get_blocks_by_heights(
        &self,
        from_height: u64,
        to_height: u64,
    ) -> Result<HashMap<u64, Block>, StoreError>;

    fn delete_block(&self, hash: &BlockHash) -> Result<(), StoreError>;

    // mempool
    fn is_tx_in_mempool(&self, txid: &SerializedTxid) -> Result<bool, StoreError>;
    fn get_mempool_txids(&self) -> Result<HashMap<SerializedTxid, MempoolEntry>, StoreError>;
    fn get_mempool_entry(&self, txid: &SerializedTxid) -> Result<MempoolEntry, StoreError>;
    fn get_mempool_entries(
        &self,
        txids: &[SerializedTxid],
    ) -> Result<HashMap<SerializedTxid, Option<MempoolEntry>>, StoreError>;
    fn get_mempool_entries_with_ancestors(
        &self,
        txids: &[SerializedTxid],
    ) -> Result<HashMap<SerializedTxid, MempoolEntry>, StoreError>;

    // outpoint
    fn get_tx_out(
        &self,
        outpoint: &SerializedOutPoint,
        mempool: Option<bool>,
    ) -> Result<TxOut, StoreError>;
    fn get_all_tx_outs(
        &self,
        mempool: bool,
    ) -> Result<HashMap<SerializedOutPoint, TxOut>, StoreError>;
    fn get_tx_out_with_mempool_spent_update(
        &self,
        outpoint: &SerializedOutPoint,
        mempool: Option<bool>,
    ) -> Result<TxOut, StoreError>;
    fn get_tx_outs(
        &self,
        outpoints: &[SerializedOutPoint],
        mempool: Option<bool>,
    ) -> Result<HashMap<SerializedOutPoint, TxOut>, StoreError>;
    fn get_tx_outs_with_mempool_spent_update(
        &self,
        outpoints: &[SerializedOutPoint],
        mempool: Option<bool>,
    ) -> Result<HashMap<SerializedOutPoint, TxOut>, StoreError>;

    // transaction changes
    fn get_tx_state_changes(
        &self,
        txid: &SerializedTxid,
        mempool: Option<bool>,
    ) -> Result<TransactionStateChange, StoreError>;
    fn get_txs_state_changes(
        &self,
        txids: &[SerializedTxid],
        mempool: bool,
    ) -> Result<HashMap<SerializedTxid, TransactionStateChange>, StoreError>;

    // bitcoin transactions
    fn get_transaction_raw(
        &self,
        txid: &SerializedTxid,
        mempool: Option<bool>,
    ) -> Result<Vec<u8>, StoreError>;
    fn get_transaction(
        &self,
        txid: &SerializedTxid,
        mempool: Option<bool>,
    ) -> Result<Transaction, StoreError>;
    fn get_transaction_confirming_block(
        &self,
        txid: &SerializedTxid,
    ) -> Result<BlockId, StoreError>;
    fn get_transaction_confirming_blocks(
        &self,
        txids: &[SerializedTxid],
    ) -> Result<HashMap<SerializedTxid, Option<BlockId>>, StoreError>;
    fn get_inputs_outputs_from_transaction(
        &self,
        transaction: &bitcoin::Transaction,
        txid: &SerializedTxid,
    ) -> Result<(Vec<Option<TxOut>>, Vec<Option<TxOut>>), StoreError>;
    fn partition_transactions_by_existence(
        &self,
        txids: &Vec<SerializedTxid>,
    ) -> Result<(Vec<SerializedTxid>, Vec<SerializedTxid>), StoreError>;

    // rune transactions
    fn get_last_rune_transactions(
        &self,
        rune_id: &RuneId,
        pagination: Option<Pagination>,
        mempool: Option<bool>,
    ) -> Result<PaginationResponse<SerializedTxid>, StoreError>;

    // runes
    fn get_runes_count(&self) -> Result<u64, StoreError>;
    fn get_rune(&self, rune_id: &RuneId) -> Result<RuneEntry, StoreError>;
    fn get_rune_id(&self, rune: &Rune) -> Result<RuneId, StoreError>;
    fn get_runes_by_ids(
        &self,
        rune_ids: &Vec<RuneId>,
    ) -> Result<HashMap<RuneId, RuneEntry>, StoreError>;
    fn get_runes(
        &self,
        pagination: Pagination,
    ) -> Result<PaginationResponse<(RuneId, RuneEntry)>, StoreError>;

    // inscription
    fn get_inscription(&self, inscription_id: &InscriptionId) -> Result<Inscription, StoreError>;

    // protorune
    fn get_protorune_balance_sheet(
        &self,
        outpoint: &SerializedOutPoint,
    ) -> Result<ProtoruneBalanceSheet, StoreError>;

    // address
    fn get_script_pubkey_outpoints(
        &self,
        script_pubkey: &ScriptBuf,
        mempool: Option<bool>,
    ) -> Result<Vec<SerializedOutPoint>, StoreError>;
    fn get_outpoints_to_script_pubkey(
        &self,
        outpoints: &[SerializedOutPoint],
        mempool: Option<bool>,
        optimistic: bool,
    ) -> Result<HashMap<SerializedOutPoint, ScriptBuf>, StoreError>;

    // batch
    fn batch_update(&self, update: &BatchUpdate, mempool: bool) -> Result<(), StoreError>;
    fn batch_delete(&self, delete: &BatchDelete) -> Result<(), StoreError>;
    fn batch_rollback(&self, rollback: &BatchRollback, mempool: bool) -> Result<(), StoreError>;

    /// Called once the indexer reaches tip so the underlying database can
    /// switch from bulk-load settings to normal online mode. Default
    /// implementation is a no-op.
    fn finish_bulk_load(&self) -> Result<(), StoreError>;

    fn get_rune_id_by_number(&self, number: u64) -> Result<RuneId, StoreError>;
    fn get_rune_ids_by_numbers(&self, numbers: &Vec<u64>) -> Result<HashMap<u64, RuneId>, StoreError>;
    fn add_rune_transactions_batch(
        &self,
        rune_tx_map: &HashMap<RuneId, Vec<SerializedTxid>>,
        mempool: bool,
    ) -> Result<(), StoreError>;
    fn delete_rune_transactions(
        &self,
        txids: &[SerializedTxid],
        mempool: bool,
    ) -> Result<(), StoreError>;
    fn get_spent_outpoints_in_mempool(
        &self,
        outpoints: &[SerializedOutPoint],
    ) -> Result<HashMap<SerializedOutPoint, Option<SpenderReference>>, StoreError>;
    fn set_subscription(&self, sub: &titan_types::Subscription) -> Result<(), StoreError>;
    fn get_subscription(&self, id: &uuid::Uuid) -> Result<titan_types::Subscription, StoreError>;
    fn get_subscriptions(&self) -> Result<Vec<titan_types::Subscription>, StoreError>;
    fn delete_subscription(&self, id: &uuid::Uuid) -> Result<(), StoreError>;
    fn update_subscription_last_success(
        &self,
        subscription_id: &uuid::Uuid,
        new_time_secs: u64,
    ) -> Result<(), StoreError>;
    fn flush(&self) -> Result<(), StoreError>;
    fn close(self) -> Result<(), StoreError>;
}

