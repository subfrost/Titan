use {
    super::{BlockId, Inscription, RuneEntry, ScriptPubkeyEntry, TransactionStateChange},
    bitcoin::{BlockHash, OutPoint, ScriptBuf, Transaction, Txid},
    ordinals::RuneId,
    std::{
        collections::{HashMap, HashSet},
        fmt::Display,
    },
    titan_types::{Block, InscriptionId, TxOutEntry},
};

#[derive(Debug, Clone)]
pub struct BatchUpdate {
    pub script_pubkeys: HashMap<ScriptBuf, ScriptPubkeyEntry>,
    pub script_pubkeys_outpoints: HashMap<OutPoint, ScriptBuf>,
    pub spent_outpoints_in_mempool: HashSet<OutPoint>,
    pub blocks: HashMap<BlockHash, Block>,
    pub block_hashes: HashMap<u64, BlockHash>,
    pub txouts: HashMap<OutPoint, TxOutEntry>,
    pub tx_state_changes: HashMap<Txid, TransactionStateChange>,
    pub rune_transactions: HashMap<RuneId, Vec<Txid>>,
    pub runes: HashMap<RuneId, RuneEntry>,
    pub rune_ids: HashMap<u128, RuneId>,
    pub rune_numbers: HashMap<u64, RuneId>,
    pub inscriptions: HashMap<InscriptionId, Inscription>,
    pub transactions: HashMap<Txid, Transaction>,
    pub transaction_confirming_block: HashMap<Txid, BlockId>,
    pub mempool_txs: HashSet<Txid>,
    pub rune_count: u64,
    pub block_count: u64,
    pub purged_blocks_count: u64,
}

impl BatchUpdate {
    pub fn new(rune_count: u64, block_count: u64, purged_blocks_count: u64) -> Self {
        Self {
            script_pubkeys: HashMap::new(),
            script_pubkeys_outpoints: HashMap::new(),
            spent_outpoints_in_mempool: HashSet::new(),
            blocks: HashMap::new(),
            block_hashes: HashMap::new(),
            txouts: HashMap::new(),
            tx_state_changes: HashMap::new(),
            rune_transactions: HashMap::new(),
            runes: HashMap::new(),
            rune_ids: HashMap::new(),
            rune_numbers: HashMap::new(),
            inscriptions: HashMap::new(),
            transactions: HashMap::new(),
            transaction_confirming_block: HashMap::new(),
            mempool_txs: HashSet::new(),
            rune_count,
            block_count,
            purged_blocks_count,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.script_pubkeys.is_empty()
            && self.script_pubkeys_outpoints.is_empty()
            && self.spent_outpoints_in_mempool.is_empty()
            && self.blocks.is_empty()
            && self.block_hashes.is_empty()
            && self.txouts.is_empty()
            && self.tx_state_changes.is_empty()
            && self.rune_transactions.is_empty()
            && self.runes.is_empty()
            && self.rune_ids.is_empty()
            && self.rune_numbers.is_empty()
            && self.inscriptions.is_empty()
            && self.mempool_txs.is_empty()
            && self.transactions.is_empty()
            && self.transaction_confirming_block.is_empty()
    }
}

impl Display for BatchUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BatchUpdate: \
             counts: [blocks: {}, runes: {}, purged_blocks: {}] \
             added: [blocks: {}, txouts: {}, tx_changes: {}, \
             addresses: {} , address_outpoints: {}, \
             spent_outpoints_in_mempool: {}, \
             runes: txs {}/ runes {}/ ids {}, \
             inscriptions: {}, \
             transactions: {}, \
             transaction_confirming_block: {}]",
            self.block_count,
            self.rune_count,
            self.purged_blocks_count,
            self.blocks.len(),
            self.txouts.len(),
            self.tx_state_changes.len(),
            self.script_pubkeys.len(),
            self.script_pubkeys_outpoints.len(),
            self.spent_outpoints_in_mempool.len(),
            self.rune_transactions.len(),
            self.runes.len(),
            self.rune_ids.len(),
            self.inscriptions.len(),
            self.transactions.len(),
            self.transaction_confirming_block.len(),
        )
    }
}
