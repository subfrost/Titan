use {
    super::{
        content::{content_response, AcceptEncoding, ContentError},
        query,
        rune::RuneResponse,
        stats::{BlockTip, Status},
        transaction::{Transaction, TxOut},
    },
    crate::{
        index::{Index, IndexError, StoreError},
        models::{Block, InscriptionId, Pagination, PaginationResponse, TxOutEntry},
    },
    bitcoin::{OutPoint, Txid},
    bitcoincore_rpc::{Client, RpcApi},
    http::HeaderMap,
    std::sync::Arc,
    tracing::error,
};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found: {0}")]
    IndexError(#[from] IndexError),
    #[error("rpc error: {0}")]
    RpcError(#[from] bitcoincore_rpc::Error),
    #[error("content error: {0}")]
    ContentError(#[from] ContentError),
}

pub type Result<T> = std::result::Result<T, ApiError>;

pub fn tip(index: Arc<Index>) -> Result<BlockTip> {
    let block_count = index.get_block_count()?;
    let block_hash = index.get_block_hash(block_count - 1)?;
    Ok(BlockTip {
        height: block_count,
        hash: block_hash.to_string(),
    })
}

pub fn status(index: Arc<Index>) -> Result<Status> {
    let block_count = index.get_block_count()?;
    let block_hash = index.get_block_hash(block_count - 1)?;
    Ok(Status {
        block_tip: BlockTip {
            height: block_count,
            hash: block_hash.to_string(),
        },
        runes_count: index.get_runes_count()?,
        mempool_tx_count: index.get_mempool_txids()?.len() as u64,
    })
}

pub fn block(index: Arc<Index>, block: &query::Block) -> Result<Block> {
    let hash = block.to_hash(&index)?;
    Ok(index.get_block_by_hash(&hash)?)
}

pub fn output(index: Arc<Index>, outpoint: &OutPoint) -> Result<TxOutEntry> {
    Ok(index.get_tx_out(outpoint)?)
}

pub fn inscription_content(
    index: Arc<Index>,
    inscription_id: &InscriptionId,
    accept_encoding: AcceptEncoding,
    csp_origin: Option<String>,
    decompress: bool,
) -> Result<Option<(HeaderMap, Vec<u8>)>> {
    let inscription = index.get_inscription(inscription_id)?;
    let content_response = content_response(inscription, accept_encoding, csp_origin, decompress)?;
    Ok(content_response)
}

pub fn rune(index: Arc<Index>, rune_query: &query::Rune) -> Result<RuneResponse> {
    let rune_id = rune_query.to_rune_id(&index)?;
    let block_count = index.get_block_count()?;
    let rune_response = RuneResponse::new(rune_id, index.get_rune(&rune_id)?, block_count);
    Ok(rune_response)
}

pub fn runes(
    index: Arc<Index>,
    pagination: Pagination,
) -> Result<PaginationResponse<RuneResponse>> {
    let rune_entries = index.get_runes(pagination)?;
    let block_count = index.get_block_count()?;
    let rune_responses: Vec<RuneResponse> = rune_entries
        .items
        .into_iter()
        .map(|(rune_id, rune_entry)| RuneResponse::new(rune_id, rune_entry, block_count))
        .collect();

    Ok(PaginationResponse {
        items: rune_responses,
        offset: rune_entries.offset,
    })
}

pub fn last_rune_transactions(
    index: Arc<Index>,
    rune_query: &query::Rune,
    pagination: Option<Pagination>,
) -> Result<PaginationResponse<String>> {
    let rune_id = rune_query.to_rune_id(&index)?;
    let transactions = index.get_last_rune_transactions(&rune_id, pagination, None)?;
    Ok(transactions)
}

pub fn transaction(index: Arc<Index>, client: Client, txid: &Txid) -> Result<Transaction> {
    let transaction = client.get_raw_transaction(txid, None)?;

    let mut tx_outs: Vec<TxOut> = Vec::new();
    for (vout, tx_out) in transaction.output.iter().enumerate() {
        match index.get_tx_out(&OutPoint {
            txid: txid.clone(),
            vout: vout as u32,
        }) {
            Ok(tx_out_entry) => {
                tx_outs.push(TxOut {
                    value: tx_out.value.to_sat(),
                    script_pubkey: tx_out.script_pubkey.clone(),
                    runes: tx_out_entry.runes,
                });
            }
            Err(IndexError::StoreError(StoreError::NotFound(_))) => {
                tx_outs.push(TxOut {
                    value: tx_out.value.to_sat(),
                    script_pubkey: tx_out.script_pubkey.clone(),
                    runes: vec![],
                });
            }
            Err(e) => {
                error!("Error getting tx out: {}", e);
            }
        }
    }

    Ok(Transaction {
        input: transaction.input,
        output: tx_outs,
    })
}

pub fn mempool_txids(index: Arc<Index>) -> Result<Vec<Txid>> {
    Ok(index.get_mempool_txids()?)
}
