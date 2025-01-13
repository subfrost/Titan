use {
    super::cache::UpdaterCache,
    crate::{
        index::{bitcoin_rpc::BitcoinCoreRpcResultExt, Chain, StoreError},
        models::{Lot, RuneAmount, TransactionStateChange, TxOutEntry},
        util::IntoUsize,
    },
    bitcoin::{consensus::encode, OutPoint, Transaction, Txid},
    bitcoincore_rpc::{Client, RpcApi},
    ordinals::{Artifact, Edict, Height, Rune, RuneId, Runestone},
    std::collections::HashMap,
    thiserror::Error,
    tracing::info,
};

#[derive(Debug, Error)]
pub enum RuneParserError {
    #[error("store error {0}")]
    Store(#[from] StoreError),
    #[error("bitcoin rpc error {0}")]
    BitcoinRpc(#[from] bitcoincore_rpc::Error),
    #[error("encode error {0}")]
    Encode(#[from] encode::Error),
}

type Result<T> = std::result::Result<T, RuneParserError>;

pub(super) struct RuneParser<'a, 'client> {
    pub(super) client: &'client Client,
    pub(super) height: u32,
    pub(super) cache: &'a UpdaterCache,
    pub(super) minimum: Rune,
    pub(super) mempool: bool,
}

impl<'a, 'client> RuneParser<'a, 'client> {
    pub(super) fn new(
        client: &'client Client,
        chain: Chain,
        height: u32,
        mempool: bool,
        cache: &'a UpdaterCache,
    ) -> Result<Self> {
        let minimum = Rune::minimum_at_height(chain.into(), Height(height));

        Ok(Self {
            client,
            height,
            cache,
            minimum,
            mempool,
        })
    }

    pub(super) fn index_runes(
        &self,
        tx_index: u32,
        tx: &Transaction,
        _txid: Txid,
    ) -> Result<TransactionStateChange> {
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

            etched = self.etched(tx_index, tx, artifact)?;

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

        // update outpoint balances
        let mut tx_outs: Vec<TxOutEntry> = vec![];
        for (vout, balances) in allocated.into_iter().enumerate() {
            // increment burned balances
            if tx.output[vout].script_pubkey.is_op_return() {
                for (id, balance) in &balances {
                    *burned.entry(*id).or_default() += *balance;
                }
                continue;
            }

            let mut balances = balances.into_iter().collect::<Vec<(RuneId, Lot)>>();

            // Sort balances by id so tests can assert balances in a fixed order
            balances.sort();

            let mut tx_out = TxOutEntry {
                runes: vec![],
                spent: false,
            };

            for (id, balance) in balances {
                tx_out.runes.push(RuneAmount {
                    rune_id: id,
                    amount: balance.0,
                });
            }

            tx_outs.push(tx_out);
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
        };

        Ok(transaction_state_change)
    }

    fn etched(
        &self,
        tx_index: u32,
        tx: &Transaction,
        artifact: &Artifact,
    ) -> Result<Option<(RuneId, Rune)>> {
        if self.mempool {
            return Ok(None);
        }

        let rune = match artifact {
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

            if rune < self.minimum || rune.is_reserved() || !self.tx_commits_to_rune(tx, rune)? {
                return Ok(None);
            }
            rune
        } else {
            Rune::reserved(self.height.into(), tx_index)
        };

        Ok(Some((
            RuneId {
                block: self.height.into(),
                tx: tx_index,
            },
            rune,
        )))
    }

    fn mint(&self, id: RuneId) -> Result<Option<Lot>> {
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

        Ok(Some(Lot(amount)))
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
        // map of rune ID to un-allocated balance of that rune
        let mut unallocated: HashMap<RuneId, Lot> = HashMap::new();

        // increment unallocated runes with the runes in tx inputs
        for input in &tx.input {
            match self.cache.get_tx_out(&input.previous_output.into()) {
                Ok(tx_out) => {
                    for rune_amount in tx_out.runes {
                        *unallocated.entry(rune_amount.rune_id).or_default() += rune_amount.amount;
                    }
                }
                Err(e) => {
                    if e.is_not_found() {
                        continue;
                    }

                    return Err(e.into());
                }
            }
        }

        Ok(unallocated)
    }
}
