use {
    bitcoin::Txid,
    std::collections::HashMap,
    std::time::{Duration, Instant},
};

/// Provides a grace period during which recently added transactions cannot be removed.
///
/// # Overview
/// 1. When a transaction is first seen or broadcast (e.g. via ZMQ), call [`mark_as_added`].
///    This records the current time for that tx.
/// 2. Periodically, call [`expire_old_entries`] to remove entries that have aged beyond
///    the configured grace period.  
/// 3. When you want to remove transactions (e.g. if they are not listed by bitcoind’s mempool),
///    call [`filter_removable_txs`]. Any transaction *still* marked as added (i.e. within
///    the grace period) will **not** be returned for removal.
///
/// This helps avoid a race condition where bitcoind’s mempool might lag behind a ZMQ feed,
/// causing you to add a transaction only to remove it again too soon.
#[derive(Default)]
pub struct MempoolRemovalGrace {
    /// Records when a transaction was first marked as added.
    added_at: HashMap<Txid, Instant>,
    /// Duration to wait before allowing a "recently added" transaction to be removed.
    debounce_duration: Duration,
}

impl MempoolRemovalGrace {
    /// Create a new grace‐period manager with the specified duration.
    pub fn new(debounce_duration: Duration) -> Self {
        Self {
            added_at: HashMap::new(),
            debounce_duration,
        }
    }

    /// Mark a transaction as "added" at the current time.
    ///
    /// Typically called when a new tx is first seen (e.g. via ZMQ).
    /// Transactions remain in `added_at` until they are cleared by
    /// [`expire_old_entries`] or replaced by a subsequent logic.
    pub fn mark_as_added(&mut self, txid: Txid) {
        // Insert or overwrite with the new "added" timestamp.
        self.added_at.insert(txid, Instant::now());
    }

    /// Remove any "added" transactions whose timestamps are older than `debounce_duration`.
    ///
    /// After a transaction has been in the `added_at` map for longer than the
    /// grace period, we assume it's safe to consider it removable again.
    pub fn expire_old_entries(&mut self) {
        let now = Instant::now();
        self.added_at
            .retain(|_, &mut added_at| now.duration_since(added_at) < self.debounce_duration);
    }

    /// Given a list of txids that you plan to remove (e.g. not found in bitcoind’s mempool),
    /// this returns **only** those which are not "protected" by the grace period.
    ///
    /// In other words, if a tx is still in `added_at` (meaning it was added and
    /// hasn't yet aged out), it is **excluded** from the returned list of removable txs.
    ///
    /// You would typically call this right before actually removing those transactions
    /// from your local index, ensuring that newly added txs have a short “grace period.”
    pub fn filter_removable_txs(&self, to_remove: &[Txid]) -> Vec<Txid> {
        to_remove
            .iter()
            .filter_map(|txid| {
                // If we haven't recently added this tx, it's safe to remove.
                if self.added_at.contains_key(txid) {
                    None
                } else {
                    Some(*txid)
                }
            })
            .collect()
    }
}
