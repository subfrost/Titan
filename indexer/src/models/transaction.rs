use {
    bitcoin::{hashes::Hash, OutPoint, ScriptBuf, Txid},
    borsh::{BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    std::io::{Read, Result, Write},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInEntry {
    pub outpoint: OutPoint,
    pub script_pubkey: ScriptBuf,
}

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
