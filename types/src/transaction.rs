use {
    crate::{rune::RuneAmount, tx_in::TxIn, tx_out::SpentStatus, TxOutEntry},
    bitcoin::{BlockHash, ScriptBuf, Txid},
    borsh::{BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    std::io::{Read, Result, Write},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransactionStatus {
    pub confirmed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_hash: Option<BlockHash>,
}

impl TransactionStatus {
    pub fn unconfirmed() -> Self {
        Self {
            confirmed: false,
            block_height: None,
            block_hash: None,
        }
    }

    pub fn confirmed(block_height: u64, block_hash: BlockHash) -> Self {
        Self {
            confirmed: true,
            block_height: Some(block_height),
            block_hash: Some(block_hash),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub txid: Txid,
    pub version: i32,
    pub lock_time: u32,
    pub input: Vec<TxIn>,
    pub output: Vec<TxOut>,
    pub status: TransactionStatus,
}

impl
    From<(
        bitcoin::Transaction,
        TransactionStatus,
        Vec<Option<TxOutEntry>>,
        Vec<Option<TxOutEntry>>,
    )> for Transaction
{
    fn from(
        (transaction, status, prev_outputs, outputs): (
            bitcoin::Transaction,
            TransactionStatus,
            Vec<Option<TxOutEntry>>,
            Vec<Option<TxOutEntry>>,
        ),
    ) -> Self {
        Transaction {
            txid: transaction.compute_txid(),
            version: transaction.version.0,
            lock_time: transaction.lock_time.to_consensus_u32(),
            input: transaction
                .input
                .into_iter()
                .zip(prev_outputs.into_iter())
                .map(|(tx_in, prev_output)| TxIn::from((tx_in, prev_output)))
                .collect(),
            output: transaction
                .output
                .into_iter()
                .zip(outputs.into_iter())
                .map(|(tx_out, tx_out_entry)| {
                    let (runes, risky_runes, spent) = match tx_out_entry {
                        Some(tx_out_entry) => (
                            tx_out_entry.runes,
                            tx_out_entry.risky_runes,
                            tx_out_entry.spent,
                        ),
                        None => (vec![], vec![], SpentStatus::SpentUnknown),
                    };

                    let tx_out = TxOut {
                        value: tx_out.value.to_sat(),
                        script_pubkey: tx_out.script_pubkey,
                        runes,
                        risky_runes,
                        spent,
                    };

                    tx_out
                })
                .collect(),
            status,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TxOut {
    pub value: u64,
    pub script_pubkey: ScriptBuf,
    pub runes: Vec<RuneAmount>,
    pub risky_runes: Vec<RuneAmount>,
    pub spent: SpentStatus,
}

impl BorshSerialize for TxOut {
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
        BorshSerialize::serialize(&self.value, writer)?;
        let script_bytes = self.script_pubkey.as_bytes();
        BorshSerialize::serialize(&(script_bytes.len() as u32), writer)?;
        writer.write_all(script_bytes)?;
        BorshSerialize::serialize(&self.runes, writer)?;
        BorshSerialize::serialize(&self.risky_runes, writer)?;
        BorshSerialize::serialize(&self.spent, writer)?;
        Ok(())
    }
}

impl BorshDeserialize for TxOut {
    fn deserialize_reader<R: Read>(reader: &mut R) -> Result<Self> {
        let value = u64::deserialize_reader(reader)?;
        let script_len = u32::deserialize_reader(reader)? as usize;
        let mut script_bytes = vec![0u8; script_len];
        reader.read_exact(&mut script_bytes)?;
        let script_pubkey = ScriptBuf::from_bytes(script_bytes);
        let runes = Vec::<RuneAmount>::deserialize_reader(reader)?;
        let risky_runes = Vec::<RuneAmount>::deserialize_reader(reader)?;
        let spent = SpentStatus::deserialize_reader(reader)?;

        Ok(Self {
            value,
            script_pubkey,
            runes,
            risky_runes,
            spent,
        })
    }
}
