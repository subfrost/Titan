pub use {error::RocksDBError, rocks::RocksDB};

pub mod entry;
mod error;
mod mapper;
mod rocks;
pub mod util;
mod wrapper;
