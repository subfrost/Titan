use crate::balance_sheet::{BalanceSheet, ProtoruneStore};
use crate::rune_transfer::RuneTransfer;
use anyhow::Result;
use bitcoin::hashes::Hash;
use bitcoin::{
    Block, BlockHash, Transaction, TxMerkleNode,
};

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

impl<P: ProtoruneStore + Clone + Default> MessageContextParcel<P> {
    pub fn new() -> Self {
        Self {
            store: P::default(),
            runes: vec![],
            transaction: Transaction {
                version: bitcoin::blockdata::transaction::Version(0),
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: vec![],
                output: vec![],
            },
            block: Block {
                header: bitcoin::block::Header {
                    version: bitcoin::block::Version::from_consensus(0),
                    prev_blockhash: BlockHash::all_zeros(),
                    merkle_root: TxMerkleNode::all_zeros(),
                    time: 0,
                    bits: bitcoin::CompactTarget::from_consensus(0),
                    nonce: 0,
                },
                txdata: vec![],
            },
            height: 0,
            pointer: 0,
            refund_pointer: 0,
            calldata: vec![],
            sheets: Box::new(BalanceSheet::default()),
            txindex: 0,
            vout: 0,
            runtime_balances: Box::new(BalanceSheet::default()),
        }
    }
}