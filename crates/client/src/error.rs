use bitcoin::{consensus, hex::HexToArrayError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("reqwest error")]
    ReqwestError(#[from] reqwest::Error),

    #[error("titan error with status {0}: {1}")]
    TitanError(reqwest::StatusCode, String),

    #[error("serde error")]
    SerdeError(#[from] serde_json::Error),

    #[error("hex error: {0}")]
    HexToArrayError(#[from] HexToArrayError),

    #[error("bitcoin consensus error: {0}")]
    BitcoinConsensusError(#[from] consensus::encode::Error),
}
