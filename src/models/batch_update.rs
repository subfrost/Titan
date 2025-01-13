use {
    super::Block,
    crate::models::{Inscription, InscriptionId, RuneEntry, TransactionStateChange, TxOutEntry},
    bitcoin::{BlockHash, OutPoint, Txid},
    ordinals::RuneId,
    std::{
        collections::{HashMap, HashSet},
        fmt::Display,
    },
};

#[derive(Debug, Clone)]
pub struct BatchUpdate {
    pub blocks: HashMap<BlockHash, Block>,
    pub block_hashes: HashMap<u64, BlockHash>,
    pub txouts: HashMap<OutPoint, TxOutEntry>,
    pub tx_state_changes: HashMap<Txid, TransactionStateChange>,
    pub rune_transactions: HashMap<RuneId, Vec<Txid>>,
    pub runes: HashMap<RuneId, RuneEntry>,
    pub rune_ids: HashMap<u128, RuneId>,
    pub rune_numbers: HashMap<u64, RuneId>,
    pub inscriptions: HashMap<InscriptionId, Inscription>,
    pub mempool_txs: HashSet<Txid>,
    pub rune_count: u64,
    pub block_count: u64,
}

impl BatchUpdate {
    pub fn new(rune_count: u64, block_count: u64) -> Self {
        Self {
            blocks: HashMap::new(),
            block_hashes: HashMap::new(),
            txouts: HashMap::new(),
            tx_state_changes: HashMap::new(),
            rune_transactions: HashMap::new(),
            runes: HashMap::new(),
            rune_ids: HashMap::new(),
            rune_numbers: HashMap::new(),
            inscriptions: HashMap::new(),
            mempool_txs: HashSet::new(),
            rune_count,
            block_count,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
            && self.block_hashes.is_empty()
            && self.txouts.is_empty()
            && self.tx_state_changes.is_empty()
            && self.rune_transactions.is_empty()
            && self.runes.is_empty()
            && self.rune_ids.is_empty()
            && self.rune_numbers.is_empty()
            && self.inscriptions.is_empty()
            && self.mempool_txs.is_empty()
    }
}

impl Display for BatchUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BatchUpdate: \
             counts: [blocks: {}, runes: {}] \
             added: [blocks: {}, txouts: {}, tx_changes: {}, \
             runes: txs {}/ runes {}/ ids {}, \
             inscriptions: {}]",
            self.block_count,
            self.rune_count,
            self.blocks.len(),
            self.txouts.len(),
            self.tx_state_changes.len(),
            self.rune_transactions.len(),
            self.runes.len(),
            self.rune_ids.len(),
            self.inscriptions.len(),
        )
    }
}
