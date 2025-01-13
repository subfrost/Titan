use {
    crate::db::Entry,
    borsh::{BorshDeserialize, BorshSerialize},
    ordinals::RuneId,
    serde::{Deserialize, Serialize},
    std::io::{Read, Result, Write},
};

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct TxOutEntry {
    pub runes: Vec<RuneAmount>,
    pub spent: bool,
}

impl TxOutEntry {
    pub fn has_runes(&self) -> bool {
        !self.runes.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneAmount {
    pub rune_id: RuneId,
    pub amount: u128,
}

impl BorshSerialize for RuneAmount {
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
        // Write out RuneId (block, tx):
        BorshSerialize::serialize(&self.rune_id.block, writer)?;
        BorshSerialize::serialize(&self.rune_id.tx, writer)?;

        // Write out amount
        BorshSerialize::serialize(&self.amount, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for RuneAmount {
    fn deserialize_reader<R: Read>(reader: &mut R) -> Result<Self> {
        // Read back RuneId fields:
        let block = u64::deserialize_reader(reader)?;
        let tx = u32::deserialize_reader(reader)?;

        // Read back amount
        let amount = u128::deserialize_reader(reader)?;

        Ok(RuneAmount {
            rune_id: RuneId { block, tx },
            amount,
        })
    }
}

impl Entry for TxOutEntry {}
