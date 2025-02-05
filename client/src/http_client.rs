use bitcoin::Txid;
use reqwest::{header::HeaderMap, Client as HttpClient, Response};
use std::{error::Error, str::FromStr};
use titan_types::{
    AddressData, Block, BlockTip, Pagination, PaginationResponse, RuneResponse, Status,
    Subscription, Transaction, TxOutEntry,
};

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

    // ------------------------------------------------------------------------
    // Status, blocks, addresses
    // ------------------------------------------------------------------------

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

    // ------------------------------------------------------------------------
    // Transactions
    // ------------------------------------------------------------------------

    /// GET /tx/{txid}
    /// Returns a structured bitcoin `Transaction` (JSON).
    pub async fn get_transaction(
        &self,
        txid: &str,
    ) -> Result<bitcoin::Transaction, Box<dyn Error>> {
        let url = format!("{}/tx/{}", self.base_url, txid);
        let resp = self.http_client.get(&url).send().await?;
        let tx = resp.json::<bitcoin::Transaction>().await?;
        Ok(tx)
    }

    /// Same as `get_transaction` but always requests runes (`with_runes=true`).
    pub async fn get_transaction_with_runes(
        &self,
        txid: &str,
    ) -> Result<Transaction, Box<dyn Error>> {
        // Build query-string for the `with_runes` parameter.
        let url = format!("{}/tx/{}?with_runes=true", self.base_url, txid);
        let resp = self.http_client.get(&url).send().await?;
        let tx = resp.json::<Transaction>().await?;
        Ok(tx)
    }

    /// GET /tx/{txid}/raw
    /// Returns the raw transaction bytes (content-type: application/octet-stream).
    pub async fn get_transaction_raw(&self, txid: &str) -> Result<Vec<u8>, Box<dyn Error>> {
        let url = format!("{}/tx/{txid}/raw", self.base_url);
        let resp = self.http_client.get(&url).send().await?;
        // Read the raw bytes directly
        let bytes = resp.bytes().await?.to_vec();
        Ok(bytes)
    }

    /// GET /tx/{txid}/hex
    /// Returns a text/plain body containing hex-encoded raw transaction.
    pub async fn get_transaction_hex(&self, txid: &str) -> Result<String, Box<dyn Error>> {
        let url = format!("{}/tx/{txid}/hex", self.base_url);
        let resp = self.http_client.get(&url).send().await?;
        let hex = resp.text().await?; // text/plain
        Ok(hex)
    }

    /// POST /tx/broadcast
    /// Broadcast a raw transaction (in hex form) to the network.
    pub async fn broadcast_transaction(&self, tx_hex: String) -> Result<Txid, Box<dyn Error>> {
        let url = format!("{}/tx/broadcast", self.base_url);

        // 1) Build the request, consuming our owned `tx_hex`.
        let resp = self
            .http_client
            .post(&url)
            .body(tx_hex) // note: we pass the owned string here
            .send()
            .await?;

        // 2) Extract status and body text. We can't read `resp.text()` twice,
        //    so we store them now before checking the status.
        let status = resp.status();
        let body_text = resp.text().await?;

        // 3) If not successful, return an error with the body as context.
        if !status.is_success() {
            return Err(format!("Broadcast failed: HTTP {} - {}", status, body_text).into());
        }

        // 4) At this point, we know it's success. The response body is the txid as text.
        let txid = Txid::from_str(&body_text)?;
        Ok(txid)
    }

    /// GET /output/{outpoint}
    pub async fn get_output(&self, outpoint: &str) -> Result<TxOutEntry, Box<dyn Error>> {
        let url = format!("{}/output/{}", self.base_url, outpoint);
        let resp = self.http_client.get(&url).send().await?;
        let output = resp.json::<TxOutEntry>().await?;
        Ok(output)
    }

    // ------------------------------------------------------------------------
    // Inscriptions
    // ------------------------------------------------------------------------

    /// GET /inscription/{inscription_id}
    /// Returns `(Headers, Bytes)`. The server sets content-type and possibly compression,
    /// and the body is the raw inscription content.
    pub async fn get_inscription(
        &self,
        inscription_id: &str,
    ) -> Result<(HeaderMap, Vec<u8>), Box<dyn Error>> {
        let url = format!("{}/inscription/{}", self.base_url, inscription_id);
        let resp: Response = self.http_client.get(&url).send().await?;

        let status = resp.status();

        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Inscription request failed: HTTP {} - {}", status, body).into());
        }
        let headers = resp.headers().clone();
        let body = resp.bytes().await?.to_vec();
        Ok((headers, body))
    }

    // ------------------------------------------------------------------------
    // Runes
    // ------------------------------------------------------------------------

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

    // ------------------------------------------------------------------------
    // Subscription endpoints
    // ------------------------------------------------------------------------

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
        let resp = self.http_client.delete(&url).send().await?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(format!("Delete subscription failed: HTTP {} - {}", status, body).into());
        }
        Ok(())
    }
}
