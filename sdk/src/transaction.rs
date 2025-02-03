use {
    crate::rune::RuneAmount,
    bitcoin::{ScriptBuf, TxIn},
    borsh::{BorshDeserialize, BorshSerialize},
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
