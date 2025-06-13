use {
    bitcoin::{OutPoint, Txid},
    std::{collections::HashSet, fmt::Display},
};

#[derive(Debug, Clone)]
pub struct BatchDelete {
    pub tx_outs: HashSet<OutPoint>,
    pub script_pubkeys_outpoints: HashSet<OutPoint>,
    pub spent_outpoints_in_mempool: HashSet<OutPoint>,
    pub tx_state_changes: HashSet<Txid>,
}

impl BatchDelete {
    pub fn new() -> Self {
        Self {
            tx_outs: HashSet::new(),
            script_pubkeys_outpoints: HashSet::new(),
            spent_outpoints_in_mempool: HashSet::new(),
            tx_state_changes: HashSet::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tx_outs.is_empty()
            && self.script_pubkeys_outpoints.is_empty()
            && self.spent_outpoints_in_mempool.is_empty()
            && self.tx_state_changes.is_empty()
    }

    /// Empties all collections while preserving their allocated capacity.
    pub fn clear(&mut self) {
        self.tx_outs.clear();
        self.script_pubkeys_outpoints.clear();
        self.spent_outpoints_in_mempool.clear();
        self.tx_state_changes.clear();
    }
}

impl Display for BatchDelete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BatchDelete: {} tx_outs, {} script_pubkeys_outpoints, {} spent_outpoints_in_mempool, {} tx_state_changes",
            self.tx_outs.len(),
            self.script_pubkeys_outpoints.len(),
            self.spent_outpoints_in_mempool.len(),
            self.tx_state_changes.len()
        )
    }
}
