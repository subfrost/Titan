use {
    bitcoin::Txid,
    std::collections::HashMap,
    std::time::{Duration, Instant},
};

#[derive(Default)]
pub struct MempoolDebouncer {
    removed_at: HashMap<Txid, Instant>,
    added_at: HashMap<Txid, Instant>,
    debounce_duration: Duration,
}

impl MempoolDebouncer {
    pub fn new(debounce_duration: Duration) -> Self {
        Self {
            removed_at: HashMap::new(),
            added_at: HashMap::new(),
            debounce_duration,
        }
    }

    /// Adds a transaction to the recently removed list with a timestamp.
    pub fn handle_removed(&mut self, txid: Txid) {
        self.added_at.remove(&txid);

        if self.removed_at.contains_key(&txid) {
            return;
        }
        self.removed_at.insert(txid, Instant::now());
    }

    /// Adds a transaction to the recently added list with a timestamp.
    pub fn handle_added(&mut self, txid: Txid) {
        self.removed_at.remove(&txid);

        if self.added_at.contains_key(&txid) {
            return;
        }
        self.added_at.insert(txid, Instant::now());
    }

    pub fn handle_indexed(&mut self, txid: Txid) {
        self.removed_at.remove(&txid);
        self.added_at.remove(&txid);
    }

    /// Returns the transactions that have been removed and added within the debounce window.
    pub fn get_tx_to_update(&mut self, skip_debounce: bool) -> (Vec<Txid>, Vec<Txid>) {
        let expired_txids: Vec<Txid> = self
            .removed_at
            .iter()
            .filter(|(_, &timestamp)| {
                if skip_debounce {
                    true
                } else {
                    Instant::now().duration_since(timestamp) >= self.debounce_duration
                }
            })
            .map(|(txid, _)| txid.clone())
            .collect();

        let added_txids: Vec<Txid> = self
            .added_at
            .iter()
            .filter(|(_, &timestamp)| {
                if skip_debounce {
                    true
                } else {
                    Instant::now().duration_since(timestamp) >= self.debounce_duration
                }
            })
            .map(|(txid, _)| txid.clone())
            .collect();

        for txid in expired_txids.iter() {
            self.removed_at.remove(txid);
        }

        for txid in added_txids.iter() {
            self.added_at.remove(txid);
        }

        (expired_txids, added_txids)
    }
}
