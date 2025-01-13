use thiserror::Error;

#[derive(Error, Debug, Clone, PartialEq)]
pub enum RocksDBError {
    #[error("error initializing database")]
    Initialization(#[from] rocksdb::Error),
    #[error("invalid column handle: {:?}", .0)]
    InvalidHandle(String),
    #[error("invalid u64")]
    InvalidU64,
    #[error("invalid String")]
    InvalidString,
    #[error("poisoned lock")]
    LockPoisoned,
    #[error("not found: {0}")]
    NotFound(String),
    #[error("overflow")]
    Overflow,
}
