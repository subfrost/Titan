use {
    super::cache::UpdaterCache,
    crate::{
        index::{
            bitcoin_rpc::BitcoinCoreRpcResultExt, inscription::index_rune_icon, Chain, StoreError,
        },
        models::{Lot, RuneEntry, TransactionStateChange},
        util::IntoUsize,
    },
    bitcoin::{consensus::encode, OutPoint, Transaction, Txid},
    bitcoincore_rpc::{Client, RpcApi},
    ordinals::{Artifact, Edict, Etching, Height, Rune, RuneId, Runestone, SpacedRune},
    types::{RuneAmount, TxOutEntry},
    std::collections::HashMap,
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum TransactionParserError {
    #[error("store error {0}")]
    Store(#[from] StoreError),
    #[error("bitcoin rpc error {0}")]
    BitcoinRpc(#[from] bitcoincore_rpc::Error),
    #[error("encode error {0}")]
    Encode(#[from] encode::Error),
    #[error("overflow in {0}")]
    Overflow(String),
}

type Result<T> = std::result::Result<T, TransactionParserError>;

pub(super) struct TransactionParser<'a, 'client> {
    pub(super) client: &'client Client,
    pub(super) height: u32,
    pub(super) cache: &'a mut UpdaterCache,
    pub(super) minimum_rune: Rune,
    pub(super) should_index_runes: bool,
    pub(super) mempool: bool,
}

impl<'a, 'client> TransactionParser<'a, 'client> {
    pub(super) fn new(
        client: &'client Client,
        chain: Chain,
        height: u32,
        mempool: bool,
        cache: &'a mut UpdaterCache,
    ) -> Result<Self> {
        let minimum_rune = Rune::minimum_at_height(chain.into(), Height(height));

        let min_rune_height = chain.first_rune_height();
        let should_index_runes = height >= min_rune_height;

        Ok(Self {
            client,
            height,
            cache,
            minimum_rune,
            should_index_runes,
            mempool,
        })
    }

    pub(super) fn pre_cache_tx_outs(&mut self, tx: &Vec<Transaction>) -> Result<()> {
        let outpoints: Vec<_> = tx
            .iter()
            .flat_map(|tx| tx.input.iter().map(|input| input.previous_output.into()))
            .collect();

        let tx_outs = self.cache.get_tx_outs(&outpoints)?;

        for (outpoint, tx_out) in tx_outs {
            self.cache.set_tx_out(outpoint, tx_out);
        }

        Ok(())
    }

    pub(super) fn parse(
        &mut self,
        block_time: u32,
        tx_index: u32,
        tx: &Transaction,
        txid: Txid,
    ) -> Result<TransactionStateChange> {
        let (allocated, minted, etched, burned) = if self.should_index_runes {
            self.parse_runes(block_time, tx_index, tx, txid)?
        } else {
            (
                vec![HashMap::new(); tx.output.len()],
                None,
                None,
                HashMap::new(),
            )
        };

        // update outpoint balances
        let mut tx_outs: Vec<TxOutEntry> = vec![];
        for (vout, balances) in allocated.into_iter().enumerate() {
            let mut balances = balances.into_iter().collect::<Vec<(RuneId, Lot)>>();

            // Sort balances by id so tests can assert balances in a fixed order
            balances.sort();

            let mut tx_out = TxOutEntry {
                runes: vec![],
                spent: false,
                value: tx.output[vout].value.to_sat(),
            };

            for (id, balance) in balances {
                tx_out.runes.push(RuneAmount {
                    rune_id: id,
                    amount: balance.0,
                });
            }

            tx_outs.push(tx_out);
        }

        for (id, balance) in burned.iter() {
            self.update_burn_balance(&id, balance.0 as i128)?;
        }

        let transaction_state_change = TransactionStateChange {
            burned,
            inputs: tx
                .input
                .iter()
                .map(|txin| OutPoint {
                    txid: txin.previous_output.txid,
                    vout: txin.previous_output.vout,
                })
                .collect(),
            outputs: tx_outs,
            etched,
            minted,
            is_coinbase: tx.is_coinbase(),
        };

        Ok(transaction_state_change)
    }

    fn parse_runes(
        &mut self,
        block_time: u32,
        tx_index: u32,
        tx: &Transaction,
        txid: Txid,
    ) -> Result<(
        Vec<HashMap<RuneId, Lot>>,
        Option<RuneAmount>,
        Option<(RuneId, Rune)>,
        HashMap<RuneId, Lot>,
    )> {
        let artifact = Runestone::decipher(tx);

        let mut unallocated = self.unallocated(tx)?;

        let mut allocated: Vec<HashMap<RuneId, Lot>> = vec![HashMap::new(); tx.output.len()];
        let mut minted: Option<RuneAmount> = None;
        let mut etched: Option<(RuneId, Rune)> = None;

        if let Some(artifact) = &artifact {
            if let Some(id) = artifact.mint() {
                if let Some(amount) = self.mint(id)? {
                    *unallocated.entry(id).or_default() += amount;
                    minted = Some(RuneAmount {
                        rune_id: id,
                        amount: amount.0,
                    });
                }
            }

            etched = self.etched(block_time, txid, tx_index, tx, artifact)?;

            if let Artifact::Runestone(runestone) = artifact {
                if let Some((id, ..)) = etched {
                    *unallocated.entry(id).or_default() +=
                        runestone.etching.unwrap().premine.unwrap_or_default();
                }

                for Edict { id, amount, output } in runestone.edicts.iter().copied() {
                    let amount = Lot(amount);

                    // edicts with output values greater than the number of outputs
                    // should never be produced by the edict parser
                    let output = usize::try_from(output).unwrap();
                    assert!(output <= tx.output.len());

                    let id = if id == RuneId::default() {
                        let Some((id, ..)) = etched else {
                            continue;
                        };

                        id
                    } else {
                        id
                    };

                    let Some(balance) = unallocated.get_mut(&id) else {
                        continue;
                    };

                    let mut allocate = |balance: &mut Lot, amount: Lot, output: usize| {
                        if amount > 0 {
                            *balance -= amount;
                            *allocated[output].entry(id).or_default() += amount;
                        }
                    };

                    if output == tx.output.len() {
                        // find non-OP_RETURN outputs
                        let destinations = tx
                            .output
                            .iter()
                            .enumerate()
                            .filter_map(|(output, tx_out)| {
                                (!tx_out.script_pubkey.is_op_return()).then_some(output)
                            })
                            .collect::<Vec<usize>>();

                        if !destinations.is_empty() {
                            if amount == 0 {
                                // if amount is zero, divide balance between eligible outputs
                                let amount = *balance / destinations.len() as u128;
                                let remainder =
                                    usize::try_from(*balance % destinations.len() as u128).unwrap();

                                for (i, output) in destinations.iter().enumerate() {
                                    allocate(
                                        balance,
                                        if i < remainder { amount + 1 } else { amount },
                                        *output,
                                    );
                                }
                            } else {
                                // if amount is non-zero, distribute amount to eligible outputs
                                for output in destinations {
                                    allocate(balance, amount.min(*balance), output);
                                }
                            }
                        }
                    } else {
                        // Get the allocatable amount
                        let amount = if amount == 0 {
                            *balance
                        } else {
                            amount.min(*balance)
                        };

                        allocate(balance, amount, output);
                    }
                }
            }
        }

        let mut burned: HashMap<RuneId, Lot> = HashMap::new();

        if let Some(Artifact::Cenotaph(_)) = artifact {
            for (id, balance) in unallocated {
                *burned.entry(id).or_default() += balance;
            }
        } else {
            let pointer = artifact
                .map(|artifact| match artifact {
                    Artifact::Runestone(runestone) => runestone.pointer,
                    Artifact::Cenotaph(_) => unreachable!(),
                })
                .unwrap_or_default();

            // assign all un-allocated runes to the default output, or the first non
            // OP_RETURN output if there is no default
            if let Some(vout) = pointer
                .map(|pointer| pointer.try_into().unwrap())
                .inspect(|&pointer| assert!(pointer < allocated.len()))
                .or_else(|| {
                    tx.output
                        .iter()
                        .enumerate()
                        .find(|(_vout, tx_out)| !tx_out.script_pubkey.is_op_return())
                        .map(|(vout, _tx_out)| vout)
                })
            {
                for (id, balance) in unallocated {
                    if balance > 0 {
                        *allocated[vout].entry(id).or_default() += balance;
                    }
                }
            } else {
                for (id, balance) in unallocated {
                    if balance > 0 {
                        *burned.entry(id).or_default() += balance;
                    }
                }
            }
        }

        for (vout, balances) in allocated.iter().enumerate() {
            // increment burned balances
            if tx.output[vout].script_pubkey.is_op_return() {
                for (id, balance) in balances {
                    *burned.entry(*id).or_default() += *balance;
                }
            }
        }

        Ok((allocated, minted, etched, burned))
    }

    fn etched(
        &mut self,
        block_time: u32,
        txid: Txid,
        tx_index: u32,
        tx: &Transaction,
        artifact: &Artifact,
    ) -> Result<Option<(RuneId, Rune)>> {
        if self.mempool {
            return Ok(None);
        }

        let rune: Option<Rune> = match artifact {
            Artifact::Runestone(runestone) => match runestone.etching {
                Some(etching) => etching.rune,
                None => return Ok(None),
            },
            Artifact::Cenotaph(cenotaph) => match cenotaph.etching {
                Some(rune) => Some(rune),
                None => return Ok(None),
            },
        };

        let rune = if let Some(rune) = rune {
            let rune_id = self.cache.get_rune_id(&rune);
            match rune_id {
                Ok(_) => return Ok(None),
                Err(e) => {
                    if !e.is_not_found() {
                        return Err(e.into());
                    }
                }
            }

            if rune < self.minimum_rune
                || rune.is_reserved()
                || !self.tx_commits_to_rune(tx, rune)?
            {
                return Ok(None);
            }
            rune
        } else {
            Rune::reserved(self.height.into(), tx_index)
        };

        let rune_id = RuneId {
            block: self.height.into(),
            tx: tx_index,
        };

        self.create_rune_entry(block_time, txid, tx, rune_id, rune, artifact)?;

        Ok(Some((rune_id, rune)))
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

    fn mint(&mut self, id: RuneId) -> Result<Option<Lot>> {
        let rune_entry = match self.cache.get_rune(&id) {
            Ok(rune_entry) => rune_entry,
            Err(err) => {
                if err.is_not_found() {
                    return Ok(None);
                } else {
                    return Err(err.into());
                }
            }
        };

        let Ok(amount) = rune_entry.mintable(self.height.into()) else {
            return Ok(None);
        };

        self.update_mints(&id, amount as i128)?;

        Ok(Some(Lot(amount)))
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
                .ok_or(TransactionParserError::Overflow("burn".to_string()))?;
            rune_entry.pending_burns = result;
        } else {
            let result = rune_entry
                .pending_burns
                .checked_add_signed(amount)
                .ok_or(TransactionParserError::Overflow("burn".to_string()))?;

            rune_entry.burned = result;
        }

        self.cache.set_rune(*rune_id, rune_entry);

        Ok(())
    }

    fn tx_commits_to_rune(&self, tx: &Transaction, rune: Rune) -> Result<bool> {
        let commitment = rune.commitment();

        for input in &tx.input {
            // extracting a tapscript does not indicate that the input being spent
            // was actually a taproot output. this is checked below, when we load the
            // output's entry from the database
            let Some(tapscript) = input.witness.tapscript() else {
                continue;
            };

            for instruction in tapscript.instructions() {
                // ignore errors, since the extracted script may not be valid
                let Ok(instruction) = instruction else {
                    break;
                };

                let Some(pushbytes) = instruction.push_bytes() else {
                    continue;
                };

                if pushbytes.as_bytes() != commitment {
                    continue;
                }

                let Some(tx_info) = self
                    .client
                    .get_raw_transaction_info(&input.previous_output.txid, None)
                    .into_option()?
                else {
                    panic!(
                        "can't get input transaction: {}",
                        input.previous_output.txid
                    );
                };

                let taproot = tx_info.vout[input.previous_output.vout.into_usize()]
                    .script_pub_key
                    .script()?
                    .is_p2tr();

                if !taproot {
                    continue;
                }

                let commit_tx_height = self
                    .client
                    .get_block_header_info(&tx_info.blockhash.unwrap())
                    .into_option()?
                    .unwrap()
                    .height;

                let confirmations = self
                    .height
                    .checked_sub(commit_tx_height.try_into().unwrap())
                    .unwrap()
                    + 1;

                if confirmations >= Runestone::COMMIT_CONFIRMATIONS.into() {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    fn unallocated(&self, tx: &Transaction) -> Result<HashMap<RuneId, Lot>> {
        let mut unallocated = HashMap::new();

        // 1) Collect all outpoints:
        let outpoints: Vec<_> = tx
            .input
            .iter()
            .map(|input| input.previous_output.into())
            .collect();

        // 2) Do a single multi-get in the cache:
        let tx_out_map = self.cache.get_tx_outs(&outpoints)?;

        // 3) Accumulate unallocated:
        for (_, tx_out) in tx_out_map {
            for rune_amount in tx_out.runes {
                *unallocated.entry(rune_amount.rune_id).or_default() += rune_amount.amount;
            }
        }

        Ok(unallocated)
    }
}
