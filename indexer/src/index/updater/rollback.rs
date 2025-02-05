use {
    crate::index::{store::Store, StoreError},
    crate::models::TransactionStateChange,
    bitcoin::{OutPoint, Txid},
    ordinals::RuneId,
    std::sync::Arc,
    thiserror::Error,
    titan_types::InscriptionId,
};

#[derive(Debug, Error)]
pub enum RollbackError {
    #[error("store error {0}")]
    Store(#[from] StoreError),
    #[error("overflow in {0}")]
    Overflow(String),
}

type Result<T> = std::result::Result<T, RollbackError>;

pub(super) struct Rollback<'a> {
    store: &'a Arc<dyn Store + Send + Sync>,
    mempool: bool,
    index_addresses: bool,
    runes: u64,
}

impl<'a> Rollback<'a> {
    pub(super) fn new(
        store: &'a Arc<dyn Store + Send + Sync>,
        mempool: bool,
        index_addresses: bool,
    ) -> Result<Self> {
        let runes = store.get_runes_count()?;
        Ok(Self {
            store,
            mempool,
            index_addresses,
            runes,
        })
    }

    fn update_spendable_input(&self, outpoint: &OutPoint, spent: bool) -> Result<()> {
        match self.store.get_tx_out(outpoint, None) {
            Ok(tx_out) => {
                let mut tx_out = tx_out;
                tx_out.spent = spent;
                self.store.set_tx_out(&outpoint, tx_out, self.mempool)?;
                Ok(())
            }
            Err(StoreError::NotFound(_)) => {
                // We don't need to do anything.
                Ok(())
            }
            Err(e) => {
                return Err(RollbackError::Store(e));
            }
        }
    }

    fn update_mints(&self, rune_id: &RuneId, amount: i128) -> Result<()> {
        let mut rune_entry = self.store.get_rune(rune_id)?;

        if self.mempool {
            let result = rune_entry.pending_mints.saturating_add_signed(amount);
            rune_entry.pending_mints = result;
        } else {
            let result = rune_entry.mints.saturating_add_signed(amount);

            rune_entry.mints = result;
        }

        self.store.set_rune(&rune_id, rune_entry)?;

        Ok(())
    }

    fn update_burn_balance(&self, rune_id: &RuneId, amount: i128) -> Result<()> {
        let mut rune_entry = self.store.get_rune(rune_id)?;

        if self.mempool {
            let result = rune_entry
                .pending_burns
                .checked_add_signed(amount)
                .ok_or(RollbackError::Overflow("burn".to_string()))?;
            rune_entry.pending_burns = result;
        } else {
            let result = rune_entry
                .pending_burns
                .checked_add_signed(amount)
                .ok_or(RollbackError::Overflow("burn".to_string()))?;

            rune_entry.burned = result;
        }

        self.store.set_rune(&rune_id, rune_entry)?;

        Ok(())
    }

    pub(super) fn revert_transaction(
        &mut self,
        txid: &Txid,
        transaction: &TransactionStateChange,
    ) -> Result<()> {
        // Make spendable the inputs again.
        for tx_in in transaction.inputs.iter() {
            self.update_spendable_input(&tx_in, false)?;
        }

        // Remove tx_outs
        for (vout, _tx_out) in transaction.outputs.iter().enumerate() {
            let outpoint = OutPoint {
                txid: txid.clone(),
                vout: vout as u32,
            };
            self.store.delete_tx_out(&outpoint, self.mempool)?;
        }

        // Remove etched rune if any.
        // if a reorg has happened and a rune was etched but it doesn't exist now,
        // we need to remove all mints and transfer edicts from other transactions
        // that might have happened.
        if let Some((id, rune)) = transaction.etched {
            let rune_entry = self.store.get_rune(&id)?;
            self.store.delete_rune(&id)?;
            self.store.delete_rune_id(&rune)?;
            self.store.delete_inscription(&InscriptionId {
                txid: txid.clone(),
                index: 0,
            })?;

            // Decrease rune count
            self.runes -= 1;
            self.store.set_runes_count(self.runes)?;
            self.store.delete_rune_id_number(rune_entry.number)?;
            self.update_rune_numbers_after_revert(rune_entry.number, self.runes)?;

            // Delete all rune transactions
            self.store.delete_all_rune_transactions(&id, true)?;
            self.store.delete_all_rune_transactions(&id, false)?;
        }

        // Remove mints if any.
        if let Some(mint) = transaction.minted.as_ref() {
            self.update_mints(&mint.rune_id, -1)?;
        }

        // Remove burned if any.
        for (rune_id, amount) in transaction.burned.iter() {
            self.update_burn_balance(rune_id, -(amount.n() as i128))?;
        }

        if self.index_addresses {
            self.revert_transaction_script_pubkeys_modifications(txid, transaction)?;
        }

        // Finally remove the transaction state change.
        self.store.delete_tx_state_changes(txid, self.mempool)?;
        self.store.delete_transaction(txid, self.mempool)?;

        Ok(())
    }

