use bitcoin::Txid;
use reqwest::{blocking::Client as BlockingReqwestClient, header::HeaderMap};
use std::str::FromStr;
use titan_types::*;

use crate::Error;

use super::TitanApiSync;

#[derive(Clone)]
pub struct SyncClient {
    /// The **blocking** HTTP client from `reqwest::blocking`.
    http_client: BlockingReqwestClient,
    /// The base URL for all endpoints (e.g. http://localhost:3030).
    base_url: String,
}

impl SyncClient {
    /// Creates a new `SyncClient` for the given `base_url`.
    pub fn new(base_url: &str) -> Self {
        Self {
            http_client: BlockingReqwestClient::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }
}

impl TitanApiSync for SyncClient {
    // 1) get_status
    fn get_status(&self) -> Result<Status, Error> {
        let url = format!("{}/status", self.base_url);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    // 2) get_tip
    fn get_tip(&self) -> Result<BlockTip, Error> {
        let url = format!("{}/tip", self.base_url);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    // 3) get_block
    fn get_block(&self, query: &query::Block) -> Result<Block, Error> {
        let url = format!("{}/block/{}", self.base_url, query);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    fn get_block_hash_by_height(&self, height: u64) -> Result<String, Error> {
        let url = format!("{}/block/{}/hash", self.base_url, height);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.text()?)
    }

    fn get_block_txids(&self, query: &query::Block) -> Result<Vec<String>, Error> {
        let url = format!("{}/block/{}/txids", self.base_url, query);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    // 4) get_address
    fn get_address(&self, address: &str) -> Result<AddressData, Error> {
        let url = format!("{}/address/{}", self.base_url, address);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    // 5) get_transaction
    fn get_transaction(&self, txid: &str) -> Result<Transaction, Error> {
        let url = format!("{}/tx/{}", self.base_url, txid);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    // 7) get_transaction_raw
    fn get_transaction_raw(&self, txid: &str) -> Result<Vec<u8>, Error> {
        let url = format!("{}/tx/{}/raw", self.base_url, txid);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.bytes()?.to_vec())
    }

    // 8) get_transaction_hex
    fn get_transaction_hex(&self, txid: &str) -> Result<String, Error> {
        let url = format!("{}/tx/{}/hex", self.base_url, txid);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.text()?)
    }

    // 9) send_transaction
    fn send_transaction(&self, tx_hex: String) -> Result<Txid, Error> {
        let url = format!("{}/tx/broadcast", self.base_url);
        let resp = self.http_client.post(&url).body(tx_hex).send()?;
        let status = resp.status();
        let body_text = resp.text()?;
        if !status.is_success() {
            return Err(Error::Runtime(format!(
                "Broadcast failed: HTTP {} - {}",
                status, body_text
            )));
        }
        let txid = Txid::from_str(&body_text)?;
        Ok(txid)
    }

    // 10) get_output
    fn get_output(&self, outpoint: &str) -> Result<TxOutEntry, Error> {
        let url = format!("{}/output/{}", self.base_url, outpoint);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    // 11) get_inscription
    fn get_inscription(&self, inscription_id: &str) -> Result<(HeaderMap, Vec<u8>), Error> {
        let url = format!("{}/inscription/{}", self.base_url, inscription_id);
        let resp = self.http_client.get(&url).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(Error::Runtime(format!(
                "Inscription request failed: HTTP {} - {}",
                status, body
            )));
        }
        let headers = resp.headers().clone();
        let bytes = resp.bytes()?.to_vec();
        Ok((headers, bytes))
    }

    // 12) get_runes
    fn get_runes(
        &self,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<RuneResponse>, Error> {
        let url = format!("{}/runes", self.base_url);
        let mut req = self.http_client.get(&url);
        if let Some(ref p) = pagination {
            req = req.query(&[("skip", p.skip), ("limit", p.limit)]);
        }
        let resp = req.send()?;
        Ok(resp.json()?)
    }

    // 13) get_rune
    fn get_rune(&self, rune: &str) -> Result<RuneResponse, Error> {
        let url = format!("{}/rune/{}", self.base_url, rune);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    // 14) get_rune_transactions
    fn get_rune_transactions(
        &self,
        rune: &str,
        pagination: Option<Pagination>,
    ) -> Result<PaginationResponse<Txid>, Error> {
        let url = format!("{}/rune/{}/transactions", self.base_url, rune);
        let mut req = self.http_client.get(&url);
        if let Some(ref p) = pagination {
            req = req.query(&[("skip", p.skip), ("limit", p.limit)]);
        }
        let resp = req.send()?;
        Ok(resp.json()?)
    }

    // 15) get_mempool_txids
    fn get_mempool_txids(&self) -> Result<Vec<Txid>, Error> {
        let url = format!("{}/mempool/txids", self.base_url);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    // 16) get_subscription
    fn get_subscription(&self, id: &str) -> Result<Subscription, Error> {
        let url = format!("{}/subscription/{}", self.base_url, id);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    // 17) list_subscriptions
    fn list_subscriptions(&self) -> Result<Vec<Subscription>, Error> {
        let url = format!("{}/subscriptions", self.base_url);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

    // 18) add_subscription
    fn add_subscription(&self, subscription: &Subscription) -> Result<Subscription, Error> {
        let url = format!("{}/subscription", self.base_url);
        let resp = self.http_client.post(&url).json(subscription).send()?;
        Ok(resp.json()?)
    }

    // 19) delete_subscription
    fn delete_subscription(&self, id: &str) -> Result<(), Error> {
        let url = format!("{}/subscription/{}", self.base_url, id);
        let resp = self.http_client.delete(&url).send()?;
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        if !status.is_success() {
            return Err(Error::Runtime(format!(
                "Delete subscription failed: HTTP {} - {}",
                status, body
            )));
        }
        Ok(())
    }
}
