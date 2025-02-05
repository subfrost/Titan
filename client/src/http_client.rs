use async_trait::async_trait;
use bitcoin::Txid;
use reqwest::{header::HeaderMap, Client as HttpClient};
use std::str::FromStr;
use titan_types::{
    query, AddressData, Block, BlockTip, Pagination, PaginationResponse, RuneResponse, Status,
    Subscription, Transaction, TxOutEntry,
};
use tokio::runtime::Runtime;

use crate::Error;

#[async_trait]
pub trait TitanApi {
    // 1) get_status
    async fn get_status(&self) -> Result<Status, Error>;
    fn get_status_blocking(&self) -> Result<Status, Error>;

    // 2) get_tip
    async fn get_tip(&self) -> Result<BlockTip, Error>;
    fn get_tip_blocking(&self) -> Result<BlockTip, Error>;

    // 3) get_block
    async fn get_block(&self, query: &query::Block) -> Result<Block, Error>;
    fn get_block_blocking(&self, query: &query::Block) -> Result<Block, Error>;
    async fn get_block_hash_by_height(&self, height: u64) -> Result<String, Error>;
    fn get_block_hash_by_height_blocking(&self, height: u64) -> Result<String, Error>;
    async fn get_block_txids(&self, query: &query::Block) -> Result<Vec<String>, Error>;
    fn get_block_txids_blocking(&self, query: &query::Block) -> Result<Vec<String>, Error>;

    // 4) get_address
    async fn get_address(&self, address: &str) -> Result<AddressData, Error>;
    fn get_address_blocking(&self, address: &str) -> Result<AddressData, Error>;

    // 5) get_transaction
    async fn get_transaction(&self, txid: &str) -> Result<Transaction, Error>;
    fn get_transaction_blocking(&self, txid: &str) -> Result<Transaction, Error>;

    // 7) get_transaction_raw
    async fn get_transaction_raw(&self, txid: &str) -> Result<Vec<u8>, Error>;
    fn get_transaction_raw_blocking(&self, txid: &str) -> Result<Vec<u8>, Error>;

    // 8) get_transaction_hex
    async fn get_transaction_hex(&self, txid: &str) -> Result<String, Error>;
    fn get_transaction_hex_blocking(&self, txid: &str) -> Result<String, Error>;

    // 9) send_transaction
    async fn send_transaction(&self, tx_hex: String) -> Result<Txid, Error>;
    fn send_transaction_blocking(&self, tx_hex: String) -> Result<Txid, Error>;

    // 10) get_output
    async fn get_output(&self, outpoint: &str) -> Result<TxOutEntry, Error>;
    fn get_output_blocking(&self, outpoint: &str) -> Result<TxOutEntry, Error>;

    // 11) get_inscription
    async fn get_inscription(&self, inscription_id: &str) -> Result<(HeaderMap, Vec<u8>), Error>;
    fn get_inscription_blocking(&self, inscription_id: &str)
        -> Result<(HeaderMap, Vec<u8>), Error>;

