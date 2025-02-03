use {
    crate::index::{updater::cache::UpdaterCache, StoreError},
    bitcoin::{OutPoint, ScriptBuf},
    crate::models::ScriptPubkeyEntry,
    std::collections::{HashMap, HashSet},
};

#[derive(Default)]
pub struct AddressUpdater {
    /// All outpoints newly created in this block: (outpoint, script_pubkey)
    new_outpoints: Vec<(OutPoint, ScriptBuf)>,

    /// All outpoints spent in this block (except coinbase).
    spent_outpoints: Vec<OutPoint>,
}

impl AddressUpdater {
    pub fn new() -> Self {
        Self {
            new_outpoints: Vec::new(),
            spent_outpoints: Vec::new(),
        }
    }

    /// Remember a newly created outpoint -> scriptPubKey
    pub fn add_new_outpoint(&mut self, outpoint: OutPoint, script_pubkey: ScriptBuf) {
        self.new_outpoints.push((outpoint, script_pubkey));
    }

    /// Remember a spent outpoint
    pub fn add_spent_outpoint(&mut self, outpoint: OutPoint) {
        self.spent_outpoints.push(outpoint);
    }

    pub fn batch_update_script_pubkey(&self, cache: &mut UpdaterCache) -> Result<(), StoreError> {
        if cache.settings.mempool {
            self.batch_update_script_pubkeys_for_mempool(cache)?;
        } else {
            self.batch_update_script_pubkeys_for_block(cache)?;
        }

        Ok(())
    }

    /// Do a single batched pass to update scriptPubKey entries for
    /// all outpoints in this block. **Important**: we first add
    /// the new outpoints, then remove the spent outpoints. That way,
    /// if an outpoint is created and spent within the same block,
    /// we can find it in ephemeral memory.
    fn batch_update_script_pubkeys_for_block(
        &self,
        cache: &mut UpdaterCache,
    ) -> Result<(), StoreError> {
        // ------------------------------------------------------
        // 1. Map all spent outpoints to their scriptPubKey
        // ------------------------------------------------------
        let spent_map = if !self.spent_outpoints.is_empty() {
            // For any spent outpoint that wasn't created in the same block,
            // we need to fetch from DB or ephemeral memory in UpdaterCache.
            cache.get_outpoints_to_script_pubkey(self.spent_outpoints.clone(), true)?
        } else {
            HashMap::new()
        };

        // ------------------------------------------------------
        // 2. Build a combined map: scriptPubKey -> (Vec of new, Vec of spent)
        // ------------------------------------------------------
        //   We'll store new outpoints in (0) and spent outpoints in (1).
        //   This allows us to do all grouping in one structure.
        let mut spk_map: HashMap<ScriptBuf, (Vec<OutPoint>, Vec<OutPoint>)> = HashMap::new();

        // a) Insert new outpoints
        for (outpoint, script_pubkey) in &self.new_outpoints {
            let entry = spk_map.entry(script_pubkey.clone()).or_default();
            entry.0.push(*outpoint); // new
        }

        // b) Insert spent outpoints
        for (outpoint, script_pubkey) in spent_map {
            let entry = spk_map.entry(script_pubkey).or_default();
            entry.1.push(outpoint); // spent
        }

        // If there's nothing to update, return early
        if spk_map.is_empty() {
            return Ok(());
        }

        // ------------------------------------------------------
        // 3. Fetch existing ScriptPubkeyEntry's in one DB call
        // ------------------------------------------------------
        let all_spks: Vec<ScriptBuf> = spk_map.keys().cloned().collect();
        let mut entries = cache.get_script_pubkey_entries(&all_spks)?;

        // ------------------------------------------------------
        // 4. Update each script_pubkey's entry: add new, remove spent
        // ------------------------------------------------------
        //   For large 'spent' vectors, you can pre-build a HashSet
        //   for O(N) removal instead of O(N*M).
        for (spk, (new_ops, spent_ops)) in &spk_map {
            // Find or create a new ScriptPubkeyEntry
            let entry = entries
                .entry(spk.clone())
                .or_insert_with(ScriptPubkeyEntry::default);

            // Add newly created outpoints
            entry.utxos.extend(new_ops.iter().copied());

            // Remove spent outpoints
            if !spent_ops.is_empty() {
                // For efficiency, remove in O(N) if we convert spent_ops to a HashSet
                let spent_set: HashSet<OutPoint> = spent_ops.iter().copied().collect();
                entry.utxos.retain(|op| !spent_set.contains(op));
            }
        }

        // ------------------------------------------------------
        // 5. Write all updated entries back to DB in a single pass
        // ------------------------------------------------------
        for (spk, entry) in entries {
            cache.set_script_pubkey_entry(spk, entry);
        }

        // ------------------------------------------------------
        // 6. Update OutPoint -> ScriptPubKey mapping for newly created outpoints
        // ------------------------------------------------------
        if !self.new_outpoints.is_empty() {
            cache.batch_set_outpoints_to_script_pubkey(&self.new_outpoints);
        }

        Ok(())
    }

    fn batch_update_script_pubkeys_for_mempool(
        &self,
        cache: &mut UpdaterCache,
    ) -> Result<(), StoreError> {
        let mut spk_map: HashMap<ScriptBuf, Vec<OutPoint>> = HashMap::new();

        // a) Insert new outpoints
        for (outpoint, script_pubkey) in &self.new_outpoints {
            let entry = spk_map.entry(script_pubkey.clone()).or_default();
            entry.push(*outpoint); // new
        }

        let mut entries = cache.get_script_pubkey_entries(&spk_map.keys().cloned().collect())?;

        for (spk, outpoints) in spk_map {
            // Find or create a new ScriptPubkeyEntry
            let entry = entries
                .entry(spk.clone())
                .or_insert_with(ScriptPubkeyEntry::default);

            // Add newly created outpoints
            entry.utxos.extend(outpoints.iter().copied());
        }

        for (spk, entry) in entries {
            cache.set_script_pubkey_entry(spk, entry);
        }

        cache.batch_set_outpoints_to_script_pubkey(&self.new_outpoints);

        // b) Insert spent outpoints
        cache.batch_set_spent_outpoints_in_mempool(self.spent_outpoints.clone());

        Ok(())
    }
}
