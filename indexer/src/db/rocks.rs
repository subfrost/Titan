use {
    super::{
        entry::Entry,
        util::{
            parse_outpoint_from_script_pubkey_key, rune_id_from_bytes, rune_index_key,
            rune_transaction_key, script_pubkey_outpoint_to_bytes, script_pubkey_search_key,
        },
        *,
    },
    crate::models::{
        BatchDelete, BatchRollback, BatchUpdate, BlockId, Inscription, RuneEntry,
        TransactionStateChange, TxRuneIndexRef,
    },
    bitcoin::{consensus, hashes::Hash, BlockHash, OutPoint, ScriptBuf, Transaction, Txid},
    borsh::BorshDeserialize,
    mapper::DBResultMapper,
    ordinals::RuneId,
    rocksdb::{
        BlockBasedOptions, BoundColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode, Direction,
        IteratorMode, MultiThreaded, Options, WriteBatch,
    },
    std::{
        collections::{HashMap, HashSet},
        sync::{Arc, RwLock},
    },
    titan_types::{Block, InscriptionId, Pagination, PaginationResponse, Subscription, TxOutEntry},
    util::{
        inscription_id_to_bytes, outpoint_to_bytes, rune_id_to_bytes, txid_from_bytes,
        txid_to_bytes,
    },
    uuid::Uuid,
    wrapper::RuneIdWrapper,
};

pub struct RocksDB {
    db: DBWithThreadMode<MultiThreaded>,
    mempool_cache: RwLock<HashSet<Txid>>,
}

pub type DBResult<T> = Result<T, RocksDBError>;

const BLOCKS_CF: &str = "blocks";
const BLOCK_HEIGHT_TO_HASH_CF: &str = "block_height_to_hash";

const OUTPOINTS_CF: &str = "outpoints";
const OUTPOINTS_MEMPOOL_CF: &str = "mempool_outpoints";

const TRANSACTIONS_STATE_CHANGE_CF: &str = "transactions_state_change";
const TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF: &str = "mempool_transactions_state_change";

const RUNE_TRANSACTIONS_CF: &str = "rune_transactions";
const RUNE_TRANSACTIONS_MEMPOOL_CF: &str = "rune_transactions_mempool";

const TRANSACTION_RUNE_INDEX_CF: &str = "transaction_rune_index";
const TRANSACTION_RUNE_INDEX_MEMPOOL_CF: &str = "transaction_rune_index_mempool";

const RUNES_COUNT_KEY: &str = "runes_count";
const RUNES_CF: &str = "runes";
const RUNE_IDS_CF: &str = "rune_ids";
const RUNE_NUMBER_CF: &str = "rune_number";

const INSCRIPTIONS_CF: &str = "inscriptions";

const SCRIPT_PUBKEYS_CF: &str = "script_pubkeys";
const SCRIPT_PUBKEYS_MEMPOOL_CF: &str = "script_pubkeys_mempool";

const OUTPOINT_TO_SCRIPT_PUBKEY_CF: &str = "outpoint_to_script_pubkey";
const OUTPOINT_TO_SCRIPT_PUBKEY_MEMPOOL_CF: &str = "outpoint_to_script_pubkey_mempool";
const SPENT_OUTPOINTS_MEMPOOL_CF: &str = "spent_outpoints_mempool";

const TRANSACTIONS_CF: &str = "transactions";
const TRANSACTIONS_MEMPOOL_CF: &str = "transactions_mempool";
const TRANSACTION_CONFIRMING_BLOCK_CF: &str = "transaction_confirming_block";

const MEMPOOL_CF: &str = "mempool";
const STATS_CF: &str = "stats";

const SETTINGS_CF: &str = "settings";

const SUBSCRIPTIONS_CF: &str = "subscriptions";

const INDEX_SPENT_OUTPUTS_KEY: &str = "index_spent_outputs";
const INDEX_ADDRESSES_KEY: &str = "index_addresses";
const INDEX_BITCOIN_TRANSACTIONS_KEY: &str = "index_bitcoin_transactions";

const BLOCK_COUNT_KEY: &str = "block_count";
const PURGED_BLOCKS_COUNT_KEY: &str = "purged_blocks_count";

impl RocksDB {
    pub fn open(file_path: &str) -> DBResult<Self> {
        // Create descriptors
        let mut cf_opts = Options::default();
        cf_opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        let blocks_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(BLOCKS_CF, cf_opts.clone());
        let block_height_to_hash_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(BLOCK_HEIGHT_TO_HASH_CF, cf_opts.clone());
        let outpoints_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(OUTPOINTS_CF, cf_opts.clone());
        let outpoints_mempool_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(OUTPOINTS_MEMPOOL_CF, cf_opts.clone());
        let transaction_state_change_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(TRANSACTIONS_STATE_CHANGE_CF, cf_opts.clone());
        let transaction_state_change_mempool_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF, cf_opts.clone());
        let runes_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(RUNES_CF, cf_opts.clone());
        let rune_ids_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(RUNE_IDS_CF, cf_opts.clone());
        let rune_number_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(RUNE_NUMBER_CF, cf_opts.clone());
        let inscriptions_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(INSCRIPTIONS_CF, cf_opts.clone());
        let mempool_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(MEMPOOL_CF, cf_opts.clone());
        let stats_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(STATS_CF, cf_opts.clone());
        let rune_transactions_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(RUNE_TRANSACTIONS_CF, cf_opts.clone());
        let rune_transactions_mempool_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(RUNE_TRANSACTIONS_MEMPOOL_CF, cf_opts.clone());
        let transaction_rune_index_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(TRANSACTION_RUNE_INDEX_CF, cf_opts.clone());
        let transaction_rune_index_mempool_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(TRANSACTION_RUNE_INDEX_MEMPOOL_CF, cf_opts.clone());
        let script_pubkeys_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(SCRIPT_PUBKEYS_CF, cf_opts.clone());
        let script_pubkeys_mempool_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(SCRIPT_PUBKEYS_MEMPOOL_CF, cf_opts.clone());
        let outpoint_to_script_pubkey_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(OUTPOINT_TO_SCRIPT_PUBKEY_CF, cf_opts.clone());
        let outpoint_to_script_pubkey_mempool_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(OUTPOINT_TO_SCRIPT_PUBKEY_MEMPOOL_CF, cf_opts.clone());
        let spent_outpoints_mempool_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(SPENT_OUTPOINTS_MEMPOOL_CF, cf_opts.clone());
        let transactions_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(TRANSACTIONS_CF, cf_opts.clone());
        let transactions_mempool_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(TRANSACTIONS_MEMPOOL_CF, cf_opts.clone());
        let transaction_confirming_block_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(TRANSACTION_CONFIRMING_BLOCK_CF, cf_opts.clone());
        let settings_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(SETTINGS_CF, cf_opts.clone());
        let subscriptions_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(SUBSCRIPTIONS_CF, cf_opts.clone());

        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        db_opts.set_max_log_file_size(64 * 1024 * 1024); // 64 MB
        db_opts.set_keep_log_file_num(10);
        db_opts.set_recycle_log_file_num(5);
        db_opts.set_compression_type(rocksdb::DBCompressionType::Lz4);
        db_opts.set_bottommost_compression_type(rocksdb::DBCompressionType::Zstd);
        db_opts.set_periodic_compaction_seconds(86400); // Run compaction every 24 hours
        db_opts.set_write_buffer_size(64 * 1024 * 1024); // 64 MB

