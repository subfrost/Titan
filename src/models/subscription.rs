use {
    super::EventType,
    crate::db::Entry,
    borsh::{BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    uuid::Uuid,
};

#[derive(Debug, Clone, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Subscription {
    pub id: Uuid,
    pub endpoint: String,
    pub event_types: Vec<EventType>,
    pub last_success_epoch_secs: u64,
}

impl Entry for Subscription {}
