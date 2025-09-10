use protorune_support::{
    balance_sheet::{BalanceSheet, ProtoruneStore},
    message::{MessageContext, MessageContextParcel},
    rune_transfer::RuneTransfer,
};
use crate::{index::store::Store, db::PROTORUNE_BALANCES_CF};
use anyhow::Result;

#[derive(Clone)]
pub struct TitanProtoruneStore<'a> {
    pub store: &'a dyn Store,
}

impl<'a> ProtoruneStore for TitanProtoruneStore<'a> {
    fn get(&self, key: &[u8]) -> Option<Vec<u8>> {
        self.store.get(PROTORUNE_BALANCES_CF, key).unwrap()
    }
}

#[derive(Clone, Default)]
pub struct TitanMessageContext(());

impl<'a> MessageContext<TitanProtoruneStore<'a>> for TitanMessageContext {
    fn protocol_tag() -> u128 {
        1
    }
    fn handle(
        _parcel: &MessageContextParcel<TitanProtoruneStore>,
    ) -> Result<(Vec<RuneTransfer>, BalanceSheet<TitanProtoruneStore<'a>>)> {
        // TODO: Implement the logic from alkanes-rs
        Ok((vec![], BalanceSheet::default()))
    }
}