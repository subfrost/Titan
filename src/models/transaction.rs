use {
    crate::db::Entry,
    bitcoin::{hashes::Hash, OutPoint, ScriptBuf, Txid},
    borsh::{BorshDeserialize, BorshSerialize},
    ordinals::RuneId,
    serde::{Deserialize, Serialize},
    std::io::{Read, Result, Write},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInEntry {
    pub outpoint: OutPoint,
    pub script_pubkey: ScriptBuf,
}

impl Entry for TxInEntry {}

impl BorshSerialize for TxInEntry {
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
        BorshSerialize::serialize(&self.outpoint.txid.as_raw_hash().as_byte_array(), writer)?;
        BorshSerialize::serialize(&self.outpoint.vout, writer)?;

        // Write out script pubkey
        let script_pubkey_bytes = self.script_pubkey.as_bytes();
        BorshSerialize::serialize(&(script_pubkey_bytes.len() as u64), writer)?;
        BorshSerialize::serialize(&script_pubkey_bytes, writer)?;
        Ok(())
    }
}

impl BorshDeserialize for TxInEntry {
    fn deserialize_reader<R: Read>(reader: &mut R) -> Result<Self> {
        let txid_bytes = <[u8; 32]>::deserialize_reader(reader)?;
        let txid = Txid::from_byte_array(txid_bytes);
        let vout = u32::deserialize_reader(reader)?;

        // Read back script pubkey
        let script_pubkey_len = u64::deserialize_reader(reader)?;
        let script_pubkey_bytes = vec![0; script_pubkey_len as usize];
        let script_pubkey = ScriptBuf::from(script_pubkey_bytes);

        Ok(TxInEntry {
            outpoint: OutPoint::new(txid, vout),
            script_pubkey,
        })
    }
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct TxOutEntry {
    pub runes: Vec<RuneAmount>,
    pub value: u64,
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

impl Entry for TxOutEntry {}
