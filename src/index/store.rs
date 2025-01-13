use {
    crate::{
        db::{RocksDB, RocksDBError},
        models::{
            BatchDelete, BatchUpdate, Block, Inscription, InscriptionId, Pagination,
            PaginationResponse, RuneEntry, TransactionStateChange, TxOutEntry,
        },
    },
    bitcoin::{hashes::sha256d, hex::HexToArrayError, BlockHash, OutPoint, Txid},
    ordinals::{Rune, RuneId},
    std::{collections::HashSet, str::FromStr},
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("db error {0}")]
    DB(RocksDBError),
    #[error("hex array error {0}")]
    HexToArray(#[from] HexToArrayError),
    #[error("not found: {0}")]
    NotFound(String),
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
    // block
    fn get_block_count(&self) -> Result<u64, StoreError>;
    fn set_block_count(&self, count: u64) -> Result<(), StoreError>;

    fn get_block_hash(&self, height: u64) -> Result<BlockHash, StoreError>;
    fn set_block_hash(&self, height: u64, hash: &BlockHash) -> Result<(), StoreError>;
    fn delete_block_hash(&self, height: u64) -> Result<(), StoreError>;

    fn get_block_by_hash(&self, hash: &BlockHash) -> Result<Block, StoreError>;
    fn set_block(&self, hash: &BlockHash, block: Block) -> Result<(), StoreError>;
    fn delete_block(&self, hash: &BlockHash) -> Result<(), StoreError>;

    // mempool
    fn get_mempool_txids(&self) -> Result<HashSet<Txid>, StoreError>;
    fn remove_mempool_tx(&self, txid: &Txid) -> Result<(), StoreError>;
    fn set_mempool_tx(&self, txid: Txid) -> Result<(), StoreError>;

    // outpoint
    fn get_tx_out(
        &self,
        outpoint: &OutPoint,
        mempool: Option<bool>,
    ) -> Result<TxOutEntry, StoreError>;
    fn set_tx_out(
        &self,
        outpoint: &OutPoint,
        tx_out: TxOutEntry,
        mempool: bool,
    ) -> Result<(), StoreError>;
    fn delete_tx_out(&self, outpoint: &OutPoint, mempool: bool) -> Result<(), StoreError>;

    // transaction changes
    fn get_tx_state_changes(
        &self,
        txid: &Txid,
        mempool: Option<bool>,
    ) -> Result<TransactionStateChange, StoreError>;
    fn set_tx_state_changes(
        &self,
        txid: &Txid,
        tx: TransactionStateChange,
        mempool: bool,
    ) -> Result<(), StoreError>;
    fn delete_tx_state_changes(&self, txid: &Txid, mempool: bool) -> Result<(), StoreError>;

    // rune transactions
    fn get_last_rune_transactions(
        &self,
        rune_id: &RuneId,
        pagination: Option<Pagination>,
        mempool: Option<bool>,
    ) -> Result<PaginationResponse<String>, StoreError>;
    fn add_rune_transaction(
        &self,
        rune_id: &RuneId,
        txid: String,
        mempool: bool,
    ) -> Result<(), StoreError>;
    fn delete_all_rune_transactions(&self, id: &RuneId, mempool: bool) -> Result<(), StoreError>;

    // runes
    fn get_runes_count(&self) -> Result<u64, StoreError>;
    fn set_runes_count(&self, count: u64) -> Result<(), StoreError>;
    fn get_rune(&self, rune_id: &RuneId) -> Result<RuneEntry, StoreError>;
    fn set_rune(&self, rune_id: &RuneId, rune_entry: RuneEntry) -> Result<(), StoreError>;
    fn get_rune_id_by_number(&self, number: u64) -> Result<RuneId, StoreError>;
    fn set_rune_id_number(&self, number: u64, rune_id: RuneId) -> Result<(), StoreError>;
    fn delete_rune_id_number(&self, number: u64) -> Result<(), StoreError>;
    fn delete_rune(&self, rune_id: &RuneId) -> Result<(), StoreError>;
    fn get_rune_id(&self, rune: &Rune) -> Result<RuneId, StoreError>;
    fn set_rune_id(&self, rune: &Rune, rune_id: RuneId) -> Result<(), StoreError>;
    fn delete_rune_id(&self, rune: &Rune) -> Result<(), StoreError>;
    fn get_runes(
        &self,
        pagination: Pagination,
    ) -> Result<PaginationResponse<(RuneId, RuneEntry)>, StoreError>;

    // inscription
    fn get_inscription(&self, inscription_id: &InscriptionId) -> Result<Inscription, StoreError>;
    fn set_inscription(
        &self,
        inscription_id: &InscriptionId,
        inscription: Inscription,
    ) -> Result<(), StoreError>;
    fn delete_inscription(&self, inscription_id: &InscriptionId) -> Result<(), StoreError>;

    fn batch_update(&self, update: BatchUpdate, mempool: bool) -> Result<(), StoreError>;
    fn batch_delete(&self, delete: BatchDelete, mempool: bool) -> Result<(), StoreError>;
}

