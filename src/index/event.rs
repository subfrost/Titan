use {
    bitcoin::{Address, OutPoint, Txid},
    ordinals::RuneId,
};

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    RuneEtched {
        block_height: u32,
        rune_id: RuneId,
        txid: Txid,
        in_mempool: bool,
    },
    RuneBurned {
        amount: u128,
        block_height: u32,
        rune_id: RuneId,
        txid: Txid,
        in_mempool: bool,
    },
    RuneMinted {
        amount: u128,
        block_height: u32,
        rune_id: RuneId,
        txid: Txid,
        in_mempool: bool,
    },
    RuneTransferred {
        amount: u128,
        block_height: u32,
        outpoint: OutPoint,
        rune_id: RuneId,
        txid: Txid,
        in_mempool: bool,
    },
    AddressModified {
        address: Address,
        block_height: u32,
    },
}