    fn revert_transaction_script_pubkeys_modifications(
        &self,
        txid: &Txid,
        transaction: &TransactionStateChange,
    ) -> Result<()> {
        // Get outpoints.
        let outpoints_to_script_pubkeys = self.store.get_outpoints_to_script_pubkey(
            &transaction
                .outputs
                .iter()
                .enumerate()
                .map(|(vout, _)| OutPoint {
                    txid: txid.clone(),
                    vout: vout as u32,
                })
                .collect(),
            Some(self.mempool),
            false,
        )?;

        // Get script pubkeys.
        let script_pubkeys = outpoints_to_script_pubkeys.values().cloned().collect();

        // Update script pubkey entries.
        let mut script_pubkey_entries = self
            .store
            .get_script_pubkey_entries(&script_pubkeys, self.mempool)?;

        // Delete outpoints.
        for (vout, _tx_out) in transaction.outputs.iter().enumerate() {
            let outpoint = OutPoint {
                txid: txid.clone(),
                vout: vout as u32,
            };

            let script_pubkey = outpoints_to_script_pubkeys
                .get(&outpoint)
                .unwrap_or_else(|| panic!("script pubkey not found"));

            let script_pubkey_entry = script_pubkey_entries
                .get_mut(script_pubkey)
                .unwrap_or_else(|| panic!("script pubkey entry not found"));

            script_pubkey_entry
                .utxos
                .retain(|pubkey_outpoint| *pubkey_outpoint != outpoint);

            // Update script pubkey entries.
            self.store.set_script_pubkey_entry(
                script_pubkey,
                script_pubkey_entry.clone(),
                self.mempool,
            )?;
        }

        self.store.delete_script_pubkey_outpoints(
            &outpoints_to_script_pubkeys.keys().cloned().collect(),
            self.mempool,
        )?;

        if self.mempool {
            // Remove spent outpoints.
            self.store
                .delete_spent_outpoints_in_mempool(&transaction.inputs)?;
        } else if !transaction.is_coinbase {
            let input_outpoints_to_script_pubkeys =
                self.store
                    .get_outpoints_to_script_pubkey(&transaction.inputs, None, false)?;

            let input_script_pubkeys = input_outpoints_to_script_pubkeys
                .values()
                .cloned()
                .collect();

            let mut input_script_pubkeys_entries = self
                .store
                .get_script_pubkey_entries(&input_script_pubkeys, self.mempool)?;

            for (outpoint, script_pubkey) in input_outpoints_to_script_pubkeys.iter() {
                let script_pubkey_entry = input_script_pubkeys_entries
                    .get_mut(script_pubkey)
                    .unwrap_or_else(|| panic!("script pubkey entry not found"));

                script_pubkey_entry.utxos.push(outpoint.clone());

                self.store.set_script_pubkey_entry(
                    script_pubkey,
                    script_pubkey_entry.clone(),
                    self.mempool,
                )?;
            }
        }

        Ok(())
    }

    fn update_rune_numbers_after_revert(
        &self,
        rune_number_deleted: u64,
        total_runes: u64,
    ) -> Result<()> {
        for i in rune_number_deleted..total_runes {
            match self.store.get_rune_id_by_number(i + 1) {
                Ok(rune_id) => {
                    let mut rune_entry = self.store.get_rune(&rune_id)?;
                    rune_entry.number = i;
                    self.store.set_rune(&rune_id, rune_entry)?;
                    self.store.set_rune_id_number(i, rune_id)?;
                }
                Err(StoreError::NotFound(_)) => {
                    break;
                }
                Err(e) => {
                    return Err(RollbackError::Store(e));
                }
            }
        }

        Ok(())
    }
}
