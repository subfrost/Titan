use {
    crate::{
        db::{RocksDB, RocksDBError},
        models::{
            BatchDelete, BatchRollback, BatchUpdate, BlockId, Inscription, RuneEntry,
            TransactionStateChange,
        },
    },
    bitcoin::{consensus, hex::HexToArrayError, BlockHash, OutPoint, ScriptBuf, Txid},
    ordinals::{Rune, RuneId},
    std::collections::{HashMap, HashSet},
    thiserror::Error,
    titan_types::{
        Block, InscriptionId, Pagination, PaginationResponse, SpenderReference, SpentStatus,
        Transaction, TransactionStatus, TxOutEntry,
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
    // settings
    fn is_index_addresses(&self) -> Result<Option<bool>, StoreError>;
    fn set_index_addresses(&self, value: bool) -> Result<(), StoreError>;
    fn is_index_bitcoin_transactions(&self) -> Result<Option<bool>, StoreError>;
    fn set_index_bitcoin_transactions(&self, value: bool) -> Result<(), StoreError>;

    // block
    fn get_block_count(&self) -> Result<u64, StoreError>;
    fn set_block_count(&self, count: u64) -> Result<(), StoreError>;
    fn get_purged_blocks_count(&self) -> Result<u64, StoreError>;

    fn get_block_hash(&self, height: u64) -> Result<BlockHash, StoreError>;
    fn delete_block_hash(&self, height: u64) -> Result<(), StoreError>;

    fn get_block_by_hash(&self, hash: &BlockHash) -> Result<Block, StoreError>;
    fn delete_block(&self, hash: &BlockHash) -> Result<(), StoreError>;

    // mempool
    fn is_tx_in_mempool(&self, txid: &Txid) -> Result<bool, StoreError>;
    fn get_mempool_txids(&self) -> Result<HashSet<Txid>, StoreError>;

    // outpoint
    fn get_tx_out(
        &self,
        outpoint: &OutPoint,
        mempool: Option<bool>,
    ) -> Result<TxOutEntry, StoreError>;
    fn get_tx_out_with_mempool_spent_update(
        &self,
        outpoint: &OutPoint,
        mempool: Option<bool>,
    ) -> Result<TxOutEntry, StoreError>;
    fn get_tx_outs(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: Option<bool>,
    ) -> Result<HashMap<OutPoint, TxOutEntry>, StoreError>;
    fn get_tx_outs_with_mempool_spent_update(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: Option<bool>,
    ) -> Result<HashMap<OutPoint, TxOutEntry>, StoreError>;

    // transaction changes
    fn get_tx_state_changes(
        &self,
        txid: &Txid,
        mempool: Option<bool>,
    ) -> Result<TransactionStateChange, StoreError>;
    fn get_txs_state_changes(
        &self,
        txids: &Vec<Txid>,
        mempool: bool,
    ) -> Result<HashMap<Txid, TransactionStateChange>, StoreError>;

    // bitcoin transactions
    fn get_transaction_raw(
        &self,
        txid: &Txid,
        mempool: Option<bool>,
    ) -> Result<Vec<u8>, StoreError>;
    fn get_transaction(
        &self,
        txid: &Txid,
        mempool: Option<bool>,
    ) -> Result<Transaction, StoreError>;
    fn get_transaction_confirming_block(&self, txid: &Txid) -> Result<BlockId, StoreError>;
    fn get_transaction_confirming_blocks(
        &self,
        txids: &Vec<Txid>,
    ) -> Result<HashMap<Txid, Option<BlockId>>, StoreError>;
    fn partition_transactions_by_existence(
        &self,
        txids: &HashSet<Txid>,
    ) -> Result<(Vec<Txid>, Vec<Txid>), StoreError>;

    // rune transactions
    fn get_last_rune_transactions(
        &self,
        rune_id: &RuneId,
        pagination: Option<Pagination>,
        mempool: Option<bool>,
    ) -> Result<PaginationResponse<Txid>, StoreError>;

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

    // address
    fn get_script_pubkey_outpoints(
        &self,
        script_pubkey: &ScriptBuf,
        mempool: Option<bool>,
    ) -> Result<Vec<OutPoint>, StoreError>;
    fn get_outpoints_to_script_pubkey(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: Option<bool>,
        optimistic: bool,
    ) -> Result<HashMap<OutPoint, ScriptBuf>, StoreError>;

    // batch
    fn batch_update(&self, update: &BatchUpdate, mempool: bool) -> Result<(), StoreError>;
    fn batch_delete(&self, delete: &BatchDelete) -> Result<(), StoreError>;
    fn batch_rollback(&self, rollback: &BatchRollback, mempool: bool) -> Result<(), StoreError>;
}

impl Store for RocksDB {
    fn is_index_addresses(&self) -> Result<Option<bool>, StoreError> {
        Ok(self.is_index_addresses()?)
    }

    fn set_index_addresses(&self, value: bool) -> Result<(), StoreError> {
        Ok(self.set_index_addresses(value)?)
    }

    fn is_index_bitcoin_transactions(&self) -> Result<Option<bool>, StoreError> {
        Ok(self.is_index_bitcoin_transactions()?)
    }

    fn set_index_bitcoin_transactions(&self, value: bool) -> Result<(), StoreError> {
        Ok(self.set_index_bitcoin_transactions(value)?)
    }

    fn get_block_count(&self) -> Result<u64, StoreError> {
        Ok(self.get_block_count()?)
    }

    fn set_block_count(&self, count: u64) -> Result<(), StoreError> {
        Ok(self.set_block_count(count)?)
    }

    fn get_purged_blocks_count(&self) -> Result<u64, StoreError> {
        Ok(self.get_purged_blocks_count()?)
    }

    fn get_block_hash(&self, height: u64) -> Result<BlockHash, StoreError> {
        Ok(self.get_block_hash(height)?)
    }

    fn delete_block_hash(&self, height: u64) -> Result<(), StoreError> {
        Ok(self.delete_block_hash(height)?)
    }

    fn get_block_by_hash(&self, hash: &BlockHash) -> Result<Block, StoreError> {
        Ok(self.get_block_by_hash(&hash)?)
    }

    fn delete_block(&self, hash: &BlockHash) -> Result<(), StoreError> {
        Ok(self.delete_block(&hash)?)
    }

    fn get_runes_by_ids(
        &self,
        rune_ids: &Vec<RuneId>,
    ) -> Result<HashMap<RuneId, RuneEntry>, StoreError> {
        Ok(self.get_runes_by_ids(rune_ids)?)
    }

    fn get_runes(
        &self,
        pagination: Pagination,
    ) -> Result<PaginationResponse<(RuneId, RuneEntry)>, StoreError> {
        let runes_count = self.get_runes_count()?;
        let (skip, limit) = pagination.into();

        let start = runes_count.saturating_sub(skip);
        let end = runes_count.saturating_sub(skip).saturating_sub(limit);

        let mut runes = Vec::new();
        for i in (end..start).rev() {
            let rune_id = self.get_rune_id_by_number(i)?;
            let rune_entry = self.get_rune(&rune_id)?;
            runes.push((rune_id, rune_entry));
        }

        Ok(PaginationResponse {
            items: runes,
            offset: start,
        })
    }

    fn get_mempool_txids(&self) -> Result<HashSet<Txid>, StoreError> {
        Ok(self.get_mempool_txids()?)
    }

    fn is_tx_in_mempool(&self, txid: &Txid) -> Result<bool, StoreError> {
        Ok(self.is_tx_in_mempool(txid)?)
    }

    fn get_tx_out(
        &self,
        outpoint: &OutPoint,
        mempool: Option<bool>,
    ) -> Result<TxOutEntry, StoreError> {
        if let Some(mempool) = mempool {
            Ok(self.get_tx_out(outpoint, mempool)?)
        } else {
            match self.get_tx_out(outpoint, false) {
                Ok(tx_out) => return Ok(tx_out),
                Err(err) => match err {
                    RocksDBError::NotFound(_) => Ok(self.get_tx_out(outpoint, true)?),
                    other => Err(StoreError::DB(other)),
                },
            }
        }
    }

    fn get_tx_out_with_mempool_spent_update(
        &self,
        outpoint: &OutPoint,
        mempool: Option<bool>,
    ) -> Result<TxOutEntry, StoreError> {
        let mut tx_out = if let Some(mempool) = mempool {
            self.get_tx_out(outpoint, mempool)?
        } else {
            match self.get_tx_out(outpoint, false) {
                Ok(tx_out) => tx_out,
                Err(err) => match err {
                    RocksDBError::NotFound(_) => self.get_tx_out(outpoint, true)?,
                    other => return Err(StoreError::DB(other)),
                },
            }
        };

        let spent_outpoints: HashMap<OutPoint, Option<SpenderReference>> =
            self.get_spent_outpoints_in_mempool(&vec![*outpoint])?;

        let spent_in_mempool = spent_outpoints.get(outpoint);

        match spent_in_mempool {
            Some(Some(spent)) => {
                tx_out.spent = SpentStatus::Spent(spent.clone());
            }
            _ => {}
        }

        Ok(tx_out)
    }

    fn get_tx_outs(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: Option<bool>,
    ) -> Result<HashMap<OutPoint, TxOutEntry>, StoreError> {
        Ok(self.get_tx_outs(outpoints, mempool)?)
    }

    fn get_tx_outs_with_mempool_spent_update(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: Option<bool>,
    ) -> Result<HashMap<OutPoint, TxOutEntry>, StoreError> {
        let mut tx_outs = self.get_tx_outs(outpoints, mempool)?;
        let spent_outpoints: HashMap<OutPoint, Option<SpenderReference>> =
            self.get_spent_outpoints_in_mempool(outpoints)?;

        for (outpoint, output) in tx_outs.iter_mut() {
            let spent_in_mempool = spent_outpoints.get(outpoint);

            match spent_in_mempool {
                Some(Some(spent)) => {
                    output.spent = SpentStatus::Spent(spent.clone());
                }
                _ => {}
            }
        }

        Ok(tx_outs)
    }

    fn get_tx_state_changes(
        &self,
        txid: &Txid,
        mempool: Option<bool>,
    ) -> Result<TransactionStateChange, StoreError> {
        if let Some(mempool) = mempool {
            Ok(self.get_tx_state_changes(txid, mempool)?)
        } else {
            match self.get_tx_state_changes(txid, false) {
                Ok(tx_state_change) => return Ok(tx_state_change),
                Err(err) => match err {
                    RocksDBError::NotFound(_) => Ok(self.get_tx_state_changes(txid, true)?),
                    other => Err(StoreError::DB(other)),
                },
            }
        }
    }

    fn get_txs_state_changes(
        &self,
        txids: &Vec<Txid>,
        mempool: bool,
    ) -> Result<HashMap<Txid, TransactionStateChange>, StoreError> {
        Ok(self.get_txs_state_changes(txids, mempool)?)
    }

    fn get_transaction(
        &self,
        txid: &Txid,
        mempool: Option<bool>,
    ) -> Result<Transaction, StoreError> {
        let (mut tx, mempool) = if let Some(mempool) = mempool {
            (
                Transaction::from(self.get_transaction(txid, mempool)?),
                mempool,
            )
        } else {
            match self.get_transaction(txid, false) {
                Ok(transaction) => (Transaction::from(transaction), false),
                Err(err) => match err {
                    RocksDBError::NotFound(_) => {
                        (Transaction::from(self.get_transaction(txid, true)?), true)
                    }
                    other => return Err(StoreError::DB(other)),
                },
            }
        };

        if !mempool {
            tx.status = Some(
                self.get_transaction_confirming_block(txid)?
                    .into_transaction_status(),
            );
        } else {
            tx.status = Some(TransactionStatus {
                confirmed: false,
                block_height: None,
                block_hash: None,
            });
        }

        let outpoints = tx
            .output
            .iter()
            .enumerate()
            .map(|(vout, _)| OutPoint {
                txid: txid.clone(),
                vout: vout as u32,
            })
            .collect();

        let tx_outs = self.get_tx_outs_with_mempool_spent_update(&outpoints, Some(mempool))?;

        for (vout, output) in tx.output.iter_mut().enumerate() {
            let tx_out_entry = tx_outs.get(&outpoints[vout]);

            if let Some(tx_out_entry) = tx_out_entry {
                output.runes = tx_out_entry.runes.clone();
                output.risky_runes = tx_out_entry.risky_runes.clone();
                output.spent = tx_out_entry.spent.clone();
            }
        }

        Ok(tx)
    }

    fn get_transaction_raw(
        &self,
        txid: &Txid,
        mempool: Option<bool>,
    ) -> Result<Vec<u8>, StoreError> {
        if let Some(mempool) = mempool {
            Ok(self.get_transaction_raw(txid, mempool)?)
        } else {
            match self.get_transaction_raw(txid, false) {
                Ok(transaction) => return Ok(transaction),
                Err(err) => match err {
                    RocksDBError::NotFound(_) => Ok(self.get_transaction_raw(txid, true)?),
                    other => Err(StoreError::DB(other)),
                },
            }
        }
    }

    fn partition_transactions_by_existence(
        &self,
        txids: &HashSet<Txid>,
    ) -> Result<(Vec<Txid>, Vec<Txid>), StoreError> {
        Ok(self.partition_transactions_by_existence(txids)?)
    }

    fn get_transaction_confirming_block(&self, txid: &Txid) -> Result<BlockId, StoreError> {
        Ok(self.get_transaction_confirming_block(txid)?)
    }

    fn get_transaction_confirming_blocks(
        &self,
        txids: &Vec<Txid>,
    ) -> Result<HashMap<Txid, Option<BlockId>>, StoreError> {
        Ok(self.get_transaction_confirming_blocks(txids)?)
    }

    fn get_runes_count(&self) -> Result<u64, StoreError> {
        Ok(self.get_runes_count()?)
    }

    fn get_rune(&self, rune_id: &RuneId) -> Result<RuneEntry, StoreError> {
        Ok(self.get_rune(rune_id)?)
    }

    fn get_rune_id(&self, rune: &Rune) -> Result<RuneId, StoreError> {
        Ok(self.get_rune_id(&rune.0)?)
    }

    fn get_inscription(&self, inscription_id: &InscriptionId) -> Result<Inscription, StoreError> {
        Ok(self.get_inscription(inscription_id)?)
    }

    fn get_last_rune_transactions(
        &self,
        rune_id: &RuneId,
        pagination: Option<Pagination>,
        mempool: Option<bool>,
    ) -> Result<PaginationResponse<Txid>, StoreError> {
        if let Some(mempool) = mempool {
            Ok(self.get_last_rune_transactions(rune_id, pagination, mempool)?)
        } else {
            // First get mempool transactions
            let mempool_txids = self.get_last_rune_transactions(rune_id, pagination, true)?;

            // Then get non-mempool transactions
            // Adapt pagination to offset
            let non_mempool_pagination = match pagination {
                Some(pagination) => Pagination {
                    skip: pagination.skip.saturating_sub(mempool_txids.offset),
                    limit: pagination
                        .limit
                        .saturating_sub(mempool_txids.items.len() as u64),
                },
                None => Pagination {
                    skip: 0,
                    limit: u64::MAX,
                },
            };

            let non_mempool_txids =
                self.get_last_rune_transactions(rune_id, Some(non_mempool_pagination), false)?;

            let new_offset = mempool_txids.offset + non_mempool_txids.offset;

            Ok(PaginationResponse {
                items: mempool_txids
                    .items
                    .into_iter()
                    .chain(non_mempool_txids.items)
                    .collect(),
                offset: new_offset,
            })
        }
    }

    fn get_script_pubkey_outpoints(
        &self,
        script_pubkey: &ScriptBuf,
        mempool: Option<bool>,
    ) -> Result<Vec<OutPoint>, StoreError> {
        if let Some(mempool) = mempool {
            Ok(self.get_script_pubkey_outpoints(script_pubkey, mempool)?)
        } else {
            let mut ledger_entry = self.get_script_pubkey_outpoints(script_pubkey, false)?;
            let mempool_entry = self.get_script_pubkey_outpoints(script_pubkey, true)?;

            ledger_entry.extend(mempool_entry);

            // Get spent outpoints in mempool.
            let spent_outpoints = self.get_spent_outpoints_in_mempool(&ledger_entry)?;
            ledger_entry.retain(|outpoint| {
                let spent = spent_outpoints.get(outpoint);
                match spent {
                    Some(Some(_)) => false,
                    Some(None) => true,
                    None => true,
                }
            });

            Ok(ledger_entry)
        }
    }

    fn get_outpoints_to_script_pubkey(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: Option<bool>,
        optimistic: bool,
    ) -> Result<HashMap<OutPoint, ScriptBuf>, StoreError> {
        let script_pubkeys = if let Some(mempool) = mempool {
            self.get_outpoints_to_script_pubkey(outpoints, mempool, optimistic)?
        } else {
            let ledger_script_pubkeys =
                self.get_outpoints_to_script_pubkey(outpoints, false, true)?;

            let remaining_outpoints = outpoints
                .iter()
                .filter(|outpoint| !ledger_script_pubkeys.contains_key(outpoint))
                .cloned()
                .collect();

            let mempool_script_pubkeys =
                self.get_outpoints_to_script_pubkey(&remaining_outpoints, true, optimistic)?;

            ledger_script_pubkeys
                .into_iter()
                .chain(mempool_script_pubkeys)
                .collect()
        };

        Ok(script_pubkeys)
    }

    fn batch_update(&self, update: &BatchUpdate, mempool: bool) -> Result<(), StoreError> {
        Ok(self.batch_update(update, mempool)?)
    }

    fn batch_delete(&self, delete: &BatchDelete) -> Result<(), StoreError> {
        Ok(self.batch_delete(delete)?)
    }

    fn batch_rollback(&self, rollback: &BatchRollback, mempool: bool) -> Result<(), StoreError> {
        Ok(self.batch_rollback(rollback, mempool)?)
    }
}
