use {
    crate::{
        index::{store::Store, StoreError},
        models::{InscriptionId, TransactionStateChange},
    },
    bitcoin::{OutPoint, Txid},
    ordinals::RuneId,
    std::sync::Arc,
    thiserror::Error,
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
    runes: u64,
}

impl<'a> Rollback<'a> {
    pub(super) fn new(store: &'a Arc<dyn Store + Send + Sync>, mempool: bool) -> Result<Self> {
        let runes = store.get_runes_count()?;
        Ok(Self {
            store,
            mempool,
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
            let result = rune_entry
                .pending_mints
                .checked_add_signed(amount)
                .ok_or(RollbackError::Overflow("mints".to_string()))?;
            rune_entry.pending_mints = result;
        } else {
            let result = rune_entry
                .mints
                .checked_add_signed(amount)
                .ok_or(RollbackError::Overflow("mints".to_string()))?;

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
            self.update_spendable_input(tx_in, false)?;
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
