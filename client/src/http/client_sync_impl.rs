use bitcoin::Txid;
use reqwest::{blocking::Client as BlockingReqwestClient, header::HeaderMap, StatusCode};
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

    fn call_text(&self, path: &str) -> Result<String, Error> {
        let url = format!("{}{}", self.base_url, path);
        let response = self.http_client.get(&url).send()?;
        if response.status().is_success() {
            Ok(response.text()?)
        } else {
            Err(Error::TitanError(response.status(), response.text()?))
        }
    }

    fn call_bytes(&self, path: &str) -> Result<Vec<u8>, Error> {
        let url = format!("{}{}", self.base_url, path);
        let response = self.http_client.get(&url).send()?;
        if response.status().is_success() {
            Ok(response.bytes()?.to_vec())
        } else {
            Err(Error::TitanError(response.status(), response.text()?))
        }
    }

    fn post_text(&self, path: &str, body: String) -> Result<String, Error> {
        let url = format!("{}{}", self.base_url, path);

        let response = self.http_client.post(&url).body(body).send()?;

        if response.status().is_success() {
            Ok(response.text()?)
        } else {
            Err(Error::TitanError(response.status(), response.text()?))
        }
    }

    fn delete(&self, path: &str) -> Result<(), Error> {
        let url = format!("{}{}", self.base_url, path);
        let response = self.http_client.delete(&url).send()?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(Error::TitanError(response.status(), response.text()?))
        }
    }
}

impl TitanApiSync for SyncClient {
    fn get_status(&self) -> Result<Status, Error> {
        let text = self.call_text("/status")?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn get_tip(&self) -> Result<BlockTip, Error> {
        let text = self.call_text("/tip")?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn get_block(&self, query: &query::Block) -> Result<Block, Error> {
        let text = self.call_text(&format!("/block/{}", query))?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn get_block_hash_by_height(&self, height: u64) -> Result<String, Error> {
        self.call_text(&format!("/block/{}/hash", height))
    }

    fn get_block_txids(&self, query: &query::Block) -> Result<Vec<String>, Error> {
        let text = self.call_text(&format!("/block/{}/txids", query))?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn get_address(&self, address: &str) -> Result<AddressData, Error> {
        let text = self.call_text(&format!("/address/{}", address))?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn get_transaction(&self, txid: &str) -> Result<Transaction, Error> {
        let text = self.call_text(&format!("/tx/{}", txid))?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn get_transaction_raw(&self, txid: &str) -> Result<Vec<u8>, Error> {
        self.call_bytes(&format!("/tx/{}/raw", txid))
    }

    fn get_transaction_hex(&self, txid: &str) -> Result<String, Error> {
        self.call_text(&format!("/tx/{}/hex", txid))
    }

    fn get_transaction_status(&self, txid: &str) -> Result<TransactionStatus, Error> {
        let text = self.call_text(&format!("/tx/{}/status", txid))?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn send_transaction(&self, tx_hex: String) -> Result<Txid, Error> {
        let text = self.post_text("/tx/broadcast", tx_hex)?;
        Txid::from_str(&text).map_err(Error::from)
    }

    fn get_output(&self, outpoint: &str) -> Result<TxOutEntry, Error> {
        let text = self.call_text(&format!("/output/{}", outpoint))?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn get_inscription(&self, inscription_id: &str) -> Result<(HeaderMap, Vec<u8>), Error> {
        let url = format!("{}/inscription/{}", self.base_url, inscription_id);
        let resp = self.http_client.get(&url).send()?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(Error::TitanError(status, body));
        }
        let headers = resp.headers().clone();
        let bytes = resp.bytes()?.to_vec();
        Ok((headers, bytes))
    }

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

    fn get_rune(&self, rune: &str) -> Result<RuneResponse, Error> {
        let url = format!("{}/rune/{}", self.base_url, rune);
        let resp = self.http_client.get(&url).send()?;
        Ok(resp.json()?)
    }

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

    fn get_mempool_txids(&self) -> Result<Vec<Txid>, Error> {
        let text = self.call_text("/mempool/txids")?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn get_subscription(&self, id: &str) -> Result<Subscription, Error> {
        let text = self.call_text(&format!("/subscription/{}", id))?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn list_subscriptions(&self) -> Result<Vec<Subscription>, Error> {
        let text = self.call_text("/subscriptions")?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn add_subscription(&self, subscription: &Subscription) -> Result<Subscription, Error> {
        let text = self.post_text("/subscription", serde_json::to_string(subscription)?)?;
        serde_json::from_str(&text).map_err(Error::from)
    }

    fn delete_subscription(&self, id: &str) -> Result<(), Error> {
        self.delete(&format!("/subscription/{}", id))
    }
}