        let mut block_based_options = BlockBasedOptions::default();
        block_based_options.set_block_size(16 * 1024); // 16 KB
        block_based_options.set_cache_index_and_filter_blocks(true);
        block_based_options.set_pin_l0_filter_and_index_blocks_in_cache(true);
        db_opts.set_block_based_table_factory(&block_based_options);

        let descriptors = DBWithThreadMode::<MultiThreaded>::open_cf_descriptors(
            &db_opts,
            file_path,
            vec![
                blocks_cfd,
                block_height_to_hash_cfd,
                outpoints_cfd,
                outpoints_mempool_cfd,
                transaction_state_change_cfd,
                transaction_state_change_mempool_cfd,
                runes_cfd,
                rune_ids_cfd,
                rune_number_cfd,
                inscriptions_cfd,
                mempool_cfd,
                stats_cfd,
                rune_transactions_cfd,
                rune_transactions_mempool_cfd,
                transaction_rune_index_cfd,
                transaction_rune_index_mempool_cfd,
                script_pubkeys_cfd,
                script_pubkeys_mempool_cfd,
                outpoint_to_script_pubkey_cfd,
                outpoint_to_script_pubkey_mempool_cfd,
                spent_outpoints_mempool_cfd,
                transactions_cfd,
                transactions_mempool_cfd,
                transaction_confirming_block_cfd,
                settings_cfd,
                subscriptions_cfd,
            ],
        )?;

        // Load initial state from DB
        let mempool_cache = Self::read_all_mempool_txids(&descriptors)?;

