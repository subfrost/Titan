use bitcoin::{ScriptBuf, Sequence, Witness};
use serde::{Deserialize, Serialize};

use crate::{RuneAmount, SerializedOutPoint, TxOutEntry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviousOutputData {
    pub value: u64,
    pub runes: Vec<RuneAmount>,
    pub risky_runes: Vec<RuneAmount>,
}

impl From<TxOutEntry> for PreviousOutputData {
    fn from(tx_out_entry: TxOutEntry) -> Self {
        Self {
            value: tx_out_entry.value,
            runes: tx_out_entry.runes,
            risky_runes: tx_out_entry.risky_runes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxIn {
    pub previous_output: SerializedOutPoint,
    pub script_sig: ScriptBuf,
    pub sequence: Sequence,
    pub witness: Witness,
    pub previous_output_data: Option<PreviousOutputData>,
}

impl From<(bitcoin::TxIn, Option<TxOutEntry>)> for TxIn {
    fn from((tx_in, previous_output_data): (bitcoin::TxIn, Option<TxOutEntry>)) -> Self {
        Self {
            previous_output: tx_in.previous_output.into(),
            script_sig: tx_in.script_sig,
            sequence: tx_in.sequence,
            witness: tx_in.witness,
            previous_output_data: previous_output_data.map(Into::into),
        }
    }
}
