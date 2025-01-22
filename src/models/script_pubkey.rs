use {
    crate::db::Entry,
    bitcoin::{hashes::Hash, OutPoint},
    borsh::{BorshDeserialize, BorshSerialize},
};

#[derive(Debug, PartialEq, Clone, Default)]
pub struct ScriptPubkeyEntry {
    pub utxos: Vec<OutPoint>,
}

impl ScriptPubkeyEntry {
    pub fn merge(&self, other: ScriptPubkeyEntry) -> ScriptPubkeyEntry {
        let mut outpoints = self.utxos.clone();
        outpoints.extend(other.utxos);
        ScriptPubkeyEntry { utxos: outpoints }
    }
}

impl BorshSerialize for ScriptPubkeyEntry {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        // First serialize the length of utxos vector
        BorshSerialize::serialize(&(self.utxos.len() as u32), writer)?;

        // Then serialize each OutPoint manually
        for outpoint in &self.utxos {
            outpoint
                .txid
                .as_raw_hash()
                .as_byte_array()
                .serialize(writer)?;
            outpoint.vout.serialize(writer)?;
        }

        Ok(())
    }
}

impl BorshDeserialize for ScriptPubkeyEntry {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        // Read vector length
        let len = u32::deserialize_reader(reader)? as usize;
        let mut utxos = Vec::with_capacity(len);

        // Deserialize each OutPoint
        for _ in 0..len {
            let txid_bytes = <[u8; 32]>::deserialize_reader(reader)?;
            let vout = u32::deserialize_reader(reader)?;
            utxos.push(OutPoint {
                txid: bitcoin::Txid::from_byte_array(txid_bytes),
                vout,
            });
        }

        Ok(Self { utxos })
    }
}

impl Entry for ScriptPubkeyEntry {}
