use {
    bitcoin::{hashes::Hash, Txid},
    bitcoincore_rpc::json::GetMempoolEntryResult,
    borsh::{BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    std::io::{Read, Result, Write},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct MempoolEntryFee {
    pub base: u64,
    pub descendant: u64,
    pub ancestor: u64,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MempoolEntry {
    pub vsize: u64,
    pub weight: Option<u64>,
    pub descendant_count: u64,
    pub descendant_size: u64,
    pub ancestor_count: u64,
    pub ancestor_size: u64,
    pub fees: MempoolEntryFee,
    pub depends: Vec<Txid>,
    #[serde(rename = "spentby")]
    pub spent_by: Vec<Txid>,
}

impl From<&GetMempoolEntryResult> for MempoolEntry {
    fn from(value: &GetMempoolEntryResult) -> Self {
        Self {
            vsize: value.vsize,
            weight: value.weight,
            descendant_count: value.descendant_count,
            descendant_size: value.descendant_size,
            ancestor_count: value.ancestor_count,
            ancestor_size: value.ancestor_size,
            fees: MempoolEntryFee {
                base: value.fees.base.to_sat(),
                descendant: value.fees.descendant.to_sat(),
                ancestor: value.fees.ancestor.to_sat(),
            },
            depends: value.depends.clone(),
            spent_by: value.spent_by.clone(),
        }
    }
}

impl BorshSerialize for MempoolEntry {
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
        BorshSerialize::serialize(&self.vsize, writer)?;

        if let Some(weight) = self.weight {
            BorshSerialize::serialize(&true, writer)?;
            BorshSerialize::serialize(&weight, writer)?;
        } else {
            BorshSerialize::serialize(&false, writer)?;
        }

        BorshSerialize::serialize(&self.descendant_count, writer)?;
        BorshSerialize::serialize(&self.descendant_size, writer)?;
        BorshSerialize::serialize(&self.ancestor_count, writer)?;
        BorshSerialize::serialize(&self.ancestor_size, writer)?;
        BorshSerialize::serialize(&self.fees, writer)?;

        BorshSerialize::serialize(&self.depends.len(), writer)?;
        for txid in &self.depends {
            BorshSerialize::serialize(&txid.as_raw_hash().to_byte_array(), writer)?;
        }

        BorshSerialize::serialize(&self.spent_by.len(), writer)?;
        for txid in &self.spent_by {
            BorshSerialize::serialize(&txid.as_raw_hash().to_byte_array(), writer)?;
        }
        Ok(())
    }
}

impl BorshDeserialize for MempoolEntry {
    fn deserialize_reader<R: Read>(reader: &mut R) -> Result<Self> {
        let vsize = u64::deserialize_reader(reader)?;

        let weight_exists = bool::deserialize_reader(reader)?;
        let weight = if weight_exists {
            Some(u64::deserialize_reader(reader)?)
        } else {
            None
        };

        let descendant_count = u64::deserialize_reader(reader)?;
        let descendant_size = u64::deserialize_reader(reader)?;
        let ancestor_count = u64::deserialize_reader(reader)?;
        let ancestor_size = u64::deserialize_reader(reader)?;
        let fees = MempoolEntryFee::deserialize_reader(reader)?;

        let depends_len = u64::deserialize_reader(reader)?;
        let mut depends = Vec::new();
        for _ in 0..depends_len {
            let txid_bytes = <[u8; 32]>::deserialize_reader(reader)?;
            let txid = bitcoin::Txid::from_byte_array(txid_bytes);
            depends.push(txid);
        }

        let spent_by_len = u64::deserialize_reader(reader)?;
        let mut spent_by = Vec::new();
        for _ in 0..spent_by_len {
            let txid_bytes = <[u8; 32]>::deserialize_reader(reader)?;
            let txid = bitcoin::Txid::from_byte_array(txid_bytes);
            spent_by.push(txid);
        }

        Ok(Self {
            vsize,
            weight,
            descendant_count,
            descendant_size,
            ancestor_count,
            ancestor_size,
            fees,
            depends,
            spent_by,
        })
    }
}
