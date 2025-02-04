use {
    crate::{
        db::{RocksDB, RocksDBError},
        models::{
            BatchDelete, BatchUpdate, Inscription, RuneEntry, ScriptPubkeyEntry,
            TransactionStateChange,
        },
    },
    bitcoin::{hex::HexToArrayError, BlockHash, OutPoint, ScriptBuf, Txid},
    ordinals::{Rune, RuneId},
    types::{Block, InscriptionId, Pagination, PaginationResponse, TxOutEntry},
    std::collections::{HashMap, HashSet},
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
    // settings
    fn is_index_spent_outputs(&self) -> Result<Option<bool>, StoreError>;
    fn set_index_spent_outputs(&self, value: bool) -> Result<(), StoreError>;
    fn is_index_addresses(&self) -> Result<Option<bool>, StoreError>;
    fn set_index_addresses(&self, value: bool) -> Result<(), StoreError>;

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
    fn remove_mempool_tx(&self, txid: &Txid) -> Result<(), StoreError>;

    // outpoint
    fn get_tx_out(
        &self,
        outpoint: &OutPoint,
        mempool: Option<bool>,
    ) -> Result<TxOutEntry, StoreError>;
    fn get_tx_outs(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: Option<bool>,
    ) -> Result<HashMap<OutPoint, TxOutEntry>, StoreError>;
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
    fn delete_tx_state_changes(&self, txid: &Txid, mempool: bool) -> Result<(), StoreError>;

    // rune transactions
    fn get_last_rune_transactions(
        &self,
        rune_id: &RuneId,
        pagination: Option<Pagination>,
        mempool: Option<bool>,
    ) -> Result<PaginationResponse<Txid>, StoreError>;
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
    fn delete_rune_id(&self, rune: &Rune) -> Result<(), StoreError>;
    fn get_runes(
        &self,
        pagination: Pagination,
    ) -> Result<PaginationResponse<(RuneId, RuneEntry)>, StoreError>;

    // inscription
    fn get_inscription(&self, inscription_id: &InscriptionId) -> Result<Inscription, StoreError>;
    fn delete_inscription(&self, inscription_id: &InscriptionId) -> Result<(), StoreError>;

    // address
    fn get_script_pubkey_entry(
        &self,
        script_pubkey: &ScriptBuf,
        mempool: Option<bool>,
    ) -> Result<ScriptPubkeyEntry, StoreError>;
    fn get_script_pubkey_entries(
        &self,
        script_pubkeys: &Vec<ScriptBuf>,
        mempool: bool,
    ) -> Result<HashMap<ScriptBuf, ScriptPubkeyEntry>, StoreError>;
    fn set_script_pubkey_entry(
        &self,
        script_pubkey: &ScriptBuf,
        script_pubkey_entry: ScriptPubkeyEntry,
        mempool: bool,
    ) -> Result<(), StoreError>;
    fn get_outpoints_to_script_pubkey(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: Option<bool>,
        optimistic: bool,
    ) -> Result<HashMap<OutPoint, ScriptBuf>, StoreError>;
    fn delete_script_pubkey_outpoints(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: bool,
    ) -> Result<(), StoreError>;
    fn delete_spent_outpoints_in_mempool(
        &self,
        outpoints: &Vec<OutPoint>,
    ) -> Result<(), StoreError>;

    // batch
    fn batch_update(&self, update: BatchUpdate, mempool: bool) -> Result<(), StoreError>;
    fn batch_delete(&self, delete: BatchDelete) -> Result<(), StoreError>;
}

impl Store for RocksDB {
    fn is_index_spent_outputs(&self) -> Result<Option<bool>, StoreError> {
        Ok(self.is_index_spent_outputs()?)
    }

    fn set_index_spent_outputs(&self, value: bool) -> Result<(), StoreError> {
        Ok(self.set_index_spent_outputs(value)?)
    }

    fn is_index_addresses(&self) -> Result<Option<bool>, StoreError> {
        Ok(self.is_index_addresses()?)
    }

