use {
    super::{address::AddressUpdater, cache::UpdaterCache},
    crate::{
        index::{Chain, Settings, StoreError},
        models::TransactionStateChange,
    },
    bitcoin::{OutPoint, ScriptBuf, Transaction, Txid},
    ordinals::RuneId,
    std::collections::HashSet,
    thiserror::Error,
    tokio::sync::mpsc::{self, error::SendError},
    types::{Event, TxOutEntry},
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
    pub(super) chain: Chain,
}

impl From<Settings> for TransactionUpdaterSettings {
    fn from(settings: Settings) -> Self {
        Self {
            index_addresses: settings.index_addresses,
            index_bitcoin_transactions: settings.index_bitcoin_transactions,
            chain: settings.chain,
        }
    }
}

pub(super) struct TransactionUpdater<'a> {
    pub(super) address_updater: Option<&'a mut AddressUpdater>,
    pub(super) event_sender: &'a Option<mpsc::Sender<Event>>,
    pub(super) cache: &'a mut UpdaterCache,

    pub(super) settings: TransactionUpdaterSettings,
    pub(super) modified_addresses: HashSet<ScriptBuf>,
}

impl<'a> TransactionUpdater<'a> {
    pub(super) fn new(
        cache: &'a mut UpdaterCache,
        event_sender: &'a Option<mpsc::Sender<Event>>,
        settings: TransactionUpdaterSettings,
        address_updater: Option<&'a mut AddressUpdater>,
    ) -> Result<Self> {
        Ok(Self {
            address_updater,
            event_sender,
            cache,
            settings,
            modified_addresses: HashSet::new(),
        })
    }

    pub(super) fn save(
        &mut self,
        height: u32,
        txid: Txid,
        transaction: &Transaction,
        transaction_state_change: &TransactionStateChange,
    ) -> Result<()> {
        // Update burned rune
        for (rune_id, amount) in transaction_state_change.burned.iter() {
            self.burn_rune(height, txid, rune_id, amount.n())?;
        }

        // Add minted rune if any.
        if let Some(minted) = transaction_state_change.minted.as_ref() {
            self.mint_rune(height, txid, &minted.rune_id, minted.amount)?;
        }

        if let Some((id, _)) = transaction_state_change.etched {
            self.etched_rune(height, txid, &id)?;
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

            self.transfer_rune(height, txid, outpoint, output)?;
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

        if self.settings.index_addresses {
            self.update_script_pubkeys(txid, transaction, transaction_state_change);
        }

        if self.cache.settings.mempool {
            self.cache.set_mempool_tx(txid.clone());
        }

        Ok(())
    }

    pub(super) fn notify_script_pubkeys_updated(&mut self, height: u32) -> Result<()> {
        if let Some(sender) = self.event_sender {
            for script_pubkey in self.modified_addresses.iter() {
                let address = Chain::address_from_script(self.settings.chain, script_pubkey);

                match address {
                    Ok(address) => {
                        sender.blocking_send(Event::AddressModified {
                            block_height: height,
                            address: address.to_string(),
                            in_mempool: self.cache.settings.mempool,
                        })?;
                    }
                    Err(_) => {
                        // Ignore unknown script pubkeys.
                    }
                }
            }
        }

        self.modified_addresses.clear();
        Ok(())
    }

    fn update_script_pubkeys(
        &mut self,
        txid: Txid,
        transaction: &Transaction,
        transaction_state_change: &TransactionStateChange,
    ) -> () {
        // skip coinbase inputs
        if !transaction_state_change.is_coinbase {
            for input_outpoint in &transaction_state_change.inputs {
                // Add the spent outpoint to the address_updater
                if let Some(addr_updater) = self.address_updater.as_mut() {
                    addr_updater.add_spent_outpoint(*input_outpoint);
                }
            }
        }

        // Add new outputs
        for (vout, txout) in transaction.output.iter().enumerate() {
            // (If you want to skip certain scriptPubKeys, that's up to you.)
            let outpoint = OutPoint {
                txid,
                vout: vout as u32,
            };
            if let Some(addr_updater) = self.address_updater.as_mut() {
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

    fn burn_rune(&mut self, height: u32, txid: Txid, rune_id: &RuneId, amount: u128) -> Result<()> {
        if let Some(sender) = self.event_sender {
            sender.blocking_send(Event::RuneBurned {
                block_height: height,
                txid,
                amount: amount as u128,
                rune_id: *rune_id,
                in_mempool: self.cache.settings.mempool,
            })?;
        }

        Ok(())
    }

    fn mint_rune(&mut self, height: u32, txid: Txid, rune_id: &RuneId, amount: u128) -> Result<()> {
        if let Some(sender) = self.event_sender {
            sender.blocking_send(Event::RuneMinted {
                block_height: height,
                txid,
                amount: amount as u128,
                rune_id: *rune_id,
                in_mempool: self.cache.settings.mempool,
            })?;
        }

        Ok(())
    }

    fn etched_rune(&mut self, height: u32, txid: Txid, id: &RuneId) -> Result<()> {
        if let Some(sender) = self.event_sender {
            sender.blocking_send(Event::RuneEtched {
                block_height: height,
                txid,
                rune_id: *id,
                in_mempool: false,
            })?;
        }

        Ok(())
    }

    fn transfer_rune(
        &mut self,
        height: u32,
        txid: Txid,
        outpoint: OutPoint,
        output: &TxOutEntry,
    ) -> Result<()> {
        if let Some(sender) = self.event_sender {
            for rune_amount in output.runes.iter() {
                sender.blocking_send(Event::RuneTransferred {
                    block_height: height,
                    txid,
                    outpoint,
                    rune_id: rune_amount.rune_id,
                    amount: rune_amount.amount,
                    in_mempool: self.cache.settings.mempool,
                })?;
            }
        }

        Ok(())
    }
}
