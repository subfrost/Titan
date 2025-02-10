use {
    super::RuneEntry,
    bitcoin::{OutPoint, ScriptBuf, Txid},
    ordinals::{Rune, RuneId},
    std::{collections::HashMap, fmt::Display},
    titan_types::{InscriptionId, TxOutEntry},
};

pub struct BatchRollback {
    pub runes_count: u64,

    pub rune_entry: HashMap<RuneId, RuneEntry>,
    pub txouts: HashMap<OutPoint, TxOutEntry>,
    pub script_pubkey_entry: HashMap<ScriptBuf, (Vec<OutPoint>, Vec<OutPoint>)>,

    pub outpoints_to_delete: Vec<OutPoint>,
    pub prev_outpoints_to_delete: Vec<OutPoint>,
    pub runes_to_delete: Vec<RuneId>,
    pub runes_ids_to_delete: Vec<Rune>,
    pub rune_numbers_to_delete: Vec<u64>,
    pub inscriptions_to_delete: Vec<InscriptionId>,
    pub delete_all_rune_transactions: Vec<RuneId>,
    pub txs_to_delete: Vec<Txid>,
}

impl BatchRollback {
    pub fn new(runes_count: u64) -> Self {
        Self {
            runes_count,
            rune_entry: HashMap::new(),
            txouts: HashMap::new(),
            script_pubkey_entry: HashMap::new(),
            outpoints_to_delete: Vec::new(),
            prev_outpoints_to_delete: Vec::new(),
            runes_to_delete: Vec::new(),
            runes_ids_to_delete: Vec::new(),
            rune_numbers_to_delete: Vec::new(),
            inscriptions_to_delete: Vec::new(),
            delete_all_rune_transactions: Vec::new(),
            txs_to_delete: Vec::new(),
        }
    }
}

impl Display for BatchRollback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BatchRollback: \
             counts: [runes: {}, txouts: {}, script_pubkeys: {}]
             outpoints_to_delete: {}, prev_outpoints_to_delete: {}, runes_to_delete: {}, \
             runes_ids_to_delete: {}, rune_numbers_to_delete: {}, inscriptions_to_delete: {}, \
             delete_all_rune_transactions: {}, txs_to_delete: {}
             ",
            self.runes_count,
            self.txouts.len(),
            self.script_pubkey_entry.len(),
            self.outpoints_to_delete.len(),
            self.prev_outpoints_to_delete.len(),
            self.runes_to_delete.len(),
            self.runes_ids_to_delete.len(),
            self.rune_numbers_to_delete.len(),
            self.inscriptions_to_delete.len(),
            self.delete_all_rune_transactions.len(),
            self.txs_to_delete.len()
        )
    }
}
