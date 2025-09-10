use {
    super::TransactionStore,
    crate::{
        index::StoreError,
        models::{RuneEntry, TransactionStateChange},
    },
    bitcoin::{Transaction, ScriptBuf},
    ordinals::{Rune, RuneId},
    rustc_hash::FxHashMap as HashMap,
    titan_types::{SerializedOutPoint, SerializedTxid, TxOut},
};

pub(crate) struct MockTransactionStore {
    tx_outs: HashMap<SerializedOutPoint, TxOut>,
    runes: HashMap<RuneId, RuneEntry>,
    rune_ids: HashMap<u128, RuneId>,
}

impl MockTransactionStore {
    pub(crate) fn new() -> Self {
        Self {
            tx_outs: HashMap::default(),
            runes: HashMap::default(),
            rune_ids: HashMap::default(),
        }
    }
}

impl TransactionStore for MockTransactionStore {
    fn get_tx_outs(
        &mut self,
        outpoints: &[SerializedOutPoint],
    ) -> Result<HashMap<SerializedOutPoint, TxOut>, StoreError> {
        let mut result = HashMap::default();
        for outpoint in outpoints {
            if let Some(tx_out) = self.tx_outs.get(outpoint) {
                result.insert(*outpoint, tx_out.clone());
            }
        }
        Ok(result)
    }

    fn set_tx_out(&mut self, outpoint: SerializedOutPoint, tx_out: TxOut, _script_pubkey: ScriptBuf) {
        self.tx_outs.insert(outpoint, tx_out);
    }

    fn set_spent_tx_out(
        &mut self,
        outpoint: &crate::models::TransactionStateChangeInput,
        _spent: titan_types::SpenderReference,
    ) -> Result<(), StoreError> {
        self.tx_outs.remove(&outpoint.previous_outpoint);
        Ok(())
    }

    fn get_transaction(&self, _txid: &SerializedTxid) -> Result<Transaction, StoreError> {
        unimplemented!()
    }

    fn set_transaction(&mut self, _txid: SerializedTxid, _transaction: Transaction) {
        unimplemented!()
    }

    fn get_transaction_confirming_block(
        &self,
        _txid: &SerializedTxid,
    ) -> Result<crate::models::BlockId, StoreError> {
        unimplemented!()
    }

    fn set_transaction_confirming_block(
        &mut self,
        _txid: SerializedTxid,
        _block_id: crate::models::BlockId,
    ) {
        unimplemented!()
    }

    fn get_rune(&mut self, rune_id: &RuneId) -> Result<RuneEntry, StoreError> {
        self.runes
            .get(rune_id)
            .cloned()
            .ok_or(StoreError::NotFound("rune not found".to_string()))
    }

    fn does_rune_exist(&mut self, rune: &Rune) -> Result<(), StoreError> {
        if self.rune_ids.contains_key(&rune.0) {
            Ok(())
        } else {
            Err(StoreError::NotFound("rune not found".to_string()))
        }
    }

    fn get_runes_count(&self) -> u64 {
        self.runes.len() as u64
    }

    fn set_rune_id(&mut self, rune: Rune, id: RuneId) {
        self.rune_ids.insert(rune.0, id);
    }

    fn set_rune(&mut self, id: RuneId, entry: RuneEntry) {
        self.runes.insert(id, entry);
    }

    fn set_rune_id_number(&mut self, _number: u64, _id: RuneId) {
        //
    }

    fn increment_runes_count(&mut self) {
        //
    }

    fn set_inscription(
        &mut self,
        _id: titan_types::InscriptionId,
        _inscription: crate::models::Inscription,
    ) {
        //
    }

    fn set_tx_state_changes(
        &mut self,
        _txid: SerializedTxid,
        _tx_state_changes: TransactionStateChange,
    ) {
        //
    }

    fn add_rune_transaction(&mut self, _rune_id: RuneId, _txid: SerializedTxid) {
        //
    }
}