impl Store for RocksDB {
    fn get_block_count(&self) -> Result<u64, StoreError> {
        Ok(self.get_block_count()?)
    }

    fn set_block_count(&self, count: u64) -> Result<(), StoreError> {
        Ok(self.set_block_count(count)?)
    }

    fn get_block_hash(&self, height: u64) -> Result<BlockHash, StoreError> {
        let hash = self.get_block_hash(height as u64)?;
        Ok(BlockHash::from_raw_hash(sha256d::Hash::from_str(&hash)?))
    }

    fn set_block_hash(&self, height: u64, hash: &BlockHash) -> Result<(), StoreError> {
        Ok(self.set_block_hash(height, &hash.to_string())?)
    }

    fn delete_block_hash(&self, height: u64) -> Result<(), StoreError> {
        Ok(self.delete_block_hash(height)?)
    }

    fn get_block_by_hash(&self, hash: &BlockHash) -> Result<Block, StoreError> {
        Ok(self.get_block_by_hash(&hash.to_string())?)
    }

    fn set_block(&self, hash: &BlockHash, block: Block) -> Result<(), StoreError> {
        Ok(self.set_block(&hash.to_string(), block)?)
    }

    fn delete_block(&self, hash: &BlockHash) -> Result<(), StoreError> {
        Ok(self.delete_block(&hash.to_string())?)
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
            let rune_entry = self.get_rune(&rune_id.to_string())?;
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

    fn remove_mempool_tx(&self, txid: &Txid) -> Result<(), StoreError> {
        Ok(self.remove_mempool_tx(txid)?)
    }

    fn set_mempool_tx(&self, txid: Txid) -> Result<(), StoreError> {
        Ok(self.set_mempool_tx(txid)?)
    }

    fn get_tx_out(
        &self,
        outpoint: &OutPoint,
        mempool: Option<bool>,
    ) -> Result<TxOutEntry, StoreError> {
        if let Some(mempool) = mempool {
            Ok(self.get_tx_out(&outpoint.to_string(), mempool)?)
        } else {
            match self.get_tx_out(&outpoint.to_string(), false) {
                Ok(tx_out) => return Ok(tx_out),
                Err(err) => match err {
                    RocksDBError::NotFound(_) => Ok(self.get_tx_out(&outpoint.to_string(), true)?),
                    other => Err(StoreError::DB(other)),
                },
            }
        }
    }

    fn set_tx_out(
        &self,
        outpoint: &OutPoint,
        tx_out: TxOutEntry,
        mempool: bool,
    ) -> Result<(), StoreError> {
        Ok(self.set_tx_out(&outpoint.to_string(), tx_out, mempool)?)
    }

    fn delete_tx_out(&self, outpoint: &OutPoint, mempool: bool) -> Result<(), StoreError> {
        Ok(self.delete_tx_out(&outpoint.to_string(), mempool)?)
    }

    fn get_tx_state_changes(
        &self,
        txid: &Txid,
        mempool: Option<bool>,
    ) -> Result<TransactionStateChange, StoreError> {
        if let Some(mempool) = mempool {
            Ok(self.get_tx_state_changes(&txid.to_string(), mempool)?)
        } else {
            match self.get_tx_state_changes(&txid.to_string(), false) {
                Ok(tx_state_change) => return Ok(tx_state_change),
                Err(err) => match err {
                    RocksDBError::NotFound(_) => {
                        Ok(self.get_tx_state_changes(&txid.to_string(), true)?)
                    }
                    other => Err(StoreError::DB(other)),
                },
            }
        }
    }

    fn set_tx_state_changes(
        &self,
        txid: &Txid,
        tx: TransactionStateChange,
        mempool: bool,
    ) -> Result<(), StoreError> {
        Ok(self.set_tx_state_changes(&txid.to_string(), tx, mempool)?)
    }

    fn delete_tx_state_changes(&self, txid: &Txid, mempool: bool) -> Result<(), StoreError> {
        Ok(self.delete_tx_state_changes(&txid.to_string(), mempool)?)
    }

    fn get_runes_count(&self) -> Result<u64, StoreError> {
        Ok(self.get_runes_count()?)
    }

    fn set_runes_count(&self, count: u64) -> Result<(), StoreError> {
        Ok(self.set_runes_count(count)?)
    }

    fn get_rune(&self, rune_id: &RuneId) -> Result<RuneEntry, StoreError> {
        Ok(self.get_rune(&rune_id.to_string())?)
    }

    fn set_rune(&self, rune_id: &RuneId, rune_entry: RuneEntry) -> Result<(), StoreError> {
        Ok(self.set_rune(&rune_id.to_string(), rune_entry)?)
    }

    fn get_rune_id_by_number(&self, number: u64) -> Result<RuneId, StoreError> {
        Ok(self.get_rune_id_by_number(number)?)
    }

    fn set_rune_id_number(&self, number: u64, rune_id: RuneId) -> Result<(), StoreError> {
        Ok(self.set_rune_id_number(number, rune_id)?)
    }

    fn delete_rune_id_number(&self, number: u64) -> Result<(), StoreError> {
        Ok(self.delete_rune_id_number(number)?)
    }

    fn delete_rune(&self, rune_id: &RuneId) -> Result<(), StoreError> {
        Ok(self.delete_rune(&rune_id.to_string())?)
    }

    fn get_rune_id(&self, rune: &Rune) -> Result<RuneId, StoreError> {
        Ok(self.get_rune_id(&rune.0)?)
    }

    fn set_rune_id(&self, rune: &Rune, rune_id: RuneId) -> Result<(), StoreError> {
        Ok(self.set_rune_id(&rune.0, rune_id)?)
    }

    fn delete_rune_id(&self, rune: &Rune) -> Result<(), StoreError> {
        Ok(self.delete_rune_id(&rune.0)?)
    }

    fn get_inscription(&self, inscription_id: &InscriptionId) -> Result<Inscription, StoreError> {
        Ok(self.get_inscription(&inscription_id.to_string())?)
    }

    fn set_inscription(
        &self,
        inscription_id: &InscriptionId,
        inscription: Inscription,
    ) -> Result<(), StoreError> {
        Ok(self.set_inscription(&inscription_id.to_string(), inscription)?)
    }

    fn delete_inscription(&self, inscription_id: &InscriptionId) -> Result<(), StoreError> {
        Ok(self.delete_inscription(&inscription_id.to_string())?)
    }

    fn get_last_rune_transactions(
        &self,
        rune_id: &RuneId,
        pagination: Option<Pagination>,
        mempool: Option<bool>,
    ) -> Result<PaginationResponse<String>, StoreError> {
        if let Some(mempool) = mempool {
            Ok(self.get_last_rune_transactions(&rune_id.to_string(), pagination, mempool)?)
        } else {
            // First get mempool transactions
            let mempool_txids =
                self.get_last_rune_transactions(&rune_id.to_string(), pagination, true)?;

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

            let non_mempool_txids = self.get_last_rune_transactions(
                &rune_id.to_string(),
                Some(non_mempool_pagination),
                false,
            )?;

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

    fn add_rune_transaction(
        &self,
        rune_id: &RuneId,
        txid: String,
        mempool: bool,
    ) -> Result<(), StoreError> {
        Ok(self.add_rune_transaction(&rune_id.to_string(), txid, mempool)?)
    }

    fn delete_all_rune_transactions(&self, id: &RuneId, mempool: bool) -> Result<(), StoreError> {
        // Get all tx related to the rune and delete their info.
        let rune_transactions = self.get_last_rune_transactions(&id.to_string(), None, mempool)?;

        for txid in rune_transactions.items {
            let mut transaction_state_change = self.get_tx_state_changes(&txid, mempool)?;

            let mut new_outputs = vec![];
            for output in transaction_state_change.outputs {
                if output.runes.iter().any(|rune| rune.rune_id == *id) {
                    let mut new_output = output.clone();
                    new_output.runes.retain(|rune| rune.rune_id != *id);

                    self.set_tx_out(&txid, new_output.clone(), mempool)?;

                    new_outputs.push(new_output);
                } else {
                    new_outputs.push(output);
                }
            }

            transaction_state_change.outputs = new_outputs;

            // Delete rune transactions
            self.delete_rune_transaction(&txid, mempool)?;

            // Update transaction state change
            self.set_tx_state_changes(&txid, transaction_state_change, mempool)?;
        }

        Ok(())
    }

    fn batch_update(&self, update: BatchUpdate, mempool: bool) -> Result<(), StoreError> {
        Ok(self.batch_update(update, mempool)?)
    }

    fn batch_delete(&self, delete: BatchDelete, mempool: bool) -> Result<(), StoreError> {
        Ok(self.batch_delete(delete, mempool)?)
    }
}
