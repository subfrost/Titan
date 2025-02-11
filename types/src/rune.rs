use {
    crate::inscription_id::InscriptionId,
    bitcoin::Txid,
    borsh::{BorshDeserialize, BorshSerialize},
    ordinals::{RuneId, SpacedRune},
    serde::{Deserialize, Serialize},
    std::io::{Read, Result, Write},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct MintResponse {
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub mintable: bool,
    pub cap: u128,
    pub amount: u128,
    pub mints: u128,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RuneResponse {
    pub id: RuneId,
    pub block: u64,
    pub burned: u128,
    pub divisibility: u8,
    pub etching: Txid,
    pub number: u64,
    pub premine: u128,
    pub supply: u128,
    pub max_supply: u128,
    pub spaced_rune: SpacedRune,
    pub symbol: Option<char>,
    pub mint: Option<MintResponse>,
    pub burns: u128,
    pub pending_burns: u128,
    pub pending_mints: u128,
    pub inscription_id: Option<InscriptionId>,
    pub timestamp: u64,
    pub turbo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneAmount {
    pub rune_id: RuneId,
    pub amount: u128,
}

impl From<(RuneId, u128)> for RuneAmount {
    fn from((rune_id, amount): (RuneId, u128)) -> Self {
        Self { rune_id, amount }
    }
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
