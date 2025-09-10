use {
    crate::index::Chain,
    bitcoincore_rpc::{Client, RpcApi},
    std::{thread, time::Duration},
    thiserror::Error,
};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum RpcClientError {
    #[error("mismatched chain `{0}` does not match `{1}`")]
    MismatchedChain(String, String),
    #[error("unknown chain {0}")]
    UnknownChain(String),
    #[error("failed to connect to rpc {0}")]
    FailedToConnect(String),
}

pub trait RpcClientProvider: Send + Sync + 'static {
    fn get_new_rpc_client(&self) -> Result<Client, RpcClientError>;
}

pub fn validate_rpc_connection(client: Client, chain: Chain) -> Result<(), RpcClientError> {
    let mut checks = 0;
    let rpc_chain = loop {
        match client.get_blockchain_info() {
            Ok(blockchain_info) => {
                break match blockchain_info.chain.to_string().as_str() {
                    "bitcoin" => Chain::Mainnet,
                    "testnet" => Chain::Testnet,
                    "testnet4" => Chain::Testnet4,
                    "regtest" => Chain::Regtest,
                    "signet" => Chain::Signet,
                    other => return Err(RpcClientError::UnknownChain(other.to_string())),
                }
            }
            Err(bitcoincore_rpc::Error::JsonRpc(bitcoincore_rpc::jsonrpc::Error::Rpc(err)))
                if err.code == -28 => {}
            Err(err) => {
                return Err(RpcClientError::FailedToConnect(err.to_string()));
            }
        }

        if checks >= 5 {
            return Err(RpcClientError::FailedToConnect(
                "Failed to connect to Bitcoin Core RPC".to_string(),
            ));
        }

        checks += 1;
        thread::sleep(Duration::from_millis(100));
    };

    if rpc_chain != chain {
        return Err(RpcClientError::MismatchedChain(
            rpc_chain.to_string(),
            chain.to_string(),
        ));
    }

    Ok(())
}
