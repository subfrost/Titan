use {
    crate::{tx_in::TxIn, tx_out::SpentStatus, TxOut},
    bitcoin::{constants::WITNESS_SCALE_FACTOR, BlockHash, Txid},
    serde::{Deserialize, Serialize},
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
    pub size: u64,
    pub weight: u64,
}

impl Transaction {
    pub fn vbytes(&self) -> u64 {
        (self.weight + WITNESS_SCALE_FACTOR as u64 - 1) / WITNESS_SCALE_FACTOR as u64
    }
}

impl
    From<(
        bitcoin::Transaction,
        TransactionStatus,
        Vec<Option<TxOut>>,
        Vec<Option<TxOut>>,
    )> for Transaction
{
    fn from(
        (transaction, status, prev_outputs, outputs): (
            bitcoin::Transaction,
            TransactionStatus,
            Vec<Option<TxOut>>,
            Vec<Option<TxOut>>,
        ),
    ) -> Self {
        Transaction {
            size: transaction.total_size() as u64,
            weight: transaction.weight().to_wu(),
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
