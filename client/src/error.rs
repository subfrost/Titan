use bitcoin::hex::HexToArrayError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("reqwest error")]
    ReqwestError(#[from] reqwest::Error),

    #[error("titan error with status {0}: {1}")]
    TitanError(reqwest::StatusCode, String),

    #[error("serde error")]
    SerdeError(#[from] serde_json::Error),

    #[error("runtime error: {0}")]
    Runtime(String),

    #[error("hex error: {0}")]
    HexToArrayError(#[from] HexToArrayError),
}
