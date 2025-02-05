use {
    crate::rune::RuneAmount,
    bitcoin::{ScriptBuf, TxIn},
    borsh::{BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct TransactionStatus {
    pub height: u64,
    pub hash: String,
    pub confirmed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub version: i32,
    pub lock_time: u32,
    pub input: Vec<TxIn>,
    pub output: Vec<TxOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TransactionStatus>,
}

impl From<bitcoin::Transaction> for Transaction {
    fn from(transaction: bitcoin::Transaction) -> Self {
        Transaction {
            version: transaction.version.0,
            lock_time: transaction.lock_time.to_consensus_u32(),
            input: transaction.input,
            output: transaction
                .output
                .iter()
                .map(|tx_out| TxOut {
                    value: tx_out.value.to_sat(),
                    script_pubkey: tx_out.script_pubkey.clone(),
                    runes: vec![],
                })
                .collect(),
            status: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TxOut {
    pub value: u64,
    pub script_pubkey: ScriptBuf,
    pub runes: Vec<RuneAmount>,
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct TxOutEntry {
    pub runes: Vec<RuneAmount>,
    pub value: u64,
    pub spent: bool,
}

impl TxOutEntry {
    pub fn has_runes(&self) -> bool {
        !self.runes.is_empty()
    }
}
