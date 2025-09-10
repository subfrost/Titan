use crate::balance_sheet::{BalanceSheet, ProtoruneStore};
use crate::rune_transfer::RuneTransfer;
use anyhow::Result;
use bitcoin::{Block, Transaction};

pub trait MessageContext<P: ProtoruneStore + Clone> {
    fn handle(
        parcel: &MessageContextParcel<P>,
    ) -> Result<(Vec<RuneTransfer>, BalanceSheet<P>)>;
    fn protocol_tag() -> u128;
}

#[derive(Clone)]
pub struct MessageContextParcel<P: ProtoruneStore + Clone> {
    pub store: P,
    pub runes: Vec<RuneTransfer>,
    pub transaction: Transaction,
    pub block: Block,
    pub height: u64,
    pub pointer: u32,
    pub refund_pointer: u32,
    pub calldata: Vec<u8>,
    pub sheets: Box<BalanceSheet<P>>,
    pub txindex: u32,
    pub vout: u32,
    pub runtime_balances: Box<BalanceSheet<P>>,
}