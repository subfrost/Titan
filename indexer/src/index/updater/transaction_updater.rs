use {
    super::{address::AddressUpdater, cache::UpdaterCache},
    crate::{
        index::{Settings, StoreError},
        models::{BlockId, TransactionStateChange},
    },
    bitcoin::{OutPoint, Transaction, Txid},
    ordinals::RuneId,
    thiserror::Error,
    titan_types::{Event, TxOutEntry},
    tokio::sync::mpsc::error::SendError,
};

#[derive(Debug, Error)]
pub enum TransactionUpdaterError {
    #[error("store error {0}")]
    Store(#[from] StoreError),
    #[error("event sender error {0}")]
    EventSender(#[from] SendError<Event>),
}

type Result<T> = std::result::Result<T, TransactionUpdaterError>;

#[derive(Debug)]
pub(super) struct TransactionUpdaterSettings {
    pub(super) index_addresses: bool,
    pub(super) index_bitcoin_transactions: bool,
}

impl From<Settings> for TransactionUpdaterSettings {
    fn from(settings: Settings) -> Self {
        Self {
            index_addresses: settings.index_addresses,
            index_bitcoin_transactions: settings.index_bitcoin_transactions,
        }
    }
}

pub(super) struct TransactionUpdater<'a> {
    pub(super) address_updater: Option<&'a mut AddressUpdater>,
    pub(super) cache: &'a mut UpdaterCache,

    pub(super) settings: TransactionUpdaterSettings,
}

impl<'a> TransactionUpdater<'a> {
    pub(super) fn new(
        cache: &'a mut UpdaterCache,
        settings: TransactionUpdaterSettings,
        address_updater: Option<&'a mut AddressUpdater>,
    ) -> Result<Self> {
        Ok(Self {
            address_updater,
            cache,
            settings,
        })
    }

    pub(super) fn save(
        &mut self,
        block_id: Option<BlockId>,
        txid: Txid,
        transaction: &Transaction,
        transaction_state_change: &TransactionStateChange,
    ) -> Result<()> {
        // Update burned rune
        for (rune_id, amount) in transaction_state_change.burned.iter() {
            self.burn_rune(
                block_id.as_ref().map(|id| id.height),
                txid,
                rune_id,
                amount.n(),
            );
        }

        // Add minted rune if any.
        if let Some(minted) = transaction_state_change.minted.as_ref() {
            self.mint_rune(
                block_id.as_ref().map(|id| id.height),
                txid,
                &minted.rune_id,
                minted.amount,
            );
        }

        if let Some((id, _)) = transaction_state_change.etched {
            self.etched_rune(block_id.as_ref().map(|id| id.height), txid, &id);
        }

        for tx_in in transaction_state_change.inputs.iter() {
            // Spend inputs
            self.update_spendable_input(tx_in, true)?;
        }

        // Create new outputs
        for (vout, output) in transaction_state_change.outputs.iter().enumerate() {
            if output.runes.is_empty() && !self.settings.index_addresses {
                continue;
            }

            // Create new outputs
            let outpoint = OutPoint {
                txid,
                vout: vout as u32,
            };

            self.cache.set_tx_out(outpoint, output.clone());

            self.transfer_rune(
                block_id.as_ref().map(|id| id.height),
                txid,
                outpoint,
                output,
            );
        }

        // Save transaction state change
        self.cache
            .set_tx_state_changes(txid, transaction_state_change.clone());

        // Save rune transactions
        // Get all modified rune ids.
        let rune_ids = transaction_state_change.rune_ids();

        for rune_id in rune_ids {
            self.cache.add_rune_transaction(rune_id, txid);
        }

        if self.settings.index_bitcoin_transactions {
            self.cache.set_transaction(txid, transaction.clone());
        }

        if !self.cache.settings.mempool {
            self.cache
                .set_transaction_confirming_block(txid, block_id.unwrap());
        }

        if self.settings.index_addresses {
            self.update_script_pubkeys(txid, transaction);
        }

        if self.cache.settings.mempool {
            self.cache.set_mempool_tx(txid.clone());
        }

        Ok(())
    }

    fn update_script_pubkeys(&mut self, txid: Txid, transaction: &Transaction) -> () {
        if let Some(addr_updater) = self.address_updater.as_mut() {
            // skip coinbase inputs
            if !transaction.is_coinbase() {
                for input in &transaction.input {
                    // Add the spent outpoint to the address_updater
                    addr_updater.add_spent_outpoint(input.previous_output);
                }
            }

            // Add new outputs
            for (vout, txout) in transaction.output.iter().enumerate() {
                // (If you want to skip certain scriptPubKeys, that's up to you.)
                let outpoint = OutPoint {
                    txid,
                    vout: vout as u32,
                };

                addr_updater.add_new_outpoint(outpoint, txout.script_pubkey.clone());
            }
        }
    }

    fn update_spendable_input(&mut self, outpoint: &OutPoint, spent: bool) -> Result<()> {
        match self.cache.get_tx_out(outpoint) {
            Ok(tx_out) => {
                let mut tx_out = tx_out;
                tx_out.spent = spent;
                self.cache.set_tx_out(outpoint.clone(), tx_out);
                Ok(())
            }
            Err(StoreError::NotFound(_)) => {
                // We don't need to do anything.
                Ok(())
            }
            Err(e) => {
                return Err(TransactionUpdaterError::Store(e));
            }
        }
    }

    fn burn_rune(&mut self, height: Option<u64>, txid: Txid, rune_id: &RuneId, amount: u128) {
        self.cache.add_event(Event::RuneBurned {
            location: height.into(),
            txid,
            amount: amount as u128,
            rune_id: *rune_id,
        });
    }

    fn mint_rune(&mut self, height: Option<u64>, txid: Txid, rune_id: &RuneId, amount: u128) {
        self.cache.add_event(Event::RuneMinted {
            location: height.into(),
            txid,
            amount: amount as u128,
            rune_id: *rune_id,
        });
    }

    fn etched_rune(&mut self, height: Option<u64>, txid: Txid, id: &RuneId) {
        self.cache.add_event(Event::RuneEtched {
            location: height.into(),
            txid,
            rune_id: *id,
        });
    }

    fn transfer_rune(
        &mut self,
        height: Option<u64>,
        txid: Txid,
        outpoint: OutPoint,
        output: &TxOutEntry,
    ) {
        for rune_amount in output.runes.iter() {
            self.cache.add_event(Event::RuneTransferred {
                location: height.into(),
                txid,
                outpoint,
                rune_id: rune_amount.rune_id,
                amount: rune_amount.amount,
            });
        }
    }
}
