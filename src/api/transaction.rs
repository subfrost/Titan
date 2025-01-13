use {
    crate::models::RuneAmount,
    bitcoin::{ScriptBuf, TxIn},
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub input: Vec<TxIn>,
    pub output: Vec<TxOut>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TxOut {
    pub value: u64,
    pub script_pubkey: ScriptBuf,
    pub runes: Vec<RuneAmount>,
}
