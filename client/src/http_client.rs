use bitcoin::Txid;
use reqwest::{header::HeaderMap, Client as HttpClient, Response};
use types::{
    AddressData, Block, BlockTip, Pagination, PaginationResponse, RuneResponse, Status,
    Subscription, Transaction, TxOutEntry,
};
use std::error::Error;

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

    /// GET /status
    pub async fn get_status(&self) -> Result<Status, Box<dyn Error>> {
        let url = format!("{}/status", self.base_url);
        let resp = self.http_client.get(&url).send().await?;
        let status = resp.json::<Status>().await?;
        Ok(status)
    }

    /// GET /tip
    pub async fn get_tip(&self) -> Result<BlockTip, Box<dyn Error>> {
        let url = format!("{}/tip", self.base_url);
        let resp = self.http_client.get(&url).send().await?;
        let tip = resp.json::<BlockTip>().await?;
        Ok(tip)
    }

    /// GET /block/{query}
    pub async fn get_block(&self, query: &str) -> Result<Block, Box<dyn Error>> {
        let url = format!("{}/block/{}", self.base_url, query);
        let resp = self.http_client.get(&url).send().await?;
        let block = resp.json::<Block>().await?;
        Ok(block)
    }

    /// GET /address/{address}
    pub async fn get_address(&self, address: &str) -> Result<AddressData, Box<dyn Error>> {
        let url = format!("{}/address/{}", self.base_url, address);
        let resp = self.http_client.get(&url).send().await?;
        let address_data = resp.json::<AddressData>().await?;
        Ok(address_data)
    }

    /// GET /tx/{txid}
    pub async fn get_tx(&self, txid: &str) -> Result<Transaction, Box<dyn Error>> {
        let url = format!("{}/tx/{}", self.base_url, txid);
        let resp = self.http_client.get(&url).send().await?;
        let tx = resp.json::<Transaction>().await?;
        Ok(tx)
    }

    /// GET /output/{outpoint}
    pub async fn get_output(&self, outpoint: &str) -> Result<TxOutEntry, Box<dyn Error>> {
        let url = format!("{}/output/{}", self.base_url, outpoint);
        let resp = self.http_client.get(&url).send().await?;
        let output = resp.json::<TxOutEntry>().await?;
        Ok(output)
    }

    /// GET /inscription/{inscription_id}
    pub async fn get_inscription(
        &self,
        inscription_id: &str,
    ) -> Result<(HeaderMap, Vec<u8>), Box<dyn Error>> {
        let url = format!("{}/inscription/{}", self.base_url, inscription_id);
        let resp: Response = self.http_client.get(&url).send().await?;
        let headers = resp.headers().clone();
        let body = resp.bytes().await?.to_vec();
        Ok((headers, body))
    }

    /// GET /runes?skip=&limit=
    pub async fn get_runes(
        &self,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<RuneResponse>, Box<dyn Error>> {
        let url = format!("{}/runes", self.base_url);
        let mut req = self.http_client.get(&url);
        if let Some(pagination) = pagination {
            req = req.query(&[("skip", pagination.skip), ("limit", pagination.limit)]);
        }
        let resp = req.send().await?;
        let runes = resp.json::<PaginationResponse<RuneResponse>>().await?;
        Ok(runes)
    }

    /// GET /rune/{rune}
    pub async fn get_rune(&self, rune: &str) -> Result<RuneResponse, Box<dyn Error>> {
        let url = format!("{}/rune/{}", self.base_url, rune);
        let resp = self.http_client.get(&url).send().await?;
        let rune_response = resp.json::<RuneResponse>().await?;
        Ok(rune_response)
    }

    /// GET /rune/{rune}/transactions?skip=&limit=
    pub async fn get_rune_transactions(
        &self,
        rune: &str,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<Txid>, Box<dyn Error>> {
        let url = format!("{}/rune/{}/transactions", self.base_url, rune);
        let mut req = self.http_client.get(&url);
        if let Some(pagination) = pagination {
            req = req.query(&[("skip", pagination.skip), ("limit", pagination.limit)]);
        }
        let resp = req.send().await?;
        let transactions = resp.json::<PaginationResponse<Txid>>().await?;
        Ok(transactions)
    }

    /// GET /mempool/txids
    pub async fn get_mempool_txids(&self) -> Result<Vec<Txid>, Box<dyn Error>> {
        let url = format!("{}/mempool/txids", self.base_url);
        let resp = self.http_client.get(&url).send().await?;
        let txids = resp.json::<Vec<Txid>>().await?;
        Ok(txids)
    }

    // Subscription endpoints:

    /// GET /subscription/{id}
    pub async fn get_subscription(&self, id: &str) -> Result<Subscription, Box<dyn Error>> {
        let url = format!("{}/subscription/{}", self.base_url, id);
        let resp = self.http_client.get(&url).send().await?;
        let subscription = resp.json::<Subscription>().await?;
        Ok(subscription)
    }

    /// GET /subscriptions
    pub async fn list_subscriptions(&self) -> Result<Vec<Subscription>, Box<dyn Error>> {
        let url = format!("{}/subscriptions", self.base_url);
        let resp = self.http_client.get(&url).send().await?;
        let subscriptions = resp.json::<Vec<Subscription>>().await?;
        Ok(subscriptions)
    }

    /// POST /subscription
    pub async fn add_subscription(
        &self,
        subscription: &Subscription,
    ) -> Result<Subscription, Box<dyn Error>> {
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

    /// DELETE /subscription/{id}
    pub async fn delete_subscription(&self, id: &str) -> Result<(), Box<dyn Error>> {
        let url = format!("{}/subscription/{}", self.base_url, id);
        self.http_client.delete(&url).send().await?;
        Ok(())
    }
}
