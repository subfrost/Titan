use {
    bitcoin::{BlockHash, OutPoint, Txid},
    borsh::{BorshDeserialize, BorshSerialize},
    ordinals::RuneId,
    serde::{Deserialize, Serialize},
    std::fmt,
};

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum EventType {
    RuneEtched,
    RuneBurned,
    RuneMinted,
    RuneTransferred,
    AddressModified,
    TransactionsAdded,
    TransactionsReplaced,
    NewBlock,
}

impl From<Event> for EventType {
    fn from(event: Event) -> Self {
        match event {
            Event::RuneEtched { .. } => EventType::RuneEtched,
            Event::RuneBurned { .. } => EventType::RuneBurned,
            Event::RuneMinted { .. } => EventType::RuneMinted,
            Event::RuneTransferred { .. } => EventType::RuneTransferred,
            Event::AddressModified { .. } => EventType::AddressModified,
            Event::TransactionsAdded { .. } => EventType::TransactionsAdded,
            Event::TransactionsReplaced { .. } => EventType::TransactionsReplaced,
            Event::NewBlock { .. } => EventType::NewBlock,
        }
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Simply print the variant name.
        match self {
            EventType::RuneEtched => write!(f, "RuneEtched"),
            EventType::RuneBurned => write!(f, "RuneBurned"),
            EventType::RuneMinted => write!(f, "RuneMinted"),
            EventType::RuneTransferred => write!(f, "RuneTransferred"),
            EventType::AddressModified => write!(f, "AddressModified"),
            EventType::TransactionsAdded => write!(f, "TransactionsAdded"),
            EventType::TransactionsReplaced => write!(f, "TransactionsReplaced"),
            EventType::NewBlock => write!(f, "NewBlock"),
        }
    }
}

impl Into<String> for EventType {
    fn into(self) -> String {
        self.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
        address: String,
        block_height: u32,
        in_mempool: bool,
    },
    TransactionsAdded {
        txids: Vec<Txid>,
    },
    TransactionsReplaced {
        txids: Vec<Txid>,
    },
    NewBlock {
        block_height: u64,
        block_hash: BlockHash,
    },
}
