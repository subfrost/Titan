use {
    crate::index::{Index, IndexError},
    bitcoin::BlockHash,
    ordinals::{RuneId, SpacedRune},
    std::{str::FromStr, sync::Arc},
};

#[derive(Debug, thiserror::Error)]
pub enum BlockParseError {
    #[error("invalid block height")]
    InvalidHeight,
    #[error("invalid block hash")]
    InvalidHash,
}

#[derive(Debug)]
pub enum Block {
    Height(u64),
    Hash(BlockHash),
}

impl Block {
    pub fn to_hash(&self, index: &Arc<Index>) -> Result<BlockHash, IndexError> {
        match self {
            Block::Height(height) => index.get_block_hash(*height),
            Block::Hash(hash) => Ok(*hash),
        }
    }
}

impl FromStr for Block {
    type Err = BlockParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if s.len() == 64 {
            Self::Hash(s.parse().map_err(|_| BlockParseError::InvalidHash)?)
        } else {
            Self::Height(s.parse().map_err(|_| BlockParseError::InvalidHeight)?)
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RuneParseError {
    #[error("invalid rune id")]
    InvalidId,
    #[error("invalid spaced rune")]
    InvalidSpacedRune,
}

#[derive(Debug)]
pub enum Rune {
    Spaced(SpacedRune),
    Id(RuneId),
}

impl Rune {
    pub fn to_rune_id(&self, index: &Arc<Index>) -> Result<RuneId, IndexError> {
        match self {
            Rune::Spaced(spaced_rune) => index.get_rune_id(&spaced_rune.rune),
            Rune::Id(rune_id) => Ok(*rune_id),
        }
    }
}

impl FromStr for Rune {
    type Err = RuneParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.contains(':') {
            Ok(Self::Id(s.parse().map_err(|_| RuneParseError::InvalidId)?))
        } else {
            Ok(Self::Spaced(
                s.parse().map_err(|_| RuneParseError::InvalidSpacedRune)?,
            ))
        }
    }
}
