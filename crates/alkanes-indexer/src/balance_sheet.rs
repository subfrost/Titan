use anyhow::{anyhow, Result};
use metashrew_support::index_pointer::KeyValuePointer;
use protorune_support::balance_sheet::{BalanceSheet, BalanceSheetOperations, ProtoruneRuneId};
use std::collections::BTreeMap;
use crate::store::AlkanesProtoruneStore;
use crate::index_pointer::IndexPointer;

pub fn load_sheet(ptr: &IndexPointer) -> BalanceSheet<AlkanesProtoruneStore> {
    let runes_ptr = ptr.keyword("/runes");
    let balances_ptr = ptr.keyword("/balances");
    let length = runes_ptr.length();
    let mut result = BalanceSheet::default();

    for i in 0..length {
        let rune = ProtoruneRuneId::from(runes_ptr.select_index(i).get());
        let balance = balances_ptr.select_index(i).get_value::<u128>();
        result.set(&rune, balance);
    }
    result
}