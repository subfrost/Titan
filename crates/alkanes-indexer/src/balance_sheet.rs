use anyhow::{anyhow, Result};
use crate::index_pointer::{IndexPointer, AtomicPointer};
use metashrew_support::index_pointer::KeyValuePointer;
use protorune_support::balance_sheet::{BalanceSheet, BalanceSheetOperations, ProtoruneRuneId};
use std::collections::BTreeMap;
use crate::store::AlkanesProtoruneStore;


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

pub trait PersistentRecord: BalanceSheetOperations {
    fn save(&self, ptr: &mut AtomicPointer, is_cenotaph: bool);
}


impl PersistentRecord for BalanceSheet<AlkanesProtoruneStore> {
	fn save(&self, ptr: &mut AtomicPointer, is_cenotaph: bool) {
		let runes_ptr = ptr.derive(&IndexPointer::from_keyword("/runes"));
        let balances_ptr = ptr.derive(&IndexPointer::from_keyword("/balances"));
        let runes_to_balances_ptr = ptr.derive(&IndexPointer::from_keyword("/id_to_balance"));

        for (rune, balance) in self.balances() {
            if *balance != 0u128 && !is_cenotaph {
                let rune_bytes: Vec<u8> = (*rune).into();
                runes_ptr.append(rune_bytes.clone().into());

                balances_ptr.append_value::<u128>(*balance);

                runes_to_balances_ptr
                    .select(&rune_bytes)
                    .set_value::<u128>(*balance);
            }
        }
	}
}