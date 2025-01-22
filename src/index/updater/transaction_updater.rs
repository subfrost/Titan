use {
    super::{address::AddressUpdater, cache::UpdaterCache},
    crate::{
        index::{event::Event, inscription::index_rune_icon, Chain, Settings, StoreError},
        models::{RuneEntry, TransactionStateChange},
    },
    bitcoin::{OutPoint, ScriptBuf, Transaction, Txid},
    ordinals::{Artifact, Etching, Rune, RuneId, Runestone, SpacedRune},
    std::collections::HashSet,
    thiserror::Error,
    tokio::sync::mpsc::{self, error::SendError},
    tracing::info,
};

#[derive(Debug, Error)]
pub enum TransactionUpdaterError {
    #[error("store error {0}")]
    Store(#[from] StoreError),
    #[error("event sender error {0}")]
    EventSender(#[from] SendError<Event>),
    #[error("overflow in {0}")]
    Overflow(String),
    #[error("no artifact found: {0}: {1}")]
    NoArtifactFound(Txid, TransactionStateChange),
}

type Result<T> = std::result::Result<T, TransactionUpdaterError>;

#[derive(Debug)]
pub(super) struct TransactionUpdaterSettings {
    pub(super) index_addresses: bool,
    pub(super) chain: Chain,
}

impl From<Settings> for TransactionUpdaterSettings {
    fn from(settings: Settings) -> Self {
        Self {
            index_addresses: settings.index_addresses,
            chain: settings.chain,
        }
    }
}

pub(super) struct TransactionUpdater<'a> {
    pub(super) address_updater: Option<&'a mut AddressUpdater>,
    pub(super) event_sender: Option<&'a mpsc::Sender<Event>>,
    pub(super) cache: &'a mut UpdaterCache,

    pub(super) settings: TransactionUpdaterSettings,
    pub(super) modified_addresses: HashSet<ScriptBuf>,
}

impl<'a> TransactionUpdater<'a> {
    pub(super) fn new(
        cache: &'a mut UpdaterCache,
        event_sender: Option<&'a mpsc::Sender<Event>>,
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
        block_time: u32,
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

        if let Some((id, rune)) = transaction_state_change.etched {
            let artifact = Runestone::decipher(transaction).ok_or(
                TransactionUpdaterError::NoArtifactFound(txid, transaction_state_change.clone()),
            )?;

            self.create_rune_entry(block_time, txid, transaction, id, rune, &artifact)?;

            if let Some(sender) = self.event_sender {
                sender.blocking_send(Event::RuneEtched {
                    block_height: height,
                    txid,
                    rune_id: id,
                    in_mempool: false,
                })?;
            }
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
                            address,
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

    fn update_mints(&mut self, rune_id: &RuneId, amount: i128) -> Result<()> {
        let mut rune_entry = self.cache.get_rune(rune_id)?;

        if self.cache.settings.mempool {
            let result = rune_entry.pending_mints.saturating_add_signed(amount);
            rune_entry.pending_mints = result;
        } else {
            let result = rune_entry.mints.saturating_add_signed(amount);

            rune_entry.mints = result;
        }

        self.cache.set_rune(*rune_id, rune_entry);

        Ok(())
    }

    fn update_burn_balance(&mut self, rune_id: &RuneId, amount: i128) -> Result<()> {
        let mut rune_entry = self.cache.get_rune(rune_id)?;

        if self.cache.settings.mempool {
            let result = rune_entry
                .pending_burns
                .checked_add_signed(amount)
                .ok_or(TransactionUpdaterError::Overflow("burn".to_string()))?;
            rune_entry.pending_burns = result;
        } else {
            let result = rune_entry
                .pending_burns
                .checked_add_signed(amount)
                .ok_or(TransactionUpdaterError::Overflow("burn".to_string()))?;

            rune_entry.burned = result;
        }

        self.cache.set_rune(*rune_id, rune_entry);

        Ok(())
    }

    fn burn_rune(&mut self, height: u32, txid: Txid, rune_id: &RuneId, amount: u128) -> Result<()> {
        self.update_burn_balance(rune_id, amount as i128)?;

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
        self.update_mints(rune_id, 1)?;

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

    fn create_rune_entry(
        &mut self,
        block_time: u32,
        txid: Txid,
        transaction: &Transaction,
        id: RuneId,
        rune: Rune,
        artifact: &Artifact,
    ) -> Result<()> {
        self.cache.increment_runes_count();

        let inscription = index_rune_icon(transaction, txid);

        if let Some((id, inscription)) = inscription.as_ref() {
            self.cache.set_inscription(id.clone(), inscription.clone());
        }

        let entry = match artifact {
            Artifact::Cenotaph(_) => RuneEntry {
                block: id.block,
                burned: 0,
                divisibility: 0,
                etching: txid,
                terms: None,
                mints: 0,
                number: self.cache.get_runes_count(),
                premine: 0,
                spaced_rune: SpacedRune { rune, spacers: 0 },
                symbol: None,
                pending_burns: 0,
                pending_mints: 0,
                inscription_id: inscription.map(|(id, _)| id),
                timestamp: block_time.into(),
                turbo: false,
            },
            Artifact::Runestone(Runestone { etching, .. }) => {
                let Etching {
                    divisibility,
                    terms,
                    premine,
                    spacers,
                    symbol,
                    turbo,
                    ..
                } = etching.unwrap();

                RuneEntry {
                    block: id.block,
                    burned: 0,
                    divisibility: divisibility.unwrap_or_default(),
                    etching: txid,
                    terms,
                    mints: 0,
                    number: self.cache.get_runes_count(),
                    premine: premine.unwrap_or_default(),
                    spaced_rune: SpacedRune {
                        rune,
                        spacers: spacers.unwrap_or_default(),
                    },
                    symbol,
                    pending_burns: 0,
                    pending_mints: 0,
                    inscription_id: inscription.map(|(id, _)| id),
                    timestamp: block_time.into(),
                    turbo,
                }
            }
        };

        self.cache.set_rune(id, entry);
        self.cache
            .set_rune_id_number(self.cache.get_runes_count(), id);
        self.cache.set_rune_id(rune, id);

        Ok(())
    }
}
