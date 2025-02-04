use {
    crate::{RuneAmount, TxOutEntry},
    bitcoin::OutPoint,
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
    pub outpoint: OutPoint,
    pub value: u64,
    pub runes: Vec<RuneAmount>,
}

impl From<(OutPoint, TxOutEntry)> for AddressTxOut {
    fn from((outpoint, tx_out): (OutPoint, TxOutEntry)) -> Self {
        Self {
            outpoint,
            value: tx_out.value,
            runes: tx_out.runes,
        }
    }
}
