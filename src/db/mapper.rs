use bitcoin::{hashes::Hash, BlockHash};
use rocks::DBResult;

use super::*;

pub trait DBResultMapper<T> {
    fn mapped(self) -> DBResult<Option<T>>;
}

fn map_db_type<T>(
    val: DBResult<Option<Vec<u8>>>,
    map_fn: &dyn Fn(&[u8]) -> DBResult<T>,
) -> DBResult<Option<T>> {
    match val {
        Ok(Some(val)) => {
            let mapped = map_fn(&val)?;
            Ok(Some(mapped))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
}

impl<T: Entry> DBResultMapper<T> for DBResult<Option<Vec<u8>>> {
    fn mapped(self) -> DBResult<Option<T>> {
        let func = |vec: &[u8]| -> DBResult<T> {
            let val = T::load(vec.to_vec());
            Ok(val)
        };

        map_db_type(self, &func)
    }
}

impl DBResultMapper<u64> for DBResult<Option<Vec<u8>>> {
    fn mapped(self) -> DBResult<Option<u64>> {
        let func = |vec: &[u8]| -> DBResult<u64> {
            match vec.to_owned().try_into() {
                Ok(val) => Ok(u64::from_le_bytes(val)),
                Err(_) => Err(RocksDBError::InvalidU64),
            }
        };

        map_db_type(self, &func)
    }
}

impl DBResultMapper<String> for DBResult<Option<Vec<u8>>> {
    fn mapped(self) -> DBResult<Option<String>> {
        let func = |vec: &[u8]| -> DBResult<String> {
            Ok(std::str::from_utf8(vec)
                .map_err(|_| RocksDBError::InvalidString)?
                .to_string())
        };

        map_db_type(self, &func)
    }
}

impl DBResultMapper<BlockHash> for DBResult<Option<Vec<u8>>> {
    fn mapped(self) -> DBResult<Option<BlockHash>> {
        let func = |vec: &[u8]| -> DBResult<BlockHash> {
            Ok(BlockHash::from_raw_hash(
                Hash::from_slice(&vec).map_err(|_| RocksDBError::InvalidBlockHash)?,
            ))
        };

        map_db_type(self, &func)
    }
}