    // 12) get_runes
    async fn get_runes(
        &self,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<RuneResponse>, Error>;
    fn get_runes_blocking(
        &self,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<RuneResponse>, Error>;

    // 13) get_rune
    async fn get_rune(&self, rune: &str) -> Result<RuneResponse, Error>;
    fn get_rune_blocking(&self, rune: &str) -> Result<RuneResponse, Error>;

    // 14) get_rune_transactions
    async fn get_rune_transactions(
        &self,
        rune: &str,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<Txid>, Error>;
    fn get_rune_transactions_blocking(
        &self,
        rune: &str,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<Txid>, Error>;

    // 15) get_mempool_txids
    async fn get_mempool_txids(&self) -> Result<Vec<Txid>, Error>;
    fn get_mempool_txids_blocking(&self) -> Result<Vec<Txid>, Error>;

    // 16) get_subscription
    async fn get_subscription(&self, id: &str) -> Result<Subscription, Error>;
    fn get_subscription_blocking(&self, id: &str) -> Result<Subscription, Error>;

    // 17) list_subscriptions
    async fn list_subscriptions(&self) -> Result<Vec<Subscription>, Error>;
    fn list_subscriptions_blocking(&self) -> Result<Vec<Subscription>, Error>;

    // 18) add_subscription
    async fn add_subscription(&self, subscription: &Subscription) -> Result<Subscription, Error>;
    fn add_subscription_blocking(&self, subscription: &Subscription)
        -> Result<Subscription, Error>;

    // 19) delete_subscription
    async fn delete_subscription(&self, id: &str) -> Result<(), Error>;
    fn delete_subscription_blocking(&self, id: &str) -> Result<(), Error>;
}

/// A client for your HTTP server.
pub struct Client {
    http_client: HttpClient,
    base_url: String,
}

impl Client {
    /// Create a new client with the given base URL (e.g. "http://localhost:3030").
    pub fn new(base_url: &str) -> Self {
        Self {
            http_client: HttpClient::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

#[async_trait::async_trait]
impl TitanApi for Client {
    // ------------------------------------------------------------------------
    // 1) get_status
    // ------------------------------------------------------------------------
    async fn get_status(&self) -> Result<Status, Error> {
        let url = format!("{}/status", self.base_url);
        let resp = self.http_client.get(&url).send().await?;
        let status = resp.json::<Status>().await?;
        Ok(status)
    }
    fn get_status_blocking(&self) -> Result<Status, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_status())
    }

    // ------------------------------------------------------------------------
    // 2) get_tip
    // ------------------------------------------------------------------------
    async fn get_tip(&self) -> Result<BlockTip, Error> {
        let url = format!("{}/tip", self.base_url);
        let resp = self.http_client.get(&url).send().await?;
        let tip = resp.json::<BlockTip>().await?;
        Ok(tip)
    }
    fn get_tip_blocking(&self) -> Result<BlockTip, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_tip())
    }

    // ------------------------------------------------------------------------
    // 3) get_block
    // ------------------------------------------------------------------------
    async fn get_block(&self, query: &query::Block) -> Result<Block, Error> {
        let url = format!("{}/block/{}", self.base_url, query.to_string());
        let resp = self.http_client.get(&url).send().await?;
        let block = resp.json::<Block>().await?;
        Ok(block)
    }
    fn get_block_blocking(&self, query: &query::Block) -> Result<Block, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_block(query))
    }

    async fn get_block_hash_by_height(&self, height: u64) -> Result<String, Error> {
        let url = format!("{}/block/{}/hash", self.base_url, height);
        let resp = self.http_client.get(&url).send().await?;
        let hash = resp.text().await?;
        Ok(hash)
    }

    fn get_block_hash_by_height_blocking(&self, height: u64) -> Result<String, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_block_hash_by_height(height))
    }

    async fn get_block_txids(&self, query: &query::Block) -> Result<Vec<String>, Error> {
        let url = format!("{}/block/{}/txids", self.base_url, query.to_string());
        let resp = self.http_client.get(&url).send().await?;
        let txids = resp.json::<Vec<String>>().await?;
        Ok(txids)
    }

    fn get_block_txids_blocking(&self, query: &query::Block) -> Result<Vec<String>, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_block_txids(query))
    }

    // ------------------------------------------------------------------------
    // 4) get_address
    // ------------------------------------------------------------------------
    async fn get_address(&self, address: &str) -> Result<AddressData, Error> {
        let url = format!("{}/address/{}", self.base_url, address);
        let resp = self.http_client.get(&url).send().await?;
        let address_data = resp.json::<AddressData>().await?;
        Ok(address_data)
    }
    fn get_address_blocking(&self, address: &str) -> Result<AddressData, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_address(address))
    }

    // ------------------------------------------------------------------------
    // 5) get_transaction
    // ------------------------------------------------------------------------
    async fn get_transaction(&self, txid: &str) -> Result<Transaction, Error> {
        let url = format!("{}/tx/{}", self.base_url, txid);
        let resp = self.http_client.get(&url).send().await?;
        let tx = resp.json::<Transaction>().await?;
        Ok(tx)
    }
    fn get_transaction_blocking(&self, txid: &str) -> Result<Transaction, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_transaction(txid))
    }

    // ------------------------------------------------------------------------
    // 7) get_transaction_raw
    // ------------------------------------------------------------------------
    async fn get_transaction_raw(&self, txid: &str) -> Result<Vec<u8>, Error> {
        let url = format!("{}/tx/{}/raw", self.base_url, txid);
        let resp = self.http_client.get(&url).send().await?;
        let bytes = resp.bytes().await?.to_vec();
        Ok(bytes)
    }
    fn get_transaction_raw_blocking(&self, txid: &str) -> Result<Vec<u8>, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_transaction_raw(txid))
    }

    // ------------------------------------------------------------------------
    // 8) get_transaction_hex
    // ------------------------------------------------------------------------
    async fn get_transaction_hex(&self, txid: &str) -> Result<String, Error> {
        let url = format!("{}/tx/{}/hex", self.base_url, txid);
        let resp = self.http_client.get(&url).send().await?;
        let hex = resp.text().await?;
        Ok(hex)
    }
    fn get_transaction_hex_blocking(&self, txid: &str) -> Result<String, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_transaction_hex(txid))
    }

    // ------------------------------------------------------------------------
    // 9) broadcast_transaction
    // ------------------------------------------------------------------------
    async fn send_transaction(&self, tx_hex: String) -> Result<Txid, Error> {
        let url = format!("{}/tx/broadcast", self.base_url);
        let resp = self.http_client.post(&url).body(tx_hex).send().await?;
        let status = resp.status();
        let body_text = resp.text().await?;
        if !status.is_success() {
            return Err(Error::Runtime(format!(
                "Broadcast failed: HTTP {} - {}",
                status, body_text
            )));
        }
        let txid = Txid::from_str(&body_text)?;
        Ok(txid)
    }
    fn send_transaction_blocking(&self, tx_hex: String) -> Result<Txid, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.send_transaction(tx_hex))
    }

    // ------------------------------------------------------------------------
    // 10) get_output
    // ------------------------------------------------------------------------
    async fn get_output(&self, outpoint: &str) -> Result<TxOutEntry, Error> {
        let url = format!("{}/output/{}", self.base_url, outpoint);
        let resp = self.http_client.get(&url).send().await?;
        let output = resp.json::<TxOutEntry>().await?;
        Ok(output)
    }
    fn get_output_blocking(&self, outpoint: &str) -> Result<TxOutEntry, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_output(outpoint))
    }

    // ------------------------------------------------------------------------
    // 11) get_inscription
    // ------------------------------------------------------------------------
    async fn get_inscription(
        &self,
        inscription_id: &str,
    ) -> Result<(reqwest::header::HeaderMap, Vec<u8>), Error> {
        let url = format!("{}/inscription/{}", self.base_url, inscription_id);
        let resp = self.http_client.get(&url).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(Error::Runtime(format!(
                "Inscription request failed: HTTP {} - {}",
                status, body
            )));
        }
        let headers = resp.headers().clone();
        let body = resp.bytes().await?.to_vec();
        Ok((headers, body))
    }
    fn get_inscription_blocking(
        &self,
        inscription_id: &str,
    ) -> Result<(HeaderMap, Vec<u8>), Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_inscription(inscription_id))
    }

    // ------------------------------------------------------------------------
    // 12) get_runes
    // ------------------------------------------------------------------------
    async fn get_runes(
        &self,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<RuneResponse>, Error> {
        let url = format!("{}/runes", self.base_url);
        let mut req = self.http_client.get(&url);
        if let Some(ref p) = pagination {
            req = req.query(&[("skip", p.skip), ("limit", p.limit)]);
        }
        let resp = req.send().await?;
        let runes = resp.json::<PaginationResponse<RuneResponse>>().await?;
        Ok(runes)
    }
    fn get_runes_blocking(
        &self,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<RuneResponse>, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_runes(pagination))
    }

    // ------------------------------------------------------------------------
    // 13) get_rune
    // ------------------------------------------------------------------------
    async fn get_rune(&self, rune: &str) -> Result<RuneResponse, Error> {
        let url = format!("{}/rune/{}", self.base_url, rune);
        let resp = self.http_client.get(&url).send().await?;
        let rune_response = resp.json::<RuneResponse>().await?;
        Ok(rune_response)
    }
    fn get_rune_blocking(&self, rune: &str) -> Result<RuneResponse, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_rune(rune))
    }

    // ------------------------------------------------------------------------
    // 14) get_rune_transactions
    // ------------------------------------------------------------------------
    async fn get_rune_transactions(
        &self,
        rune: &str,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<Txid>, Error> {
        let url = format!("{}/rune/{}/transactions", self.base_url, rune);
        let mut req = self.http_client.get(&url);
        if let Some(ref p) = pagination {
            req = req.query(&[("skip", p.skip), ("limit", p.limit)]);
        }
        let resp = req.send().await?;
        let transactions = resp.json::<PaginationResponse<Txid>>().await?;
        Ok(transactions)
    }
    fn get_rune_transactions_blocking(
        &self,
        rune: &str,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<Txid>, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_rune_transactions(rune, pagination))
    }

    // ------------------------------------------------------------------------
    // 15) get_mempool_txids
    // ------------------------------------------------------------------------
    async fn get_mempool_txids(&self) -> Result<Vec<Txid>, Error> {
        let url = format!("{}/mempool/txids", self.base_url);
        let resp = self.http_client.get(&url).send().await?;
        let txids = resp.json::<Vec<Txid>>().await?;
        Ok(txids)
    }
    fn get_mempool_txids_blocking(&self) -> Result<Vec<Txid>, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_mempool_txids())
    }

    // ------------------------------------------------------------------------
    // 16) get_subscription
    // ------------------------------------------------------------------------
    async fn get_subscription(&self, id: &str) -> Result<Subscription, Error> {
        let url = format!("{}/subscription/{}", self.base_url, id);
        let resp = self.http_client.get(&url).send().await?;
        let subscription = resp.json::<Subscription>().await?;
        Ok(subscription)
    }
    fn get_subscription_blocking(&self, id: &str) -> Result<Subscription, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.get_subscription(id))
    }

    // ------------------------------------------------------------------------
    // 17) list_subscriptions
    // ------------------------------------------------------------------------
    async fn list_subscriptions(&self) -> Result<Vec<Subscription>, Error> {
        let url = format!("{}/subscriptions", self.base_url);
        let resp = self.http_client.get(&url).send().await?;
        let subscriptions = resp.json::<Vec<Subscription>>().await?;
        Ok(subscriptions)
    }
    fn list_subscriptions_blocking(&self) -> Result<Vec<Subscription>, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.list_subscriptions())
    }

    // ------------------------------------------------------------------------
    // 18) add_subscription
    // ------------------------------------------------------------------------
    async fn add_subscription(&self, subscription: &Subscription) -> Result<Subscription, Error> {
        let url = format!("{}/subscription", self.base_url);
        let resp = self
            .http_client
            .post(&url)
            .json(subscription)
            .send()
            .await?;
        let added = resp.json::<Subscription>().await?;
        Ok(added)
    }
    fn add_subscription_blocking(
        &self,
        subscription: &Subscription,
    ) -> Result<Subscription, Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.add_subscription(subscription))
    }

    // ------------------------------------------------------------------------
    // 19) delete_subscription
    // ------------------------------------------------------------------------
    async fn delete_subscription(&self, id: &str) -> Result<(), Error> {
        let url = format!("{}/subscription/{}", self.base_url, id);
        let resp = self.http_client.delete(&url).send().await?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(Error::Runtime(format!(
                "Delete subscription failed: HTTP {} - {}",
                status, body
            )));
        }
        Ok(())
    }
    fn delete_subscription_blocking(&self, id: &str) -> Result<(), Error> {
        let rt = Runtime::new().map_err(|e| Error::Runtime(e.to_string()))?;
        rt.block_on(self.delete_subscription(id))
    }
}
