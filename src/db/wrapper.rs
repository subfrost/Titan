use std::io::{Read, Result, Write};

use borsh::{BorshDeserialize, BorshSerialize};
use ordinals::RuneId;
use serde::{Deserialize, Serialize};

use super::Entry;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct RuneIdWrapper(pub RuneId);

impl BorshSerialize for RuneIdWrapper {
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
        BorshSerialize::serialize(&self.0.block, writer)?;
        BorshSerialize::serialize(&self.0.tx, writer)?;
        Ok(())
    }
}

impl BorshDeserialize for RuneIdWrapper {
    fn deserialize_reader<R: Read>(reader: &mut R) -> Result<Self> {
        // Read back RuneId fields:
        let block: u64 = u64::deserialize_reader(reader)?;
        let tx = u32::deserialize_reader(reader)?;

        Ok(RuneIdWrapper(RuneId { block, tx }))
    }
}

impl Entry for RuneIdWrapper {}