        let rocks_db = RocksDB {
            db: descriptors,
            mempool_cache: RwLock::new(mempool_cache),
        };
        Ok(rocks_db)
    }

    fn cf_handle(&self, name: &str) -> DBResult<Arc<BoundColumnFamily>> {
        match self.db.cf_handle(name) {
            None => Err(RocksDBError::InvalidHandle(name.to_string())),
            Some(handle) => Ok(handle),
        }
    }

    fn get_option_vec_data<K: AsRef<[u8]>>(
        &self,
        cf_handle: &Arc<BoundColumnFamily>,
        key: K,
    ) -> DBResult<Option<Vec<u8>>> {
        match self.db.get_cf(cf_handle, key) {
            Ok(val) => Ok(val),
            Err(e) => Err(e)?,
        }
    }

    pub fn is_index_spent_outputs(&self) -> DBResult<Option<bool>> {
        let cf_handle = self.cf_handle(SETTINGS_CF)?;
        let val: Option<u64> = self
            .get_option_vec_data(&cf_handle, INDEX_SPENT_OUTPUTS_KEY)
            .mapped()?;

        Ok(val.map(|v| v == 1))
    }

    pub fn set_index_spent_outputs(&self, value: bool) -> DBResult<()> {
        let cf_handle = self.cf_handle(SETTINGS_CF)?;
        self.db.put_cf(
            &cf_handle,
            INDEX_SPENT_OUTPUTS_KEY,
            (value as u64).to_le_bytes().to_vec(),
        )?;
        Ok(())
    }

    pub fn is_index_addresses(&self) -> DBResult<Option<bool>> {
        let cf_handle = self.cf_handle(SETTINGS_CF)?;
        let val: Option<u64> = self
            .get_option_vec_data(&cf_handle, INDEX_ADDRESSES_KEY)
            .mapped()?;

        Ok(val.map(|v| v == 1))
    }

    pub fn set_index_addresses(&self, value: bool) -> DBResult<()> {
        let cf_handle = self.cf_handle(SETTINGS_CF)?;
        self.db.put_cf(
            &cf_handle,
            INDEX_ADDRESSES_KEY,
            (value as u64).to_le_bytes().to_vec(),
        )?;
        Ok(())
    }

    pub fn is_index_bitcoin_transactions(&self) -> DBResult<Option<bool>> {
        let cf_handle = self.cf_handle(SETTINGS_CF)?;
        let val: Option<u64> = self
            .get_option_vec_data(&cf_handle, INDEX_BITCOIN_TRANSACTIONS_KEY)
            .mapped()?;

        Ok(val.map(|v| v == 1))
    }

    pub fn set_index_bitcoin_transactions(&self, value: bool) -> DBResult<()> {
        let cf_handle = self.cf_handle(SETTINGS_CF)?;
        self.db.put_cf(
            &cf_handle,
            INDEX_BITCOIN_TRANSACTIONS_KEY,
            (value as u64).to_le_bytes().to_vec(),
        )?;
        Ok(())
    }

    pub fn get_block_count(&self) -> DBResult<u64> {
        let cf_handle = self.cf_handle(STATS_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, BLOCK_COUNT_KEY)
            .mapped()?
            .unwrap_or(0))
    }

    pub fn set_block_count(&self, count: u64) -> DBResult<()> {
        let cf_handle = self.cf_handle(STATS_CF)?;
        self.db
            .put_cf(&cf_handle, BLOCK_COUNT_KEY, count.to_le_bytes().to_vec())?;
        Ok(())
    }

    pub fn get_purged_blocks_count(&self) -> DBResult<u64> {
        let cf_handle = self.cf_handle(STATS_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, PURGED_BLOCKS_COUNT_KEY)
            .mapped()?
            .unwrap_or(0))
    }

    pub fn get_block_hash(&self, height: u64) -> DBResult<BlockHash> {
        let cf_handle = self.cf_handle(BLOCK_HEIGHT_TO_HASH_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, height.to_le_bytes())
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "block hash not found: {}",
                height
            )))?)
    }

    pub fn delete_block_hash(&self, height: u64) -> DBResult<()> {
        let cf_handle = self.cf_handle(BLOCK_HEIGHT_TO_HASH_CF)?;
        self.db.delete_cf(&cf_handle, height.to_le_bytes())?;
        Ok(())
    }

    pub fn get_block_by_hash(&self, hash: &BlockHash) -> DBResult<Block> {
        let cf_handle = self.cf_handle(BLOCKS_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, hash.as_raw_hash().to_byte_array())
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!("block not found: {}", hash)))?)
    }

    pub fn delete_block(&self, hash: &BlockHash) -> DBResult<()> {
        let cf_handle = self.cf_handle(BLOCKS_CF)?;
        self.db
            .delete_cf(&cf_handle, hash.as_raw_hash().to_byte_array())?;
        Ok(())
    }

    pub fn get_rune(&self, rune_id: &RuneId) -> DBResult<RuneEntry> {
        let cf_handle = self.cf_handle(RUNES_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, rune_id_to_bytes(rune_id))
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "rune not found: {}",
                rune_id
            )))?)
    }

    pub fn get_runes_by_ids(&self, rune_ids: &Vec<RuneId>) -> DBResult<HashMap<RuneId, RuneEntry>> {
        let cf_handle = self.cf_handle(RUNES_CF)?;
        let keys: Vec<_> = rune_ids
            .iter()
            .map(|id| (&cf_handle, rune_id_to_bytes(id)))
            .collect();

        let values = self.db.multi_get_cf(keys);

        let mut result = HashMap::new();
        for (i, value) in values.iter().enumerate() {
            if let Ok(Some(value)) = value {
                result.insert(rune_ids[i], RuneEntry::load(value.clone()));
            }
        }

        Ok(result)
    }

    pub fn get_rune_id_by_number(&self, number: u64) -> DBResult<RuneId> {
        let cf_handle = self.cf_handle(RUNE_NUMBER_CF)?;
        let rune_id_wrapper: RuneIdWrapper = self
            .get_option_vec_data(&cf_handle, number.to_le_bytes())
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "rune id not found: {}",
                number
            )))?;

        Ok(rune_id_wrapper.0)
    }

    pub fn get_rune_ids_by_numbers(&self, numbers: &Vec<u64>) -> DBResult<HashMap<u64, RuneId>> {
        let cf_handle = self.cf_handle(RUNE_NUMBER_CF)?;
        let keys: Vec<_> = numbers
            .iter()
            .map(|n| (&cf_handle, n.to_le_bytes()))
            .collect();

        let values = self.db.multi_get_cf(keys);

        let mut result = HashMap::new();
        for (i, value) in values.iter().enumerate() {
            if let Ok(Some(value)) = value {
                result.insert(numbers[i], RuneIdWrapper::load(value.clone()).0);
            }
        }

        Ok(result)
    }

    pub fn get_tx_out(&self, outpoint: &OutPoint, mempool: bool) -> DBResult<TxOutEntry> {
        let cf_handle = if mempool {
            self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
        } else {
            self.cf_handle(OUTPOINTS_CF)?
        };

        Ok(self
            .get_option_vec_data(&cf_handle, outpoint_to_bytes(outpoint))
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "outpoint not found: {}",
                outpoint
            )))?)
    }

    pub fn get_tx_outs(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: bool,
    ) -> DBResult<HashMap<OutPoint, TxOutEntry>> {
        let cf_handle = if mempool {
            self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
        } else {
            self.cf_handle(OUTPOINTS_CF)?
        };

        let keys: Vec<_> = outpoints
            .iter()
            .map(|o| (&cf_handle, outpoint_to_bytes(o)))
            .collect();

        let values = self.db.multi_get_cf(keys);

        let mut result = HashMap::new();
        for (i, value) in values.iter().enumerate() {
            if let Ok(Some(value)) = value {
                result.insert(outpoints[i], TxOutEntry::load(value.clone()));
            }
        }

        Ok(result)
    }

    pub fn get_tx_state_changes(
        &self,
        tx_id: &Txid,
        mempool: bool,
    ) -> DBResult<TransactionStateChange> {
        let cf_handle = if mempool {
            self.cf_handle(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTIONS_STATE_CHANGE_CF)?
        };

        Ok(self
            .get_option_vec_data(&cf_handle, txid_to_bytes(tx_id))
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "transaction state change not found: {}",
                tx_id
            )))?)
    }

    pub fn get_txs_state_changes(
        &self,
        txids: &Vec<Txid>,
        mempool: bool,
    ) -> DBResult<HashMap<Txid, TransactionStateChange>> {
        let cf_handle = if mempool {
            self.cf_handle(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTIONS_STATE_CHANGE_CF)?
        };

        let keys: Vec<_> = txids
            .iter()
            .map(|id| (&cf_handle, txid_to_bytes(id)))
            .collect();

        let values = self.db.multi_get_cf(keys);

        let mut result = HashMap::new();
        for (i, value) in values.iter().enumerate() {
            if let Ok(Some(value)) = value {
                result.insert(txids[i], TransactionStateChange::load(value.clone()));
            }
        }

        Ok(result)
    }

    pub fn get_runes_count(&self) -> DBResult<u64> {
        let cf_handle = self.cf_handle(STATS_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, RUNES_COUNT_KEY)
            .mapped()?
            .unwrap_or(0))
    }

    pub fn get_rune_id(&self, rune: &u128) -> DBResult<RuneId> {
        let cf_handle = self.cf_handle(RUNE_IDS_CF)?;
        let rune_id_wrapper: RuneIdWrapper = self
            .get_option_vec_data(&cf_handle, rune.to_le_bytes())
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "rune id not found: {}",
                rune
            )))?;

        Ok(rune_id_wrapper.0)
    }

    pub fn get_inscription(&self, id: &InscriptionId) -> DBResult<Inscription> {
        let cf_handle = self.cf_handle(INSCRIPTIONS_CF)?;
        let inscription: Inscription = self
            .get_option_vec_data(&cf_handle, inscription_id_to_bytes(id))
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "inscription not found: {}",
                id
            )))?;

        Ok(inscription)
    }

    pub fn get_last_rune_transactions(
        &self,
        rune_id: &RuneId,
        pagination: Option<Pagination>,
        mempool: bool,
    ) -> DBResult<PaginationResponse<Txid>> {
        let cf_handle = if mempool {
            self.cf_handle(RUNE_TRANSACTIONS_MEMPOOL_CF)?
        } else {
            self.cf_handle(RUNE_TRANSACTIONS_CF)?
        };

        // 1. Get last_index
        let last_index_key = rune_index_key(rune_id);
        let last_index: u64 = self
            .get_option_vec_data(&cf_handle, &last_index_key)
            .mapped()?
            .unwrap_or(0);

        if last_index == 0 {
            // No entries
            return Ok(PaginationResponse {
                items: vec![],
                offset: 0,
            });
        }

        let (skip, limit) = pagination.unwrap_or_default().into();

        // 2. Compute the relevant range in [start_index ..= end_index], descending
        let end_index = last_index.saturating_sub(skip);
        if end_index == 0 {
            // skip is too large, no items
            let offset = skip - last_index;
            return Ok(PaginationResponse {
                items: vec![],
                offset,
            });
        }

        let start_index = end_index.saturating_sub(limit - 1);
        // If limit > end_index, start_index could go below 1, so clamp to 1 for safety
        // but actually we can do 0 for convenience, because we won't find negative keys
        // anyway.
        // We'll do it cleanly:
        let start_index = if start_index < 1 { 1 } else { start_index };

        let prefix_bytes = {
            let rune_id_bytes = rune_id_to_bytes(rune_id);
            // "rune:<rune_id>:"
            let mut p = Vec::with_capacity(5 + rune_id_bytes.len() + 1);
            p.extend_from_slice(b"rune:");
            p.extend_from_slice(&rune_id_bytes);
            p.push(b':');
            p
        };

        // 3. Construct the 'seek_key' for end_index
        let seek_key = rune_transaction_key(rune_id, end_index);

        // 4. Reverse iterate
        let mut iter = self.db.iterator_cf(
            &cf_handle,
            IteratorMode::From(&seek_key, Direction::Reverse),
        );

        let mut results = Vec::new();

        while let Some(Ok((key_bytes, value_bytes))) = iter.next() {
            // Check prefix: if key_bytes doesn't start with prefix_bytes, break
            if !key_bytes.starts_with(&prefix_bytes) {
                break;
            }

            // Extract the last 8 bytes for the big-endian index
            if key_bytes.len() < prefix_bytes.len() + 8 {
                // Malformed key?
                continue;
            }
            let idx_bytes = &key_bytes[key_bytes.len() - 8..];
            let idx = u64::from_le_bytes(idx_bytes.try_into().unwrap());

            if idx < start_index {
                // We’ve gone past the range
                break;
            }
            if idx > end_index {
                // Shouldn't happen if our iteration is correct, but skip if so
                continue;
            }

            // Convert valuecf to Txid
            let txid: Txid =
                txid_from_bytes(&value_bytes).map_err(|_| RocksDBError::InvalidTxid)?;

            results.push(txid);

            // Stop if we have limit items
            if results.len() as u64 >= limit {
                break;
            }
        }

        let offset = skip + results.len() as u64;

        Ok(PaginationResponse {
            items: results,
            offset,
        })
    }

    /// Batch-add multiple rune transactions.
    ///
    /// # Arguments
    /// * `rune_tx_map` - A map where the key is a `RuneId` and the value is a list of `Txid`
    ///   to be added for that rune.
    /// * `mempool` - Boolean flag for using mempool column families.
    ///
    /// This method will replace the secondary index for each txid. So be sure that all the rune
    /// transactions are included for the txid.
    ///
    /// This function groups the transactions by rune, reads the current last index once per rune,
    /// and then updates both the primary rune transactions and the secondary tx-index in one batch.
    pub fn add_rune_transactions_batch(
        &self,
        rune_tx_map: &HashMap<RuneId, Vec<Txid>>,
        mempool: bool,
    ) -> DBResult<()> {
        // Get the appropriate column families for primary and secondary indexes.
        let primary_cf = if mempool {
            self.cf_handle(RUNE_TRANSACTIONS_MEMPOOL_CF)?
        } else {
            self.cf_handle(RUNE_TRANSACTIONS_CF)?
        };

        let secondary_cf = if mempool {
            self.cf_handle(TRANSACTION_RUNE_INDEX_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTION_RUNE_INDEX_CF)?
        };

        // Write batch to accumulate all changes.
        let mut batch = WriteBatch::default();

        // Accumulator for secondary index updates:
        // For each txid, we collect the new TxRuneIndexRef entries.
        let mut sec_index_acc: HashMap<Txid, Vec<TxRuneIndexRef>> = HashMap::new();

        // Process each rune and its list of txids.
        for (rune_id, txids) in rune_tx_map {
            // Compute the key that holds the current last index for this rune.
            let last_index_key = rune_index_key(rune_id);

            // Read the current last index from the primary CF, defaulting to 0.
            let last_index = match self.db.get_cf(&primary_cf, &last_index_key)? {
                Some(bytes) if bytes.len() >= 8 => {
                    u64::from_le_bytes(bytes[..8].try_into().expect("Expected 8 bytes"))
                }
                _ => 0,
            };

            let mut current_index = last_index;

            // Process each txid for this rune.
            for txid in txids {
                current_index = current_index
                    .checked_add(1)
                    .ok_or_else(|| RocksDBError::Overflow)?;

                // Create the primary key: "rune:<rune_id>:<new_index_in_le>"
                let tx_key = rune_transaction_key(rune_id, current_index);
                batch.put_cf(&primary_cf, tx_key, txid_to_bytes(txid));

                // Instead of fetching the existing secondary index, simply accumulate the new index.
                sec_index_acc
                    .entry(txid.clone())
                    .or_default()
                    .push(TxRuneIndexRef {
                        rune_id: rune_id_to_bytes(rune_id),
                        index: current_index,
                    });
            }

            // Update the last index for this rune.
            batch.put_cf(&primary_cf, last_index_key, current_index.to_le_bytes());
        }

        // Now update the secondary index for each txid in one go.
        for (txid, new_refs) in sec_index_acc.into_iter() {
            batch.put_cf(&secondary_cf, txid_to_bytes(&txid), new_refs.store());
        }

        // Write all batched operations atomically.
        self.db.write(batch)?;
        Ok(())
    }

    fn get_txs_index_refs(
        &self,
        txids: &Vec<Txid>,
        mempool: bool,
    ) -> DBResult<HashMap<Txid, Vec<TxRuneIndexRef>>> {
        let cf_handle = if mempool {
            self.cf_handle(TRANSACTION_RUNE_INDEX_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTION_RUNE_INDEX_CF)?
        };

        let keys: Vec<_> = txids
            .iter()
            .map(|txid| (&cf_handle, txid_to_bytes(txid)))
            .collect();

        let values = self.db.multi_get_cf(keys);

        let mut result = HashMap::new();
        for (i, value) in values.iter().enumerate() {
            if let Ok(Some(value)) = value {
                result.insert(
                    txids[i],
                    BorshDeserialize::deserialize(&mut &value[..]).unwrap(),
                );
            }
        }

        Ok(result)
    }

    /// Remove `txid` from *all* rune lists
    pub fn delete_rune_transactions(&self, txids: &Vec<Txid>, mempool: bool) -> DBResult<()> {
        let idx_refs = self.get_txs_index_refs(txids, mempool)?;

        if idx_refs.is_empty() {
            // Nothing to remove
            return Ok(());
        }

        let primary_cf = if mempool {
            self.cf_handle(RUNE_TRANSACTIONS_MEMPOOL_CF)?
        } else {
            self.cf_handle(RUNE_TRANSACTIONS_CF)?
        };

        let secondary_cf = if mempool {
            self.cf_handle(TRANSACTION_RUNE_INDEX_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTION_RUNE_INDEX_CF)?
        };

        let mut batch = WriteBatch::default();

        for (txid, idx_refs) in idx_refs {
            for TxRuneIndexRef { rune_id, index } in &idx_refs {
                let key = rune_transaction_key(
                    &rune_id_from_bytes(rune_id).map_err(|_| RocksDBError::InvalidRuneId)?,
                    *index,
                );
                batch.delete_cf(&primary_cf, key);
            }

            batch.delete_cf(&secondary_cf, txid_to_bytes(&txid));
        }

        self.db.write(batch)?;
        Ok(())
    }

    pub fn get_mempool_txids(&self) -> DBResult<HashSet<Txid>> {
        // O(1) read from cache
        Ok(self
            .mempool_cache
            .read()
            .map_err(|_| RocksDBError::LockPoisoned)?
            .clone())
    }

    pub fn is_tx_in_mempool(&self, txid: &Txid) -> DBResult<bool> {
        let exists = self
            .mempool_cache
            .read()
            .map_err(|_| RocksDBError::LockPoisoned)?
            .contains(txid);

        Ok(exists)
    }

    pub fn _validate_mempool_cache(&self) -> DBResult<()> {
        let db_txids: HashSet<Txid> = Self::read_all_mempool_txids(&self.db)?;

        *self
            .mempool_cache
            .write()
            .map_err(|_| RocksDBError::LockPoisoned)? = db_txids;
        Ok(())
    }

    fn read_all_mempool_txids(db: &DBWithThreadMode<MultiThreaded>) -> DBResult<HashSet<Txid>> {
        let mut db_txids: HashSet<Txid> = HashSet::with_capacity(300_000); // Pre-allocate for ~300k txs

        let cf_handle: Arc<BoundColumnFamily<'_>> = db
            .cf_handle(MEMPOOL_CF)
            .ok_or(RocksDBError::InvalidHandle(MEMPOOL_CF.to_string()))?;

        let iter = db.iterator_cf(&cf_handle, IteratorMode::Start);
        for item in iter {
            let (key, _) = item?;
            if let Ok(txid) = consensus::deserialize(&key) {
                db_txids.insert(txid);
            }
        }

        Ok(db_txids)
    }

    pub fn get_script_pubkey_outpoints(
        &self,
        script_pubkey: &ScriptBuf,
        mempool: bool,
    ) -> DBResult<Vec<OutPoint>> {
        let cf_handle = if mempool {
            self.cf_handle(SCRIPT_PUBKEYS_MEMPOOL_CF)?
        } else {
            self.cf_handle(SCRIPT_PUBKEYS_CF)?
        };

        let search_key = script_pubkey_search_key(script_pubkey);
        let iter = self.db.iterator_cf(
            &cf_handle,
            IteratorMode::From(&search_key, Direction::Forward),
        );

        let mut outpoints = Vec::new();
        for item in iter {
            let (key, _) = item?;
            if !key.starts_with(&search_key) {
                break;
            }

            outpoints.push(
                parse_outpoint_from_script_pubkey_key(&key)
                    .map_err(|_| RocksDBError::InvalidOutpoint)?,
            );
        }

        Ok(outpoints)
    }

    pub fn get_outpoints_to_script_pubkey(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: bool,
        optimistic: bool,
    ) -> DBResult<HashMap<OutPoint, ScriptBuf>> {
        let cf_handle = if mempool {
            self.cf_handle(OUTPOINT_TO_SCRIPT_PUBKEY_MEMPOOL_CF)?
        } else {
            self.cf_handle(OUTPOINT_TO_SCRIPT_PUBKEY_CF)?
        };

        let keys: Vec<_> = outpoints
            .iter()
            .map(|outpoint| (&cf_handle, outpoint_to_bytes(outpoint)))
            .collect();

        let results = self.db.multi_get_cf(keys);

        // Process results and collect into HashMap
        let mut script_pubkeys = HashMap::with_capacity(results.len());

        for (i, result) in results.iter().enumerate() {
            match result {
                Ok(Some(bytes)) => {
                    script_pubkeys.insert(outpoints[i].clone(), ScriptBuf::from(bytes.clone()));
                }
                Ok(None) => {
                    if optimistic {
                        continue;
                    }

                    return Err(RocksDBError::NotFound(format!(
                        "outpoint to script pubkey not found: {}",
                        outpoints[i]
                    )));
                }
                Err(e) => return Err(e.clone().into()),
            }
        }

        Ok(script_pubkeys)
    }

    pub fn filter_spent_outpoints_in_mempool(
        &self,
        outpoints: &Vec<OutPoint>,
    ) -> DBResult<Vec<OutPoint>> {
        let cf_handle = self.cf_handle(SPENT_OUTPOINTS_MEMPOOL_CF)?;
        let mut spent_outpoints = Vec::new();

        let keys: Vec<_> = outpoints
            .iter()
            .map(|outpoint| (&cf_handle, outpoint_to_bytes(outpoint)))
            .collect();

        let results = self.db.multi_get_cf(keys);

        for (i, result) in results.iter().enumerate() {
            match result {
                Ok(Some(_)) => {
                    spent_outpoints.push(outpoints[i].clone());
                }
                _ => {}
            }
        }

        Ok(spent_outpoints)
    }

    pub fn get_transaction_raw(&self, txid: &Txid, mempool: bool) -> DBResult<Vec<u8>> {
        let cf_handle = if mempool {
            self.cf_handle(TRANSACTIONS_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTIONS_CF)?
        };

        self.get_option_vec_data(&cf_handle, txid_to_bytes(txid))
            .transpose()
            .ok_or(RocksDBError::NotFound(format!(
                "transaction not found: {}",
                txid
            )))?
    }

    pub fn get_transaction(&self, txid: &Txid, mempool: bool) -> DBResult<Transaction> {
        let cf_handle = if mempool {
            self.cf_handle(TRANSACTIONS_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTIONS_CF)?
        };

        let transaction = self
            .get_option_vec_data(&cf_handle, txid_to_bytes(txid))
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "transaction not found: {}",
                txid
            )))?;

        Ok(transaction)
    }

    pub fn partition_transactions_by_existence<'a, I>(
        &self,
        txids: I,
    ) -> DBResult<(Vec<Txid>, Vec<Txid>)>
    where
        I: IntoIterator<Item = &'a Txid>,
    {
        let (mut exists, mut not_exists): (Vec<Txid>, Vec<Txid>) = {
            let mempool = self
                .mempool_cache
                .read()
                .map_err(|_| RocksDBError::LockPoisoned)?;

            let (in_mempool, not_in_mempool) =
                txids.into_iter().partition(|txid| mempool.contains(*txid));

            (in_mempool, not_in_mempool)
        };

        let cf_handle = self.cf_handle(TRANSACTIONS_CF)?;
        let keys: Vec<_> = not_exists
            .iter()
            .map(|txid| (&cf_handle, txid_to_bytes(txid)))
            .collect();

        let results = self.db.multi_get_cf(keys);

        for (i, result) in results.iter().enumerate() {
            match result {
                Ok(Some(_)) => {
                    exists.push(not_exists[i].clone());
                    not_exists.remove(i);
                }
                Ok(None) => {}
                Err(e) => return Err(e.clone().into()),
            }
        }

        Ok((exists, not_exists))
    }

    pub fn get_transaction_confirming_block(&self, txid: &Txid) -> DBResult<BlockId> {
        let cf_handle = self.cf_handle(TRANSACTION_CONFIRMING_BLOCK_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, txid_to_bytes(txid))
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "transaction confirming block not found: {}",
                txid
            )))?)
    }

    pub fn get_transaction_confirming_blocks(
        &self,
        txids: &Vec<Txid>,
    ) -> DBResult<HashMap<Txid, Option<BlockId>>> {
        let cf_handle = self.cf_handle(TRANSACTION_CONFIRMING_BLOCK_CF)?;
        let keys: Vec<_> = txids
            .iter()
            .map(|txid| (&cf_handle, txid_to_bytes(txid)))
            .collect();
        let results = self.db.multi_get_cf(keys);

        let mut confirming_blocks = HashMap::with_capacity(txids.len());
        for (i, result) in results.iter().enumerate() {
            match result {
                Ok(Some(bytes)) => {
                    confirming_blocks.insert(txids[i].clone(), Some(BlockId::load(bytes.clone())));
                }
                Ok(None) => {
                    confirming_blocks.insert(txids[i].clone(), None);
                }
                Err(e) => return Err(e.clone().into()),
            }
        }

        Ok(confirming_blocks)
    }

    pub fn batch_update(&self, update: &BatchUpdate, mempool: bool) -> DBResult<()> {
        let mut batch = WriteBatch::default();

        // 1. Update blocks
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(BLOCKS_CF)?;
            for (block_hash, block) in update.blocks.iter() {
                batch.put_cf(
                    &cf_handle,
                    block_hash.as_raw_hash().to_byte_array(),
                    block.clone().store(),
                );
            }
        }

        // 2. Update block_hashes
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(BLOCK_HEIGHT_TO_HASH_CF)?;
            for (block_height, block_hash) in update.block_hashes.iter() {
                batch.put_cf(
                    &cf_handle,
                    block_height.to_le_bytes(),
                    block_hash.as_raw_hash().to_byte_array(),
                );
            }
        }

        // 3. Update txouts
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
            } else {
                self.cf_handle(OUTPOINTS_CF)?
            };

            for (outpoint, txout) in update.txouts.iter() {
                batch.put_cf(
                    &cf_handle,
                    outpoint_to_bytes(&outpoint),
                    txout.clone().store(),
                );
            }
        }

        // 4. Update tx_state_changes
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF)?
            } else {
                self.cf_handle(TRANSACTIONS_STATE_CHANGE_CF)?
            };

            for (txid, tx_state_change) in update.tx_state_changes.iter() {
                batch.put_cf(
                    &cf_handle,
                    txid_to_bytes(&txid),
                    tx_state_change.clone().store(),
                );
            }
        }

        // 5. Update runes
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNES_CF)?;

            for (rune_id, rune) in update.runes.iter() {
                batch.put_cf(&cf_handle, rune_id_to_bytes(&rune_id), rune.clone().store());
            }
        }

        // 6. Update rune_ids
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNE_IDS_CF)?;

            for (rune, rune_id) in update.rune_ids.iter() {
                let rune_id_wrapper = RuneIdWrapper(rune_id.clone());
                batch.put_cf(&cf_handle, rune.to_le_bytes(), rune_id_wrapper.store());
            }
        }

        // 7. Update rune_numbers
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNE_NUMBER_CF)?;

            for (number, rune_id) in update.rune_numbers.iter() {
                let rune_id_wrapper = RuneIdWrapper(rune_id.clone());
                batch.put_cf(&cf_handle, number.to_le_bytes(), rune_id_wrapper.store());
            }
        }

        // 8. Update inscriptions
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(INSCRIPTIONS_CF)?;

            for (inscription_id, inscription) in update.inscriptions.iter() {
                batch.put_cf(
                    &cf_handle,
                    inscription_id_to_bytes(&inscription_id),
                    inscription.clone().store(),
                );
            }
        }

        // 9. Update mempool_txs
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(MEMPOOL_CF)?;
            for txid in update.mempool_txs.iter() {
                batch.put_cf(&cf_handle, txid_to_bytes(&txid), vec![1]);

                self.mempool_cache
                    .write()
                    .map_err(|_| RocksDBError::LockPoisoned)?
                    .insert(txid.clone());
            }
        }

        // 10. Update runes_count
        if !mempool {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(STATS_CF)?;
            batch.put_cf(
                &cf_handle,
                RUNES_COUNT_KEY,
                update.rune_count.to_le_bytes().to_vec(),
            );
        }

        // 11. Update block_count
        if !mempool {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(STATS_CF)?;
            batch.put_cf(
                &cf_handle,
                BLOCK_COUNT_KEY,
                update.block_count.to_le_bytes().to_vec(),
            );
        }

        // 12. Update purged_blocks_count
        if !mempool {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(STATS_CF)?;
            batch.put_cf(
                &cf_handle,
                PURGED_BLOCKS_COUNT_KEY,
                update.purged_blocks_count.to_le_bytes().to_vec(),
            );
        }

        // 13. Update addresses
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(SCRIPT_PUBKEYS_MEMPOOL_CF)?
            } else {
                self.cf_handle(SCRIPT_PUBKEYS_CF)?
            };

            for (script_pubkey, (new_ops, spent_ops)) in update.script_pubkeys.iter() {
                for outpoint in new_ops.iter() {
                    batch.put_cf(
                        &cf_handle,
                        script_pubkey_outpoint_to_bytes(script_pubkey, outpoint),
                        vec![1],
                    );
                }

                for outpoint in spent_ops.iter() {
                    batch.delete_cf(
                        &cf_handle,
                        script_pubkey_outpoint_to_bytes(script_pubkey, outpoint),
                    );
                }
            }
        }

        // 14. Update address_outpoints
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(OUTPOINT_TO_SCRIPT_PUBKEY_MEMPOOL_CF)?
            } else {
                self.cf_handle(OUTPOINT_TO_SCRIPT_PUBKEY_CF)?
            };

            for (outpoint, script_pubkey) in update.script_pubkeys_outpoints.iter() {
                batch.put_cf(
                    &cf_handle,
                    outpoint_to_bytes(&outpoint),
                    script_pubkey.clone(),
                );
            }
        }

        // 15. Update spent_outpoints_in_mempool
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> =
                self.cf_handle(SPENT_OUTPOINTS_MEMPOOL_CF)?;
            for outpoint in update.spent_outpoints_in_mempool.iter() {
                batch.put_cf(&cf_handle, outpoint_to_bytes(&outpoint), vec![1]);
            }
        }

        // 16. Update transactions
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(TRANSACTIONS_MEMPOOL_CF)?
            } else {
                self.cf_handle(TRANSACTIONS_CF)?
            };

            for (txid, transaction) in update.transactions.iter() {
                batch.put_cf(
                    &cf_handle,
                    txid_to_bytes(&txid),
                    consensus::serialize(&transaction.clone()),
                );
            }
        }

        // 17. Update transaction_confirming_block
        if !mempool {
            let cf_handle: Arc<BoundColumnFamily<'_>> =
                self.cf_handle(TRANSACTION_CONFIRMING_BLOCK_CF)?;

            for (txid, block_id) in update.transaction_confirming_block.iter() {
                batch.put_cf(&cf_handle, txid_to_bytes(&txid), block_id.clone().store());
            }
        }

        // Proceed with the actual write
        self.db.write(batch)?;

        // 12. Update rune_transactions. This is batched on its own.
        if !update.rune_transactions.is_empty() {
            self.add_rune_transactions_batch(&update.rune_transactions, mempool)?;
        }

        Ok(())
    }

    pub fn batch_delete(&self, delete: &BatchDelete) -> DBResult<()> {
        let mut batch = WriteBatch::default();

        // 1. Delete blocks
        {
            let cf_handle = self.cf_handle(OUTPOINTS_CF)?;
            let cf_handle_mempool = self.cf_handle(OUTPOINTS_MEMPOOL_CF)?;

            for txout in delete.tx_outs.iter() {
                batch.delete_cf(&cf_handle, outpoint_to_bytes(&txout));
                batch.delete_cf(&cf_handle_mempool, outpoint_to_bytes(&txout));
            }
        }

        // 2. Delete tx_state_changes
        {
            let cf_handle = self.cf_handle(TRANSACTIONS_STATE_CHANGE_CF)?;
            let cf_handle_mempool = self.cf_handle(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF)?;

            for txid in delete.tx_state_changes.iter() {
                batch.delete_cf(&cf_handle, txid_to_bytes(&txid));
                batch.delete_cf(&cf_handle_mempool, txid_to_bytes(&txid));
            }
        }

        // 3. Delete script_pubkeys_outpoints
        {
            let cf_handle = self.cf_handle(OUTPOINT_TO_SCRIPT_PUBKEY_CF)?;
            let cf_handle_mempool = self.cf_handle(OUTPOINT_TO_SCRIPT_PUBKEY_MEMPOOL_CF)?;

            for outpoint in delete.script_pubkeys_outpoints.iter() {
                batch.delete_cf(&cf_handle, outpoint_to_bytes(&outpoint));
                batch.delete_cf(&cf_handle_mempool, outpoint_to_bytes(&outpoint));
            }
        }

        // 4. Delete spent_outpoints_in_mempool
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> =
                self.cf_handle(SPENT_OUTPOINTS_MEMPOOL_CF)?;
            for outpoint in delete.spent_outpoints_in_mempool.iter() {
                batch.delete_cf(&cf_handle, outpoint_to_bytes(&outpoint));
            }
        }

        self.db.write(batch)?;
        Ok(())
    }

    pub fn batch_rollback(&self, rollback: &BatchRollback, mempool: bool) -> DBResult<()> {
        let mut batch = WriteBatch::default();

        // 1. Update runes count
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(STATS_CF)?;
            batch.put_cf(
                &cf_handle,
                RUNES_COUNT_KEY,
                rollback.runes_count.to_le_bytes().to_vec(),
            );
        }

        // 2. Update rune_entry
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNES_CF)?;
            for (rune_id, rune_entry) in rollback.rune_entry.iter() {
                batch.put_cf(
                    &cf_handle,
                    rune_id_to_bytes(&rune_id),
                    rune_entry.clone().store(),
                );
            }
        }

        // 3. Update txouts
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
            } else {
                self.cf_handle(OUTPOINTS_CF)?
            };

            for (outpoint, txout) in rollback.txouts.iter() {
                batch.put_cf(
                    &cf_handle,
                    outpoint_to_bytes(&outpoint),
                    txout.clone().store(),
                );
            }
        }

        // 4. Update script_pubkey_entry
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(SCRIPT_PUBKEYS_MEMPOOL_CF)?
            } else {
                self.cf_handle(SCRIPT_PUBKEYS_CF)?
            };

            for (script_pubkey, (new_ops, spent_ops)) in rollback.script_pubkey_entry.iter() {
                for outpoint in new_ops.iter() {
                    batch.put_cf(
                        &cf_handle,
                        script_pubkey_outpoint_to_bytes(script_pubkey, outpoint),
                        vec![1],
                    );
                }

                for outpoint in spent_ops.iter() {
                    batch.delete_cf(
                        &cf_handle,
                        script_pubkey_outpoint_to_bytes(script_pubkey, outpoint),
                    );
                }
            }
        }

        // 5. Update outpoints_to_delete
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
            } else {
                self.cf_handle(OUTPOINTS_CF)?
            };

            for outpoint in rollback.outpoints_to_delete.iter() {
                batch.delete_cf(&cf_handle, outpoint_to_bytes(outpoint));
            }
        }

        // 6. Script pubkey outpoints
        {
            let cf_handle = if mempool {
                self.cf_handle(OUTPOINT_TO_SCRIPT_PUBKEY_MEMPOOL_CF)?
            } else {
                self.cf_handle(OUTPOINT_TO_SCRIPT_PUBKEY_CF)?
            };

            for outpoint in rollback.outpoints_to_delete.iter() {
                batch.delete_cf(&cf_handle, outpoint_to_bytes(outpoint));
            }
        }

        // 7. Update prev_outpoints_to_delete
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
            } else {
                self.cf_handle(OUTPOINTS_CF)?
            };

            for outpoint in rollback.prev_outpoints_to_delete.iter() {
                batch.delete_cf(&cf_handle, outpoint_to_bytes(outpoint));
            }
        }

        // 8. Update runes_to_delete
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNES_CF)?;
            for rune_id in rollback.runes_to_delete.iter() {
                batch.delete_cf(&cf_handle, rune_id_to_bytes(rune_id));
            }
        }

        // 9. Update runes_ids_to_delete
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNE_IDS_CF)?;
            for rune in rollback.runes_ids_to_delete.iter() {
                batch.delete_cf(&cf_handle, rune.0.to_le_bytes());
            }
        }

        // 10. Update rune_numbers_to_delete
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNE_NUMBER_CF)?;
            for number in rollback.rune_numbers_to_delete.iter() {
                batch.delete_cf(&cf_handle, number.to_le_bytes());
            }
        }

        // 11. Update inscriptions_to_delete
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(INSCRIPTIONS_CF)?;
            for inscription_id in rollback.inscriptions_to_delete.iter() {
                batch.delete_cf(&cf_handle, inscription_id_to_bytes(inscription_id));
            }
        }

        // 12. Update delete_all_rune_transactions in block
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNE_TRANSACTIONS_CF)?;
            for rune_id in rollback.delete_all_rune_transactions.iter() {
                batch.delete_cf(&cf_handle, rune_id_to_bytes(rune_id));
            }
        }

        // 13. Update delete_all_rune_transactions in mempool
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> =
                self.cf_handle(RUNE_TRANSACTIONS_MEMPOOL_CF)?;
            for rune_id in rollback.delete_all_rune_transactions.iter() {
                batch.delete_cf(&cf_handle, rune_id_to_bytes(rune_id));
            }
        }

        // 14. Update txs_to_delete
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(TRANSACTIONS_MEMPOOL_CF)?
            } else {
                self.cf_handle(TRANSACTIONS_CF)?
            };

            for txid in rollback.txs_to_delete.iter() {
                batch.delete_cf(&cf_handle, txid_to_bytes(txid));
            }
        }

        // 15. Update tx state changes
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF)?
            } else {
                self.cf_handle(TRANSACTIONS_STATE_CHANGE_CF)?
            };

            for txid in rollback.txs_to_delete.iter() {
                batch.delete_cf(&cf_handle, txid_to_bytes(txid));
            }
        }

        // 16. Update tx confirming block
        if !mempool {
            let cf_handle: Arc<BoundColumnFamily<'_>> =
                self.cf_handle(TRANSACTION_CONFIRMING_BLOCK_CF)?;
            for txid in rollback.txs_to_delete.iter() {
                batch.delete_cf(&cf_handle, txid_to_bytes(txid));
            }
        }

        // 17. Remove mempool txs
        if mempool {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(MEMPOOL_CF)?;
            for txid in rollback.txs_to_delete.iter() {
                batch.delete_cf(&cf_handle, txid_to_bytes(txid));

                // Update cache
                self.mempool_cache
                    .write()
                    .map_err(|_| RocksDBError::LockPoisoned)?
                    .remove(txid);
            }
        }

        self.db.write(batch)?;

        self.delete_rune_transactions(&rollback.txs_to_delete, mempool)?;

        // Update runen numbers after revert.
        let total_runes_before_delete =
            rollback.runes_count + rollback.rune_numbers_to_delete.len() as u64;

        self.update_rune_numbers_after_revert(
            &rollback.rune_numbers_to_delete,
            total_runes_before_delete,
        )?;

        Ok(())
    }

    fn update_rune_numbers_after_revert(
        &self,
        rune_numbers_deleted: &Vec<u64>,
        total_runes: u64,
    ) -> DBResult<()> {
        if rune_numbers_deleted.is_empty() {
            return Ok(());
        }

        // Sort rune numbers in ascending order
        let mut rune_numbers_deleted = rune_numbers_deleted.clone();
        rune_numbers_deleted.sort();

        // Now we have to fill the gaps in the rune numbers.
        let start = rune_numbers_deleted[0] + 1;
        let mut to_substract = 1;
        let mut rune_numbers_to_update = HashMap::new();
        for i in start..total_runes {
            if rune_numbers_deleted.contains(&i) {
                to_substract += 1;
                continue;
            }

            rune_numbers_to_update.insert(i, i - to_substract);
        }

        let rune_ids_to_update =
            self.get_rune_ids_by_numbers(&rune_numbers_to_update.keys().cloned().collect())?;

        let mut rune_entries_to_update = self.get_runes_by_ids(
            &rune_ids_to_update
                .values()
                .cloned()
                .collect::<Vec<RuneId>>(),
        )?;

        let mut batch = WriteBatch::default();

        let runes_cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNES_CF)?;
        let rune_number_cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNE_NUMBER_CF)?;

        for (number, rune_id) in rune_ids_to_update {
            let rune_entry = rune_entries_to_update.get_mut(&rune_id).unwrap();
            let new_number = rune_numbers_to_update.get(&number).unwrap();
            rune_entry.number = *new_number;
            batch.put_cf(
                &runes_cf_handle,
                rune_id_to_bytes(&rune_id),
                rune_entry.clone().store(),
            );

            batch.put_cf(
                &rune_number_cf_handle,
                new_number.to_le_bytes(),
                rune_id_to_bytes(&rune_id),
            );
        }

        self.db.write(batch)?;
        Ok(())
    }

    pub fn set_subscription(&self, sub: &Subscription) -> DBResult<()> {
        let cf_handle = self.cf_handle(SUBSCRIPTIONS_CF)?;
        self.db
            .put_cf(&cf_handle, sub.id.as_bytes(), sub.clone().store())?;
        Ok(())
    }

    pub fn get_subscription(&self, id: &Uuid) -> DBResult<Subscription> {
        let cf_handle = self.cf_handle(SUBSCRIPTIONS_CF)?;
        let key = id.as_bytes();
        let data =
            self.get_option_vec_data(&cf_handle, key)
                .mapped()?
                .ok_or(RocksDBError::NotFound(format!(
                    "Subscription not found: {}",
                    id
                )))?;
        Ok(data)
    }

    pub fn get_subscriptions(&self) -> DBResult<Vec<Subscription>> {
        let cf_handle = self.cf_handle(SUBSCRIPTIONS_CF)?;
        let iter = self.db.iterator_cf(&cf_handle, IteratorMode::Start);
        let mut subs = Vec::new();
        for item in iter {
            let (_key, value) = item?;
            subs.push(Subscription::load(value.to_vec()));
        }

        Ok(subs)
    }

    pub fn delete_subscription(&self, id: &Uuid) -> DBResult<()> {
        let cf_handle = self.cf_handle(SUBSCRIPTIONS_CF)?;
        let key = id.as_bytes();
        self.db.delete_cf(&cf_handle, key)?;
        Ok(())
    }

    pub fn update_subscription_last_success(
        &self,
        subscription_id: &Uuid,
        new_time_secs: u64,
    ) -> DBResult<()> {
        let mut sub = self.get_subscription(subscription_id)?;
        sub.last_success_epoch_secs = new_time_secs;
        self.set_subscription(&sub)
    }

    pub fn flush(&self) -> DBResult<()> {
        self.db.flush()?;
        Ok(())
    }

    pub fn close(self) -> DBResult<()> {
        // 1. Explicitly flush any pending writes
        self.flush()?;

        // 2. Because we're consuming `self`, the DB reference is dropped
        //    at the end of this function.
        //    That will release RocksDB file handles, locks, etc.
        Ok(())
    }
}
