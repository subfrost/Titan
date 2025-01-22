use {
    super::*,
    crate::models::{
        BatchDelete, BatchUpdate, Block, Inscription, Pagination, PaginationResponse, RuneEntry,
        TransactionStateChange, TxOutEntry, TxRuneIndexRef,
    },
    bitcoin::{consensus, Txid},
    helpers::{rune_index_key, rune_transaction_key},
    mapper::DBResultMapper,
    ordinals::RuneId,
    rocksdb::{
        BlockBasedOptions, BoundColumnFamily, ColumnFamilyDescriptor, DBWithThreadMode,
        IteratorMode, MultiThreaded, Options, WriteBatch,
    },
    std::{
        collections::HashSet,
        sync::{Arc, RwLock},
    },
    tracing::info,
    wrapper::RuneIdWrapper,
};

pub struct RocksDB {
    db: DBWithThreadMode<MultiThreaded>,
    mempool_cache: RwLock<HashSet<Txid>>,
}

pub type DBResult<T> = Result<T, RocksDBError>;

const BLOCKS_CF: &str = "blocks";
const TXS_CF: &str = "txs";
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

const MEMPOOL_CF: &str = "mempool";
const STATS_CF: &str = "stats";
const SETTINGS_CF: &str = "settings";

const INDEX_SPENT_OUTPUTS_KEY: &str = "index_spent_outputs";
const INDEX_ADDRESSES_KEY: &str = "index_addresses";

const BLOCK_COUNT_KEY: &str = "block_count";
const PURGED_BLOCKS_COUNT_KEY: &str = "purged_blocks_count";

