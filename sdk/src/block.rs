use {
    bitcoin::{
        block::{Header, Version},
        hashes::Hash,
        BlockHash, CompactTarget, TxMerkleNode,
    },
    borsh::{BorshDeserialize, BorshSerialize},
    ordinals::RuneId,
    serde::{Deserialize, Serialize},
    std::io::{Read, Result, Write},
};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Block {
    pub height: u32,
    pub header: Header,
    pub tx_ids: Vec<String>,
    pub etched_runes: Vec<RuneId>,
}

impl BorshSerialize for Block {
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
        // 1) Serialize `height`
        BorshSerialize::serialize(&self.height, writer)?;

        // 2) Serialize `header` manually
        BorshSerialize::serialize(&self.header.version.to_consensus(), writer)?;

        // `as_raw_hash()` returns a &sha256d::Hash, which we can then convert
        // into a 32-byte array. That array is what Borsh will see.
        BorshSerialize::serialize(
            &self.header.prev_blockhash.as_raw_hash().as_byte_array(),
            writer,
        )?;

        BorshSerialize::serialize(
            &self.header.merkle_root.as_raw_hash().as_byte_array(),
            writer,
        )?;
        BorshSerialize::serialize(&self.header.time, writer)?;
        BorshSerialize::serialize(&self.header.bits.to_consensus(), writer)?;
        BorshSerialize::serialize(&self.header.nonce, writer)?;

        // 3) Serialize `tx_ids` as a Vec<String>
        //    (Vec<String> already implements BorshSerialize)
        BorshSerialize::serialize(&self.tx_ids, writer)?;

        // 3) Serialize `etched_runes` manually:
        //    Borsh doesn't know about `RuneId`, so we store it ourselves:
        //
        //    - First, write the length of the vector
        //    - Then for each `RuneId`, write out (block, tx)

        let etched_len = self.etched_runes.len() as u64;
        BorshSerialize::serialize(&etched_len, writer)?;

        for rune_id in &self.etched_runes {
            BorshSerialize::serialize(&rune_id.block, writer)?;
            BorshSerialize::serialize(&rune_id.tx, writer)?;
        }

        Ok(())
    }
}

impl BorshDeserialize for Block {
    fn deserialize_reader<R: Read>(reader: &mut R) -> std::io::Result<Self> {
        // 1) Deserialize `height`
        let height = u32::deserialize_reader(reader)?;

        // 2) Deserialize `header` manually
        let version = Version::from_consensus(i32::deserialize_reader(reader)?);

        let mut prev_hash = [0u8; 32];
        reader.read_exact(&mut prev_hash)?;
        let prev_blockhash =
            BlockHash::from_raw_hash(Hash::from_slice(&prev_hash).map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid prev_blockhash")
            })?);

        let mut merkle = [0u8; 32];
        reader.read_exact(&mut merkle)?;
        let merkle_root = TxMerkleNode::from_raw_hash(Hash::from_slice(&merkle).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid merkle_root")
        })?);

        let time = u32::deserialize_reader(reader)?;
        let bits = CompactTarget::from_consensus(u32::deserialize_reader(reader)?);
        let nonce = u32::deserialize_reader(reader)?;

        let header = Header {
            version,
            prev_blockhash,
            merkle_root,
            time,
            bits,
            nonce,
        };

        // 3) Deserialize `tx_ids` (Vec<String>)
        let tx_ids = Vec::<String>::deserialize_reader(reader)?;

        // 3) Deserialize `etched_runes` manually:
        //    - Read the length
        //    - For each entry, read `block` (u64) then `tx` (u32)

        let etched_len = u64::deserialize_reader(reader)?;
        let mut etched_runes = Vec::with_capacity(etched_len as usize);

        for _ in 0..etched_len {
            let block = u64::deserialize_reader(reader)?;
            let tx = u32::deserialize_reader(reader)?;
            etched_runes.push(RuneId { block, tx });
        }

        Ok(Self {
            height,
            header,
            tx_ids,
            etched_runes,
        })
    }
}

impl Block {
    pub fn empty_block(height: u32, header: Header) -> Self {
        Self {
            height,
            header,
            tx_ids: vec![],
            etched_runes: vec![],
        }
    }
}
