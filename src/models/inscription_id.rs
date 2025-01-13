use {
    bitcoin::{hashes::Hash, Txid},
    borsh::{BorshDeserialize, BorshSerialize},
    core::str,
    serde::{Deserialize, Serialize},
    std::{
        fmt::{self, Display, Formatter},
        io::{self, Read, Write},
        str::FromStr,
    },
};

#[derive(Debug, Eq, PartialEq, Clone, Hash, Serialize, Deserialize)]
pub struct InscriptionId {
    pub txid: Txid,
    pub index: u32,
}

impl BorshSerialize for InscriptionId {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        // Serialize txid bytes (32 bytes)
        BorshSerialize::serialize(&self.txid.as_raw_hash().to_byte_array(), writer)?;

        // Serialize index
        BorshSerialize::serialize(&self.index, writer)?;

        Ok(())
    }
}

impl BorshDeserialize for InscriptionId {
    fn deserialize_reader<R: Read>(reader: &mut R) -> io::Result<Self> {
        // Read 32 bytes for txid
        let txid_bytes = <[u8; 32]>::deserialize_reader(reader)?;

        // Deserialize index
        let index = u32::deserialize_reader(reader)?;

        Ok(Self {
            txid: Txid::from_byte_array(txid_bytes),
            index,
        })
    }
}

impl Display for InscriptionId {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}i{}", self.txid, self.index)
    }
}

#[derive(Debug)]
pub enum ParseError {
    Character(char),
    Length(usize),
    Separator(char),
    Txid(bitcoin::hex::HexToArrayError),
    Index(std::num::ParseIntError),
}

impl Display for ParseError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Character(c) => write!(f, "invalid character: '{c}'"),
            Self::Length(len) => write!(f, "invalid length: {len}"),
            Self::Separator(c) => write!(f, "invalid separator: `{c}`"),
            Self::Txid(err) => write!(f, "invalid txid: {err}"),
            Self::Index(err) => write!(f, "invalid index: {err}"),
        }
    }
}

impl std::error::Error for ParseError {}

impl FromStr for InscriptionId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(char) = s.chars().find(|char| !char.is_ascii()) {
            return Err(ParseError::Character(char));
        }

        const TXID_LEN: usize = 64;
        const MIN_LEN: usize = TXID_LEN + 2;

        if s.len() < MIN_LEN {
            return Err(ParseError::Length(s.len()));
        }

        let txid = &s[..TXID_LEN];

        let separator = s.chars().nth(TXID_LEN).unwrap();

        if separator != 'i' {
            return Err(ParseError::Separator(separator));
        }

        let vout = &s[TXID_LEN + 1..];

        Ok(Self {
            txid: txid.parse().map_err(ParseError::Txid)?,
            index: vout.parse().map_err(ParseError::Index)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use bitcoin::Txid;

    use super::*;

    fn inscription_id(n: u32) -> InscriptionId {
        let hex = format!("{n:x}");

        if hex.is_empty() || hex.len() > 1 {
            panic!();
        }

        format!("{}i{n}", hex.repeat(64)).parse().unwrap()
    }

    fn txid(n: u32) -> Txid {
        let hex = format!("{n:x}");

        if hex.is_empty() || hex.len() > 1 {
            panic!();
        }

        hex.repeat(64).parse().unwrap()
    }

    #[test]
    fn display() {
        assert_eq!(
            inscription_id(1).to_string(),
            "1111111111111111111111111111111111111111111111111111111111111111i1",
        );
        assert_eq!(
            InscriptionId {
                txid: txid(1),
                index: 0,
            }
            .to_string(),
            "1111111111111111111111111111111111111111111111111111111111111111i0",
        );
        assert_eq!(
            InscriptionId {
                txid: txid(1),
                index: 0xFFFFFFFF,
            }
            .to_string(),
            "1111111111111111111111111111111111111111111111111111111111111111i4294967295",
        );
    }

    #[test]
    fn from_str() {
        assert_eq!(
            "1111111111111111111111111111111111111111111111111111111111111111i1"
                .parse::<InscriptionId>()
                .unwrap(),
            inscription_id(1),
        );
        assert_eq!(
            "1111111111111111111111111111111111111111111111111111111111111111i4294967295"
                .parse::<InscriptionId>()
                .unwrap(),
            InscriptionId {
                txid: txid(1),
                index: 0xFFFFFFFF,
            },
        );
        assert_eq!(
            "1111111111111111111111111111111111111111111111111111111111111111i4294967295"
                .parse::<InscriptionId>()
                .unwrap(),
            InscriptionId {
                txid: txid(1),
                index: 0xFFFFFFFF,
            },
        );
    }
}
