use {
    crate::rune::RuneAmount,
    bitcoin::{hashes::Hash, Txid},
    borsh::{BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    std::io::{Read, Result, Write},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpenderReference {
    pub txid: Txid,
    pub vin: u32,
}

impl BorshSerialize for SpenderReference {
    fn serialize<W: Write>(&self, writer: &mut W) -> Result<()> {
        BorshSerialize::serialize(&self.txid.as_raw_hash().to_byte_array(), writer)?;
        BorshSerialize::serialize(&self.vin, writer)?;
        Ok(())
    }
}

impl BorshDeserialize for SpenderReference {
    fn deserialize_reader<R: Read>(reader: &mut R) -> Result<Self> {
        let txid_bytes = <[u8; 32]>::deserialize_reader(reader)?;
        let txid = bitcoin::Txid::from_byte_array(txid_bytes);
        let vin = u32::deserialize_reader(reader)?;
        Ok(Self { txid, vin })
    }
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
pub enum SpentStatus {
    Unspent,
    Spent(SpenderReference),
}

// Intermediate structure for JSON serialization
#[derive(Serialize, Deserialize)]
struct SpentStatusJson {
    spent: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    vin: Option<SpenderReference>,
}

impl Serialize for SpentStatus {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            SpentStatus::Unspent => SpentStatusJson {
                spent: false,
                vin: None,
            },
            SpentStatus::Spent(vin) => SpentStatusJson {
                spent: true,
                vin: Some(vin.clone()),
            },
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SpentStatus {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let json = SpentStatusJson::deserialize(deserializer)?;
        Ok(if json.spent {
            SpentStatus::Spent(json.vin.ok_or_else(|| {
                serde::de::Error::custom("missing vin field for spent transaction")
            })?)
        } else {
            SpentStatus::Unspent
        })
    }
}

#[derive(Debug, Clone, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct TxOutEntry {
    pub runes: Vec<RuneAmount>,
    pub risky_runes: Vec<RuneAmount>,
    pub value: u64,
    pub spent: SpentStatus,
}

impl TxOutEntry {
    pub fn has_runes(&self) -> bool {
        !self.runes.is_empty()
    }
}
