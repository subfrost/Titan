use {
    super::cache::UpdaterCache,
    crate::{
        index::{bitcoin_rpc::BitcoinCoreRpcResultExt, Chain, StoreError},
        models::{Lot, TransactionStateChange},
        util::IntoUsize,
    },
    bitcoin::{consensus::encode, OutPoint, Transaction},
    bitcoincore_rpc::{Client, RpcApi},
    ordinals::{Artifact, Edict, Height, Rune, RuneId, Runestone},
    std::collections::HashMap,
    thiserror::Error,
    titan_types::{RuneAmount, TxOutEntry},
};

#[derive(Debug, Error)]
pub enum TransactionParserError {
    #[error("store error {0}")]
    Store(#[from] StoreError),
    #[error("bitcoin rpc error {0}")]
    BitcoinRpc(#[from] bitcoincore_rpc::Error),
    #[error("encode error {0}")]
    Encode(#[from] encode::Error),
}

type Result<T> = std::result::Result<T, TransactionParserError>;

pub(super) struct TransactionParser<'client> {
    pub(super) client: &'client Client,
    pub(super) height: u64,
    pub(super) minimum_rune: Rune,
    pub(super) should_index_runes: bool,
    pub(super) mempool: bool,
}

impl<'client> TransactionParser<'client> {
    pub(super) fn new(
        client: &'client Client,
        chain: Chain,
        height: u64,
        mempool: bool,
    ) -> Result<Self> {
        let minimum_rune = Rune::minimum_at_height(chain.into(), Height(height as u32));

        let min_rune_height = chain.first_rune_height() as u64;
        let should_index_runes = height >= min_rune_height;

        Ok(Self {
            client,
            height,
            minimum_rune,
            should_index_runes,
            mempool,
        })
    }

    pub(super) fn parse(
        &mut self,
        cache: &UpdaterCache,
        tx_index: u32,
        tx: &Transaction,
    ) -> Result<TransactionStateChange> {
        let (allocated, minted, etched, burned) = if self.should_index_runes {
            self.parse_runes(cache, tx_index, tx)?
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
        cache: &UpdaterCache,
        tx_index: u32,
        tx: &Transaction,
    ) -> Result<(
        Vec<HashMap<RuneId, Lot>>,
        Option<RuneAmount>,
        Option<(RuneId, Rune)>,
        HashMap<RuneId, Lot>,
    )> {
        let artifact = Runestone::decipher(tx);

        let mut unallocated = self.unallocated(cache, tx)?;

        let mut allocated: Vec<HashMap<RuneId, Lot>> = vec![HashMap::new(); tx.output.len()];
        let mut minted: Option<RuneAmount> = None;
        let mut etched: Option<(RuneId, Rune)> = None;

        if let Some(artifact) = &artifact {
            if let Some(id) = artifact.mint() {
                if let Some(amount) = self.mint(cache, id)? {
                    *unallocated.entry(id).or_default() += amount;
                    minted = Some(RuneAmount {
                        rune_id: id,
                        amount: amount.0,
                    });
                }
            }

            etched = self.etched(cache, tx_index, tx, artifact)?;

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
        cache: &UpdaterCache,
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
            let rune_id = cache.get_rune_id(&rune);
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
                || !self.tx_commits_to_rune(cache, tx, rune)?
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

        Ok(Some((rune_id, rune)))
    }

    fn mint(&mut self, cache: &UpdaterCache, id: RuneId) -> Result<Option<Lot>> {
        let rune_entry = match cache.get_rune(&id) {
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

    fn tx_commits_to_rune(
        &self,
        cache: &UpdaterCache,
        tx: &Transaction,
        rune: Rune,
    ) -> Result<bool> {
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

                match self.validate_commit_transaction_with_cache(cache, input.previous_output) {
                    Ok(true) => return Ok(true),
                    Ok(false) => continue,
                    Err(e) => {
                        if matches!(e, TransactionParserError::Store(StoreError::NotFound(_))) {
                            return self.validate_commit_transaction(input.previous_output);
                        } else {
                            return Err(e.into());
                        }
                    }
                }
            }
        }

        Ok(false)
    }

    fn validate_commit_transaction_with_cache(
        &self,
        cache: &UpdaterCache,
        outpoint: OutPoint,
    ) -> Result<bool> {
        let transaction = cache.get_transaction(outpoint.txid)?;

        let taproot = transaction.output[outpoint.vout.into_usize()]
            .script_pubkey
            .is_p2tr();

        if !taproot {
            return Ok(false);
        }

        let block_id = cache.get_transaction_confirming_block(outpoint.txid)?;

        let confirmations = self.height.checked_sub(block_id.height).unwrap() + 1;

        Ok(confirmations >= Runestone::COMMIT_CONFIRMATIONS.into())
    }

    fn validate_commit_transaction(&self, outpoint: OutPoint) -> Result<bool> {
        let Some(tx_info) = self
            .client
            .get_raw_transaction_info(&outpoint.txid, None)
            .into_option()?
        else {
            panic!("can't get input transaction: {}", outpoint.txid);
        };

        let taproot = tx_info.vout[outpoint.vout.into_usize()]
            .script_pub_key
            .script()?
            .is_p2tr();

        if !taproot {
            return Ok(false);
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

        Ok(confirmations >= Runestone::COMMIT_CONFIRMATIONS.into())
    }

    fn unallocated(&self, cache: &UpdaterCache, tx: &Transaction) -> Result<HashMap<RuneId, Lot>> {
        let mut unallocated = HashMap::new();

        // 1) Collect all outpoints:
        let outpoints: Vec<_> = tx
            .input
            .iter()
            .map(|input| input.previous_output.into())
            .collect();

        // 2) Do a single multi-get in the cache:
        let tx_out_map = cache.get_tx_outs(&outpoints)?;

        if !tx.is_coinbase() {
            assert_eq!(
                outpoints.len(),
                tx_out_map.len(),
                "outpoints and tx_out_map should have the same length in tx: {}",
                tx.compute_txid()
            );
        }

        // 3) Accumulate unallocated:
        for (_, tx_out) in tx_out_map {
            for rune_amount in tx_out.runes {
                *unallocated.entry(rune_amount.rune_id).or_default() += rune_amount.amount;
            }
        }

        Ok(unallocated)
    }
}
