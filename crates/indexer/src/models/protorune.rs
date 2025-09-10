use {
    crate::db::entry::Entry,
    borsh::{BorshDeserialize, BorshSerialize},
    protorune_support::balance_sheet::{BalanceSheet, BalanceSheetOperations, ProtoruneRuneId},
    serde::{Deserialize, Serialize},
    std::collections::BTreeMap,
};

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ProtoruneBalanceSheet {
    pub balances: BTreeMap<ProtoruneRuneId, u128>,
}


impl BalanceSheetOperations for ProtoruneBalanceSheet {
    fn get(&self, rune: &ProtoruneRuneId) -> u128 {
        *self.balances.get(rune).unwrap_or(&0u128)
    }

    fn set(&mut self, rune: &ProtoruneRuneId, value: u128) {
        self.balances.insert(*rune, value);
    }

    fn new() -> Self {
        Self {
            balances: BTreeMap::new(),
        }
    }

    fn merge(a: &Self, b: &Self) -> anyhow::Result<Self> {
        let mut merged = Self::new();
        merged.merge_sheets(a, b)?;
        Ok(merged)
    }

    fn balances(&self) -> &BTreeMap<ProtoruneRuneId, u128> {
        &self.balances
    }
}