impl RocksDB {
    pub fn open(file_path: &str) -> DBResult<Self> {
        // Create descriptors
        let mut cf_opts = Options::default();
        cf_opts.set_compression_type(rocksdb::DBCompressionType::Lz4);

        let blocks_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(BLOCKS_CF, cf_opts.clone());
        let txs_cfd: ColumnFamilyDescriptor = ColumnFamilyDescriptor::new(TXS_CF, cf_opts.clone());
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
        let settings_cfd: ColumnFamilyDescriptor =
            ColumnFamilyDescriptor::new(SETTINGS_CF, cf_opts.clone());

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
                txs_cfd,
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
                settings_cfd,
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

    fn get_option_vec_data(
        &self,
        cf_handle: &Arc<BoundColumnFamily>,
        key: &str,
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

    pub fn set_purged_blocks_count(&self, count: u64) -> DBResult<()> {
        let cf_handle = self.cf_handle(STATS_CF)?;
        self.db.put_cf(
            &cf_handle,
            PURGED_BLOCKS_COUNT_KEY,
            count.to_le_bytes().to_vec(),
        )?;
        Ok(())
    }

    pub fn get_block_hash(&self, height: u64) -> DBResult<String> {
        let cf_handle = self.cf_handle(BLOCK_HEIGHT_TO_HASH_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, height.to_string().as_str())
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "block hash not found: {}",
                height
            )))?)
    }

    pub fn set_block_hash(&self, height: u64, hash: &str) -> DBResult<()> {
        let cf_handle = self.cf_handle(BLOCK_HEIGHT_TO_HASH_CF)?;
        self.db.put_cf(
            &cf_handle,
            height.to_string().as_str(),
            hash.as_bytes().to_vec(),
        )?;
        Ok(())
    }

    pub fn delete_block_hash(&self, height: u64) -> DBResult<()> {
        let cf_handle = self.cf_handle(BLOCK_HEIGHT_TO_HASH_CF)?;
        self.db.delete_cf(&cf_handle, height.to_string().as_str())?;
        Ok(())
    }

    pub fn get_block_by_hash(&self, hash: &str) -> DBResult<Block> {
        let cf_handle = self.cf_handle(BLOCKS_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, hash)
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!("block not found: {}", hash)))?)
    }

    pub fn set_block(&self, hash: &str, block: Block) -> DBResult<()> {
        let cf_handle = self.cf_handle(BLOCKS_CF)?;
        self.db.put_cf(&cf_handle, hash, block.store())?;
        Ok(())
    }

    pub fn delete_block(&self, hash: &str) -> DBResult<()> {
        let cf_handle = self.cf_handle(BLOCKS_CF)?;
        self.db.delete_cf(&cf_handle, hash)?;
        Ok(())
    }

    pub fn get_rune(&self, rune_id: &str) -> DBResult<RuneEntry> {
        let cf_handle = self.cf_handle(RUNES_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, rune_id)
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "rune not found: {}",
                rune_id
            )))?)
    }

    pub fn set_rune(&self, rune_id: &str, rune_entry: RuneEntry) -> DBResult<()> {
        let cf_handle = self.cf_handle(RUNES_CF)?;
        self.db.put_cf(&cf_handle, rune_id, rune_entry.store())?;
        Ok(())
    }

    pub fn delete_rune(&self, rune_id: &str) -> DBResult<()> {
        let cf_handle = self.cf_handle(RUNES_CF)?;
        self.db.delete_cf(&cf_handle, rune_id)?;
        Ok(())
    }

    pub fn get_rune_id_by_number(&self, number: u64) -> DBResult<RuneId> {
        let cf_handle = self.cf_handle(RUNE_NUMBER_CF)?;
        let rune_id_wrapper: RuneIdWrapper = self
            .get_option_vec_data(&cf_handle, number.to_string().as_str())
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "rune id not found: {}",
                number
            )))?;

        Ok(rune_id_wrapper.0)
    }

    pub fn set_rune_id_number(&self, number: u64, rune_id: RuneId) -> DBResult<()> {
        let cf_handle = self.cf_handle(RUNE_NUMBER_CF)?;
        let wrapper = RuneIdWrapper(rune_id);
        self.db
            .put_cf(&cf_handle, number.to_string().as_str(), wrapper.store())?;
        Ok(())
    }

    pub fn delete_rune_id_number(&self, number: u64) -> DBResult<()> {
        let cf_handle = self.cf_handle(RUNE_NUMBER_CF)?;
        self.db.delete_cf(&cf_handle, number.to_string().as_str())?;
        Ok(())
    }

    pub fn get_tx_out(&self, outpoint: &str, mempool: bool) -> DBResult<TxOutEntry> {
        let cf_handle = if mempool {
            self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
        } else {
            self.cf_handle(OUTPOINTS_CF)?
        };

        Ok(self
            .get_option_vec_data(&cf_handle, outpoint)
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "outpoint not found: {}",
                outpoint
            )))?)
    }

    pub fn set_tx_out(&self, outpoint: &str, tx_out: TxOutEntry, mempool: bool) -> DBResult<()> {
        let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
            self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
        } else {
            self.cf_handle(OUTPOINTS_CF)?
        };
        self.db.put_cf(&cf_handle, outpoint, tx_out.store())?;
        Ok(())
    }

    pub fn delete_tx_out(&self, outpoint: &str, mempool: bool) -> DBResult<()> {
        let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
            self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
        } else {
            self.cf_handle(OUTPOINTS_CF)?
        };
        self.db.delete_cf(&cf_handle, outpoint)?;
        Ok(())
    }

    pub fn get_tx_state_changes(
        &self,
        tx_id: &str,
        mempool: bool,
    ) -> DBResult<TransactionStateChange> {
        let cf_handle = if mempool {
            self.cf_handle(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTIONS_STATE_CHANGE_CF)?
        };

        Ok(self
            .get_option_vec_data(&cf_handle, tx_id)
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "transaction state change not found: {}",
                tx_id
            )))?)
    }

    pub fn set_tx_state_changes(
        &self,
        tx_id: &str,
        tx: TransactionStateChange,
        mempool: bool,
    ) -> DBResult<()> {
        let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
            self.cf_handle(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTIONS_STATE_CHANGE_CF)?
        };
        self.db.put_cf(&cf_handle, tx_id, tx.store())?;
        Ok(())
    }

    pub fn delete_tx_state_changes(&self, tx_id: &str, mempool: bool) -> DBResult<()> {
        let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
            self.cf_handle(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTIONS_STATE_CHANGE_CF)?
        };
        self.db.delete_cf(&cf_handle, tx_id)?;
        Ok(())
    }

    pub fn get_runes_count(&self) -> DBResult<u64> {
        let cf_handle = self.cf_handle(STATS_CF)?;
        Ok(self
            .get_option_vec_data(&cf_handle, RUNES_COUNT_KEY)
            .mapped()?
            .unwrap_or(0))
    }

    pub fn set_runes_count(&self, amount: u64) -> DBResult<()> {
        let cf_handle = self.cf_handle(STATS_CF)?;
        self.db
            .put_cf(&cf_handle, RUNES_COUNT_KEY, amount.to_le_bytes().to_vec())?;
        Ok(())
    }

    pub fn get_rune_id(&self, rune: &u128) -> DBResult<RuneId> {
        let cf_handle = self.cf_handle(RUNE_IDS_CF)?;
        let rune_id_wrapper: RuneIdWrapper = self
            .get_option_vec_data(&cf_handle, rune.to_string().as_str())
            .mapped()?
            .ok_or(RocksDBError::NotFound(format!(
                "rune id not found: {}",
                rune
            )))?;

        Ok(rune_id_wrapper.0)
    }

    pub fn set_rune_id(&self, rune: &u128, rune_id: RuneId) -> DBResult<()> {
        let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNE_IDS_CF)?;
        let wrapper = RuneIdWrapper(rune_id);
        self.db
            .put_cf(&cf_handle, rune.to_string().as_str(), wrapper.store())?;
        Ok(())
    }

    pub fn delete_rune_id(&self, rune: &u128) -> DBResult<()> {
        let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNE_IDS_CF)?;
        self.db.delete_cf(&cf_handle, rune.to_string().as_str())?;
        Ok(())
    }

    pub fn get_inscription(&self, id: &String) -> DBResult<Inscription> {
        let cf_handle = self.cf_handle(INSCRIPTIONS_CF)?;
        let inscription: Inscription =
            self.get_option_vec_data(&cf_handle, id)
                .mapped()?
                .ok_or(RocksDBError::NotFound(format!(
                    "inscription not found: {}",
                    id
                )))?;

        Ok(inscription)
    }

    pub fn set_inscription(&self, id: &String, inscription: Inscription) -> DBResult<()> {
        let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(INSCRIPTIONS_CF)?;
        self.db
            .put_cf(&cf_handle, id.to_string().as_str(), inscription.store())?;
        Ok(())
    }

    pub fn delete_inscription(&self, id: &String) -> DBResult<()> {
        let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(INSCRIPTIONS_CF)?;
        self.db.delete_cf(&cf_handle, id.to_string().as_str())?;
        Ok(())
    }

    pub fn get_last_rune_transactions(
        &self,
        rune_id: &str,
        pagination: Option<Pagination>,
        mempool: bool,
    ) -> DBResult<PaginationResponse<String>> {
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
            // "rune:<rune_id>:"
            let mut p = Vec::with_capacity(5 + rune_id.len() + 1);
            p.extend_from_slice(b"rune:");
            p.extend_from_slice(rune_id.as_bytes());
            p.push(b':');
            p
        };

        // 3. Construct the 'seek_key' for end_index
        let seek_key = rune_transaction_key(rune_id, end_index);

        // 4. Reverse iterate
        let mut iter = self.db.iterator_cf(
            &cf_handle,
            rocksdb::IteratorMode::From(&seek_key, rocksdb::Direction::Reverse),
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

            // Convert valuecf to String
            let txid: String =
                String::from_utf8(value_bytes.to_vec()).map_err(|_| RocksDBError::InvalidString)?;

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

    pub fn add_rune_transaction(&self, rune_id: &str, txid: String, mempool: bool) -> DBResult<()> {
        let cf = if mempool {
            self.cf_handle(RUNE_TRANSACTIONS_MEMPOOL_CF)?
        } else {
            self.cf_handle(RUNE_TRANSACTIONS_CF)?
        };

        // 1. Get current last_index from DB
        let last_index_key = rune_index_key(rune_id);
        let last_index = match self.db.get_cf(&cf, &last_index_key)? {
            Some(bytes) => {
                // stored as 8 bytes, little-endian
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&bytes);
                u64::from_le_bytes(arr)
            }
            None => 0,
        };

        // 2. Increment to get new_index
        let new_index = last_index
            .checked_add(1)
            .ok_or_else(|| RocksDBError::Overflow)?;

        // 3. Put the transaction under the key "rune:<rune_id>:<new_index_in_le>"
        let key = rune_transaction_key(rune_id, new_index);
        self.db.put_cf(&cf, key, txid.clone())?;

        // 4. Update the last_index
        self.db
            .put_cf(&cf, last_index_key, &new_index.to_le_bytes())?;

        // 5. **Update the secondary index**: tx_index:<txid> => add (rune_id, new_index)
        let mut idx_refs = self.get_tx_index_refs(&txid, mempool)?;
        idx_refs.push(TxRuneIndexRef {
            rune_id: rune_id.to_string(),
            index: new_index,
        });

        self.set_tx_index_refs(&txid, &idx_refs, mempool)?;

        Ok(())
    }

    fn get_tx_index_refs(&self, txid: &str, mempool: bool) -> DBResult<Vec<TxRuneIndexRef>> {
        let cf_handle = if mempool {
            self.cf_handle(TRANSACTION_RUNE_INDEX_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTION_RUNE_INDEX_CF)?
        };

        let tx_indexes: Vec<TxRuneIndexRef> = self
            .get_option_vec_data(&cf_handle, txid)
            .mapped()?
            .unwrap_or(vec![]);

        Ok(tx_indexes)
    }

    fn set_tx_index_refs(
        &self,
        txid: &str,
        tx_refs: &Vec<TxRuneIndexRef>,
        mempool: bool,
    ) -> DBResult<()> {
        let cf_handle = if mempool {
            self.cf_handle(TRANSACTION_RUNE_INDEX_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTION_RUNE_INDEX_CF)?
        };

        self.db.put_cf(&cf_handle, txid, tx_refs.clone().store())?;
        Ok(())
    }

    /// Remove `txid` from *all* rune lists
    pub fn delete_rune_transaction(&self, txid: &String, mempool: bool) -> DBResult<()> {
        // 1) get all references from the secondary index
        let idx_refs = self.get_tx_index_refs(txid, mempool)?;

        if idx_refs.is_empty() {
            // Nothing to remove
            return Ok(());
        }

        // 2) For each (rune_id, index), remove from primary
        let cf_handle = if mempool {
            self.cf_handle(RUNE_TRANSACTIONS_MEMPOOL_CF)?
        } else {
            self.cf_handle(RUNE_TRANSACTIONS_CF)?
        };

        for TxRuneIndexRef { rune_id, index } in &idx_refs {
            let key = rune_transaction_key(rune_id, *index);
            self.db.delete_cf(&cf_handle, key)?;
        }

        // 3) Remove the entire secondary index key
        let idx_cf_handle = if mempool {
            self.cf_handle(TRANSACTION_RUNE_INDEX_MEMPOOL_CF)?
        } else {
            self.cf_handle(TRANSACTION_RUNE_INDEX_CF)?
        };

        self.db.delete_cf(&idx_cf_handle, txid)?;

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

    pub fn set_mempool_tx(&self, txid: Txid) -> DBResult<()> {
        // Write to DB
        let cf_handle = self.cf_handle(MEMPOOL_CF)?;
        self.db
            .put_cf(&cf_handle, consensus::serialize(&txid), vec![1])?;

        // Update cache
        self.mempool_cache
            .write()
            .map_err(|_| RocksDBError::LockPoisoned)?
            .insert(txid);

        Ok(())
    }

    pub fn is_tx_in_mempool(&self, txid: &Txid) -> DBResult<bool> {
        let exists = self
            .mempool_cache
            .read()
            .map_err(|_| RocksDBError::LockPoisoned)?
            .contains(txid);

        Ok(exists)
    }

    pub fn remove_mempool_tx(&self, txid: &Txid) -> DBResult<()> {
        // Write to DB
        let cf_handle = self.cf_handle(MEMPOOL_CF)?;
        self.db.delete_cf(&cf_handle, consensus::serialize(txid))?;

        // Update cache
        self.mempool_cache
            .write()
            .map_err(|_| RocksDBError::LockPoisoned)?
            .remove(txid);

        Ok(())
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

    pub fn batch_update(&self, update: BatchUpdate, mempool: bool) -> DBResult<()> {
        let mut batch = WriteBatch::default();

        // 1. Update blocks
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(BLOCKS_CF)?;
            for (block_hash, block) in update.blocks {
                batch.put_cf(&cf_handle, block_hash.to_string(), block.store());
            }
        }

        // 2. Update block_hashes
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(BLOCK_HEIGHT_TO_HASH_CF)?;
            for (block_height, block_hash) in update.block_hashes {
                batch.put_cf(&cf_handle, block_height.to_string(), block_hash.to_string());
            }
        }

        // 3. Update txouts
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
            } else {
                self.cf_handle(OUTPOINTS_CF)?
            };

            for (outpoint, txout) in update.txouts {
                batch.put_cf(&cf_handle, outpoint.to_string(), txout.store());
            }
        }

        // 4. Update tx_state_changes
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF)?
            } else {
                self.cf_handle(TRANSACTIONS_STATE_CHANGE_CF)?
            };

            for (txid, tx_state_change) in update.tx_state_changes {
                batch.put_cf(&cf_handle, txid.to_string(), tx_state_change.store());
            }
        }

        // 5. Update runes
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNES_CF)?;

            for (rune_id, rune) in update.runes {
                batch.put_cf(&cf_handle, rune_id.to_string(), rune.store());
            }
        }

        // 6. Update rune_ids
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNE_IDS_CF)?;

            for (rune, rune_id) in update.rune_ids {
                let rune_id_wrapper = RuneIdWrapper(rune_id);
                batch.put_cf(&cf_handle, rune.to_string(), rune_id_wrapper.store());
            }
        }

        // 7. Update rune_numbers
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(RUNE_NUMBER_CF)?;

            for (number, rune_id) in update.rune_numbers {
                let rune_id_wrapper = RuneIdWrapper(rune_id);
                batch.put_cf(&cf_handle, number.to_string(), rune_id_wrapper.store());
            }
        }

        // 8. Update inscriptions
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(INSCRIPTIONS_CF)?;

            for (inscription_id, inscription) in update.inscriptions {
                batch.put_cf(&cf_handle, inscription_id.to_string(), inscription.store());
            }
        }

        // 9. Update mempool_txs
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = self.cf_handle(MEMPOOL_CF)?;
            for txid in update.mempool_txs {
                batch.put_cf(&cf_handle, consensus::serialize(&txid), vec![1]);

                self.mempool_cache
                    .write()
                    .map_err(|_| RocksDBError::LockPoisoned)?
                    .insert(txid);
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

        // Proceed with the actual write
        self.db.write(batch)?;

        // 12. Update rune_transactions. This can't be batched.
        {
            for (rune_id, txids) in update.rune_transactions {
                for txid in txids {
                    self.add_rune_transaction(
                        rune_id.to_string().as_str(),
                        txid.to_string(),
                        mempool,
                    )?;
                }
            }
        }

        Ok(())
    }

    pub fn batch_delete(&self, delete: BatchDelete, mempool: bool) -> DBResult<()> {
        let mut batch = WriteBatch::default();

        // 1. Delete blocks
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(OUTPOINTS_MEMPOOL_CF)?
            } else {
                self.cf_handle(OUTPOINTS_CF)?
            };

            for txout in delete.tx_outs {
                batch.delete_cf(&cf_handle, txout.to_string());
            }
        }

        // 2. Delete tx_state_changes
        {
            let cf_handle: Arc<BoundColumnFamily<'_>> = if mempool {
                self.cf_handle(TRANSACTIONS_STATE_CHANGE_MEMPOOL_CF)?
            } else {
                self.cf_handle(TRANSACTIONS_STATE_CHANGE_CF)?
            };

            for txid in delete.tx_state_changes {
                batch.delete_cf(&cf_handle, txid.to_string());
            }
        }

        self.db.write(batch)?;
        Ok(())
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
