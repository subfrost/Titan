use {
    super::{
        content::{content_response, AcceptEncoding, ContentError},
        query::{to_hash, to_rune_id},
    },
    crate::{
        index::{Index, IndexError},
        subscription::{self, WebhookSubscriptionManager},
    },
    bitcoin::{consensus, Address, OutPoint, Txid},
    bitcoincore_rpc::{Client, RpcApi},
    http::HeaderMap,
    std::sync::Arc,
    titan_types::{
        query, AddressData, Block, BlockTip, InscriptionId, Pagination, PaginationResponse,
        RuneResponse, Status, Subscription, Transaction, TransactionStatus, TxOutEntry,
    },
    tracing::error,
    uuid::Uuid,
};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("not found: {0}")]
    IndexError(#[from] IndexError),
    #[error("rpc error: {0}")]
    RpcError(#[from] bitcoincore_rpc::Error),
    #[error("content error: {0}")]
    ContentError(#[from] ContentError),
    #[error("subscription error: {0}")]
    SubscriptionError(#[from] subscription::WebhookStoreError),
    #[error("hex error: {0}")]
    HexError(#[from] hex::FromHexError),
    #[error("consensus error: {0}")]
    ConsensusError(#[from] consensus::encode::Error),
}

pub type Result<T> = std::result::Result<T, ApiError>;

pub fn tip(index: Arc<Index>) -> Result<BlockTip> {
    let block_count = index.get_block_count()?;
    let block_hash = index.get_block_hash(block_count - 1)?;
    Ok(BlockTip {
        height: block_count - 1,
        hash: block_hash.to_string(),
    })
}

pub fn status(index: Arc<Index>) -> Result<Status> {
    let block_count = index.get_block_count()?;
    let block_hash = index.get_block_hash(block_count - 1)?;
    Ok(Status {
        block_tip: BlockTip {
            height: block_count - 1,
            hash: block_hash.to_string(),
        },
        runes_count: index.get_runes_count()?,
        mempool_tx_count: index.get_mempool_txids()?.len() as u64,
    })
}

pub fn block(index: Arc<Index>, block: &query::Block) -> Result<Block> {
    let hash = to_hash(block, &index)?;
    Ok(index.get_block_by_hash(&hash)?)
}

pub fn block_hash_by_height(index: Arc<Index>, height: u64) -> Result<String> {
    let hash = index.get_block_hash(height)?;
    Ok(hash.to_string())
}

pub fn block_txids(index: Arc<Index>, block: &query::Block) -> Result<Vec<String>> {
    let hash = to_hash(block, &index)?;
    let block = index.get_block_by_hash(&hash)?;
    Ok(block.tx_ids)
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
    let rune_id = to_rune_id(rune_query, &index)?;
    let block_count = index.get_block_count()?;
    let rune_response = index
        .get_rune(&rune_id)?
        .to_rune_response(rune_id, block_count - 1);
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
        .map(|(rune_id, rune_entry)| rune_entry.to_rune_response(rune_id, block_count))
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
) -> Result<PaginationResponse<Txid>> {
    let rune_id = to_rune_id(rune_query, &index)?;
    let transactions = index.get_last_rune_transactions(&rune_id, pagination, None)?;
    Ok(transactions)
}

pub fn broadcast_transaction(index: Arc<Index>, client: Client, hex: &str) -> Result<Txid> {
    let txid = client.send_raw_transaction(hex)?;
    let transaction = consensus::deserialize(&hex::decode(hex)?)?;
    index.index_new_transaction(&txid, &transaction);
    Ok(txid)
}

pub fn bitcoin_transaction_raw(index: Arc<Index>, client: Client, txid: &Txid) -> Result<Vec<u8>> {
    if index.is_indexing_bitcoin_transactions() {
        Ok(index.get_transaction_raw(txid)?)
    } else {
        Ok(consensus::serialize(
            &client.get_raw_transaction(txid, None)?,
        ))
    }
}

pub fn bitcoin_transaction_hex(index: Arc<Index>, client: Client, txid: &Txid) -> Result<String> {
    let transaction = bitcoin_transaction_raw(index, client, txid)?;
    Ok(hex::encode(transaction))
}

pub fn transaction(index: Arc<Index>, client: Client, txid: &Txid) -> Result<Transaction> {
    let mut transaction = Transaction::from(if index.is_indexing_bitcoin_transactions() {
        index.get_transaction(txid)?
    } else {
        let mut transaction = Transaction::from(client.get_raw_transaction(txid, None)?);
        transaction.status = Some(index.get_transaction_status(txid)?);
        transaction
    });

    let outpoints = transaction
        .output
        .iter()
        .enumerate()
        .map(|(vout, _)| OutPoint {
            txid: txid.clone(),
            vout: vout as u32,
        })
        .collect();

    let tx_outs = index.get_tx_outs(&outpoints)?;

    for (vout, tx_out) in transaction.output.iter_mut().enumerate() {
        let outpoint = outpoints[vout];
        match tx_outs.get(&outpoint) {
            Some(tx_out_entry) => {
                tx_out.runes = tx_out_entry.runes.clone();
                tx_out.risky_runes = tx_out_entry.risky_runes.clone();
                tx_out.spent = tx_out_entry.spent.clone();
            }
            None => {
                // Ignore.
            }
        }
    }

    Ok(transaction)
}

pub fn transaction_status(index: Arc<Index>, txid: &Txid) -> Result<TransactionStatus> {
    Ok(index.get_transaction_status(txid)?)
}

pub fn mempool_txids(index: Arc<Index>) -> Result<Vec<Txid>> {
    Ok(index.get_mempool_txids()?)
}

pub fn address(index: Arc<Index>, address: &Address) -> Result<AddressData> {
    let script_pubkey = address.script_pubkey();
    let outpoints = index.get_script_pubkey_outpoints(&script_pubkey)?;
    Ok(outpoints)
}

pub fn subscriptions(
    subscription_manager: Arc<WebhookSubscriptionManager>,
) -> Result<Vec<Subscription>> {
    Ok(subscription_manager.get_subscriptions()?)
}

pub fn add_subscription(
    subscription_manager: Arc<WebhookSubscriptionManager>,
    subscription: Subscription,
) -> Result<()> {
    Ok(subscription_manager.add_subscription(&subscription)?)
}

pub fn delete_subscription(
    subscription_manager: Arc<WebhookSubscriptionManager>,
    id: Uuid,
) -> Result<()> {
    Ok(subscription_manager.delete_subscription(&id)?)
}

pub fn get_subscription(
    subscription_manager: Arc<WebhookSubscriptionManager>,
    id: Uuid,
) -> Result<Subscription> {
    Ok(subscription_manager.get_subscription(&id)?)
}
