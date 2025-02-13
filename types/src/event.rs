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
    Reorg,
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
            Event::Reorg { .. } => EventType::Reorg,
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
            EventType::Reorg => write!(f, "Reorg"),
        }
    }
}

impl Into<String> for EventType {
    fn into(self) -> String {
        self.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Location {
    pub mempool: bool,
    pub block_height: Option<u64>,
}

impl Location {
    pub fn mempool() -> Self {
        Location {
            mempool: true,
            block_height: None,
        }
    }

    pub fn block(block_height: u64) -> Self {
        Location {
            mempool: false,
            block_height: Some(block_height),
        }
    }
}

impl From<Option<u64>> for Location {
    fn from(block_height: Option<u64>) -> Self {
        match block_height {
            Some(block_height) => Location {
                mempool: false,
                block_height: Some(block_height),
            },
            None => Location {
                mempool: true,
                block_height: None,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    RuneEtched {
        location: Location,
        rune_id: RuneId,
        txid: Txid,
    },
    RuneBurned {
        amount: u128,
        location: Location,
        rune_id: RuneId,
        txid: Txid,
    },
    RuneMinted {
        amount: u128,
        location: Location,
        rune_id: RuneId,
        txid: Txid,
    },
    RuneTransferred {
        amount: u128,
        location: Location,
        outpoint: OutPoint,
        rune_id: RuneId,
        txid: Txid,
    },
    AddressModified {
        address: String,
        location: Location,
    },
    TransactionsAdded {
        txids: Vec<Txid>,
    },
    TransactionsReplaced {
        txids: Vec<Txid>,
    },
    NewBlock {
        block_hash: BlockHash,
        block_height: u64,
    },
    Reorg {
        height: u64,
        depth: u64,
    },
}
