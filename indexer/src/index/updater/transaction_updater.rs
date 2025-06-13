use {
    super::{address::AddressUpdater, cache::UpdaterCache},
    crate::{
        index::{inscription::index_rune_icon, Settings, StoreError},
        models::{BlockId, RuneEntry, TransactionStateChange},
    },
    bitcoin::{OutPoint, Transaction, Txid},
    ordinals::{Artifact, Etching, Rune, RuneId, Runestone, SpacedRune},
    std::sync::Arc,
    thiserror::Error,
    titan_types::{Event, MempoolEntry, SpenderReference, SpentStatus, TxOutEntry},
    tokio::sync::mpsc::error::SendError,
};

#[derive(Debug, Error)]
pub enum TransactionUpdaterError {
    #[error("store error {0}")]
    Store(#[from] StoreError),
    #[error("event sender error {0}")]
    EventSender(#[from] SendError<Event>),
    #[error("overflow in {0}")]
    Overflow(String),
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
    pub(super) settings: TransactionUpdaterSettings,
}

impl<'a> TransactionUpdater<'a> {
    pub(super) fn new(
        settings: TransactionUpdaterSettings,
        address_updater: Option<&'a mut AddressUpdater>,
    ) -> Result<Self> {
        Ok(Self {
            address_updater,
            settings,
        })
    }

    pub(super) fn save(
        &mut self,
        cache: &mut UpdaterCache,
        block_time: u32,
        block_id: Option<BlockId>,
        txid: Txid,
        transaction: &Transaction,
        transaction_state_change: &TransactionStateChange,
        mempool_entry: Option<MempoolEntry>,
    ) -> Result<()> {
        if let Some((id, rune)) = transaction_state_change.etched {
            self.etched_rune(
                cache,
                block_time,
                block_id.as_ref().map(|id| id.height),
                txid,
                transaction,
                &id,
                rune,
            )?;
        }

        // Update burned rune
        for (rune_id, amount) in transaction_state_change.burned.iter() {
            self.burn_rune(
                cache,
                block_id.as_ref().map(|id| id.height),
                txid,
                rune_id,
                amount.n(),
            )?;
        }

        // Add minted rune if any.
        if let Some(minted) = transaction_state_change.minted.as_ref() {
            self.mint_rune(
                cache,
                block_id.as_ref().map(|id| id.height),
                txid,
                &minted.rune_id,
                minted.amount,
            )?;
        }

        for (vin, tx_in) in transaction_state_change.inputs.iter().enumerate() {
            // Spend inputs
            self.update_spendable_input(
                cache,
                tx_in,
                SpentStatus::Spent(SpenderReference {
                    txid,
                    vin: vin as u32,
                }),
            )?;
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

            cache.set_tx_out(outpoint, output.clone());

            self.transfer_rune(
                cache,
                block_id.as_ref().map(|id| id.height),
                txid,
                outpoint,
                output,
            );
        }

        // Save transaction state change
        cache.set_tx_state_changes(txid, transaction_state_change.clone());

        // Save rune transactions
        // Get all modified rune ids.
        let rune_ids = transaction_state_change.rune_ids();

        for rune_id in rune_ids {
            cache.add_rune_transaction(rune_id, txid);
        }

        if self.settings.index_bitcoin_transactions {
            let tx_arc = Arc::new(transaction.clone());
            cache.set_transaction(txid, tx_arc);
        }

        if !cache.settings.mempool {
            cache.set_transaction_confirming_block(txid, block_id.unwrap());
        }

        if self.settings.index_addresses {
            self.update_script_pubkeys(txid, transaction);
        }

        if let Some(mempool_entry) = mempool_entry {
            cache.set_mempool_tx(txid, mempool_entry);
        }

        Ok(())
    }

    fn update_script_pubkeys(&mut self, txid: Txid, transaction: &Transaction) -> () {
        if let Some(addr_updater) = self.address_updater.as_mut() {
            // skip coinbase inputs
            if !transaction.is_coinbase() {
                for (vin, input) in transaction.input.iter().enumerate() {
                    // Add the spent outpoint to the address_updater
                    addr_updater.add_spent_outpoint(
                        input.previous_output,
                        SpenderReference {
                            txid,
                            vin: vin as u32,
                        },
                    );
                }
            }

            // Add new outputs
            for (vout, txout) in transaction.output.iter().enumerate() {
                let outpoint = OutPoint {
                    txid,
                    vout: vout as u32,
                };

                addr_updater.add_new_outpoint(outpoint, txout.script_pubkey.clone());
            }
        }
    }

    fn update_spendable_input(
        &mut self,
        cache: &mut UpdaterCache,
        outpoint: &OutPoint,
        spent: SpentStatus,
    ) -> Result<()> {
        match cache.get_tx_out(outpoint) {
            Ok(tx_out) => {
                let mut tx_out = tx_out;
                tx_out.spent = spent;
                cache.set_tx_out(outpoint.clone(), tx_out);
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

    fn burn_rune(
        &mut self,
        cache: &mut UpdaterCache,
        height: Option<u64>,
        txid: Txid,
        rune_id: &RuneId,
        amount: u128,
    ) -> Result<()> {
        self.update_burn_balance(cache, rune_id, amount as i128)?;

        cache.add_event(Event::RuneBurned {
            location: height.into(),
            txid,
            amount: amount as u128,
            rune_id: *rune_id,
        });

        Ok(())
    }

    fn mint_rune(
        &mut self,
        cache: &mut UpdaterCache,
        height: Option<u64>,
        txid: Txid,
        rune_id: &RuneId,
        amount: u128,
    ) -> Result<()> {
        self.increment_mint(cache, rune_id)?;

        cache.add_event(Event::RuneMinted {
            location: height.into(),
            txid,
            amount: amount as u128,
            rune_id: *rune_id,
        });

        Ok(())
    }

    fn update_burn_balance(
        &mut self,
        cache: &mut UpdaterCache,
        rune_id: &RuneId,
        amount: i128,
    ) -> Result<()> {
        let mut rune_entry = cache.get_rune(rune_id)?;

        if cache.settings.mempool {
            let result = rune_entry
                .pending_burns
                .checked_add_signed(amount)
                .ok_or(TransactionUpdaterError::Overflow("burn".to_string()))?;
            rune_entry.pending_burns = result;
        } else {
            let result = rune_entry
                .burned
                .checked_add_signed(amount)
                .ok_or(TransactionUpdaterError::Overflow("burn".to_string()))?;

            rune_entry.burned = result;
        }

        cache.set_rune(*rune_id, rune_entry);

        Ok(())
    }

    fn increment_mint(&mut self, cache: &mut UpdaterCache, rune_id: &RuneId) -> Result<()> {
        let mut rune_entry = cache.get_rune(rune_id)?;

        if cache.settings.mempool {
            let result = rune_entry.pending_mints.saturating_add_signed(1);
            rune_entry.pending_mints = result;
        } else {
            let result = rune_entry.mints.saturating_add_signed(1);

            rune_entry.mints = result;
        }

        cache.set_rune(*rune_id, rune_entry);

        Ok(())
    }

    fn etched_rune(
        &mut self,
        cache: &mut UpdaterCache,
        block_time: u32,
        height: Option<u64>,
        txid: Txid,
        transaction: &Transaction,
        id: &RuneId,
        rune: Rune,
    ) -> Result<()> {
        let artifact = Runestone::decipher(transaction).unwrap();
        self.create_rune_entry(cache, block_time, txid, transaction, *id, rune, &artifact)?;

        cache.add_event(Event::RuneEtched {
            location: height.into(),
            txid,
            rune_id: *id,
        });

        Ok(())
    }

    fn create_rune_entry(
        &mut self,
        cache: &mut UpdaterCache,
        block_time: u32,
        txid: Txid,
        transaction: &Transaction,
        id: RuneId,
        rune: Rune,
        artifact: &Artifact,
    ) -> Result<()> {
        let inscription = index_rune_icon(transaction, txid);

        if let Some((id, inscription)) = inscription.as_ref() {
            cache.set_inscription(id.clone(), inscription.clone());
        }

        let entry = match artifact {
            Artifact::Cenotaph(_) => RuneEntry {
                block: id.block,
                burned: 0,
                divisibility: 0,
                etching: txid,
                terms: None,
                mints: 0,
                number: cache.get_runes_count(),
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
                    number: cache.get_runes_count(),
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

        cache.set_rune(id, entry);
        cache.set_rune_id_number(cache.get_runes_count(), id);
        cache.set_rune_id(rune, id);
        cache.increment_runes_count();

        Ok(())
    }

    fn transfer_rune(
        &mut self,
        cache: &mut UpdaterCache,
        height: Option<u64>,
        txid: Txid,
        outpoint: OutPoint,
        output: &TxOutEntry,
    ) {
        for rune_amount in output.runes.iter() {
            cache.add_event(Event::RuneTransferred {
                location: height.into(),
                txid,
                outpoint,
                rune_id: rune_amount.rune_id,
                amount: rune_amount.amount,
            });
        }
    }
}