    fn set_index_addresses(&self, value: bool) -> Result<(), StoreError> {
        Ok(self.set_index_addresses(value)?)
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

    fn get_tx_outs(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: Option<bool>,
    ) -> Result<HashMap<OutPoint, TxOutEntry>, StoreError> {
        if let Some(mempool) = mempool {
            Ok(self.get_tx_outs(outpoints, mempool)?)
        } else {
            let ledger_outpoints = self.get_tx_outs(outpoints, false)?;

            let remaining = outpoints
                .iter()
                .filter_map(|outpoint| {
                    ledger_outpoints
                        .iter()
                        .any(|o| *o.0 == *outpoint)
                        .then_some(outpoint.clone())
                })
                .collect();

            let mempool_outpoints = self.get_tx_outs(&remaining, true)?;

            Ok(ledger_outpoints
                .into_iter()
                .chain(mempool_outpoints.into_iter())
                .collect())
        }
    }

    fn set_tx_out(
        &self,
        outpoint: &OutPoint,
        tx_out: TxOutEntry,
        mempool: bool,
    ) -> Result<(), StoreError> {
        Ok(self.set_tx_out(outpoint, tx_out, mempool)?)
    }

    fn delete_tx_out(&self, outpoint: &OutPoint, mempool: bool) -> Result<(), StoreError> {
        Ok(self.delete_tx_out(outpoint, mempool)?)
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

    fn delete_tx_state_changes(&self, txid: &Txid, mempool: bool) -> Result<(), StoreError> {
        Ok(self.delete_tx_state_changes(txid, mempool)?)
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

    fn delete_rune_id(&self, rune: &Rune) -> Result<(), StoreError> {
        Ok(self.delete_rune_id(&rune.0)?)
    }

    fn get_inscription(&self, inscription_id: &InscriptionId) -> Result<Inscription, StoreError> {
        Ok(self.get_inscription(inscription_id)?)
    }

    fn delete_inscription(&self, inscription_id: &InscriptionId) -> Result<(), StoreError> {
        Ok(self.delete_inscription(inscription_id)?)
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

    fn delete_all_rune_transactions(&self, id: &RuneId, mempool: bool) -> Result<(), StoreError> {
        // Get all tx related to the rune and delete their info.
        let rune_transactions = self.get_last_rune_transactions(id, None, mempool)?;

        for txid in rune_transactions.items {
            let mut transaction_state_change = self.get_tx_state_changes(&txid, mempool)?;

            let mut new_outputs = vec![];
            for (index, output) in transaction_state_change.outputs.iter().enumerate() {
                if output.runes.iter().any(|rune| rune.rune_id == *id) {
                    let mut new_output = output.clone();
                    new_output.runes.retain(|rune| rune.rune_id != *id);

                    let outpoint = OutPoint {
                        txid: txid.clone(),
                        vout: index as u32,
                    };

                    self.set_tx_out(&outpoint, new_output.clone(), mempool)?;

                    new_outputs.push(new_output);
                } else {
                    new_outputs.push(output.clone());
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

    fn get_script_pubkey_entry(
        &self,
        script_pubkey: &ScriptBuf,
        mempool: Option<bool>,
    ) -> Result<ScriptPubkeyEntry, StoreError> {
        if let Some(mempool) = mempool {
            Ok(self.get_script_pubkey_entry(script_pubkey, mempool)?)
        } else {
            let ledger_entry = self.get_script_pubkey_entry(script_pubkey, false)?;
            let mempool_entry = self.get_script_pubkey_entry(script_pubkey, true)?;

            let mut entry = ledger_entry.merge(mempool_entry);

            // Get spent outpoints in mempool.
            let spent_outpoints = self.filter_spent_outpoints_in_mempool(&entry.utxos)?;
            entry
                .utxos
                .retain(|outpoint| !spent_outpoints.contains(outpoint));

            Ok(entry)
        }
    }

    fn get_script_pubkey_entries(
        &self,
        script_pubkeys: &Vec<ScriptBuf>,
        mempool: bool,
    ) -> Result<HashMap<ScriptBuf, ScriptPubkeyEntry>, StoreError> {
        Ok(self.get_script_pubkey_entries(script_pubkeys, mempool)?)
    }

    fn set_script_pubkey_entry(
        &self,
        script_pubkey: &ScriptBuf,
        script_pubkey_entry: ScriptPubkeyEntry,
        mempool: bool,
    ) -> Result<(), StoreError> {
        Ok(self.set_script_pubkey_entry(script_pubkey, script_pubkey_entry, mempool)?)
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

    fn delete_script_pubkey_outpoints(
        &self,
        outpoints: &Vec<OutPoint>,
        mempool: bool,
    ) -> Result<(), StoreError> {
        Ok(self.delete_script_pubkey_outpoints(outpoints, mempool)?)
    }

    fn delete_spent_outpoints_in_mempool(
        &self,
        outpoints: &Vec<OutPoint>,
    ) -> Result<(), StoreError> {
        Ok(self.delete_spent_outpoints_in_mempool(outpoints)?)
    }

    fn batch_update(&self, update: BatchUpdate, mempool: bool) -> Result<(), StoreError> {
        Ok(self.batch_update(update, mempool)?)
    }

    fn batch_delete(&self, delete: BatchDelete) -> Result<(), StoreError> {
        Ok(self.batch_delete(delete)?)
    }
}
