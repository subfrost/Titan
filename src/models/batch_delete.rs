use {
    bitcoin::{OutPoint, Txid},
    std::{collections::HashSet, fmt::Display},
};

#[derive(Debug, Clone)]
pub struct BatchDelete {
    pub tx_outs: HashSet<OutPoint>,
    pub tx_state_changes: HashSet<Txid>,
}

impl BatchDelete {
    pub fn new() -> Self {
        Self {
            tx_outs: HashSet::new(),
            tx_state_changes: HashSet::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.tx_outs.is_empty() && self.tx_state_changes.is_empty()
    }
}

impl Display for BatchDelete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "BatchDelete: {} tx_outs, {} tx_state_changes",
            self.tx_outs.len(),
            self.tx_state_changes.len()
        )
    }
}
