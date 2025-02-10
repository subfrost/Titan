use {
    crate::{transaction::TransactionStatus, RuneAmount, TxOutEntry},
    bitcoin::{OutPoint, Txid},
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct AddressData {
    pub value: u64,
    pub runes: Vec<RuneAmount>,
    pub outputs: Vec<AddressTxOut>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddressTxOut {
    pub txid: Txid,
    pub vout: u32,
    pub value: u64,
    pub runes: Vec<RuneAmount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<TransactionStatus>,
}

impl From<(OutPoint, TxOutEntry, Option<TransactionStatus>)> for AddressTxOut {
    fn from((outpoint, tx_out, status): (OutPoint, TxOutEntry, Option<TransactionStatus>)) -> Self {
        Self {
            txid: outpoint.txid,
            vout: outpoint.vout,
            value: tx_out.value,
            runes: tx_out.runes,
            status,
        }
    }
}
