use {
    super::{
        metrics::Metrics,
        settings::Settings,
        store::{Store, StoreError},
        updater::Updater,
        zmq::ZmqManager,
        RpcClientError,
    },
    crate::{
        index::updater::{ReorgError, UpdaterError},
        models::{block_id_to_transaction_status, Inscription, RuneEntry},
    },
    bitcoin::{Address, BlockHash, OutPoint, Transaction as BitcoinTransaction, Txid},
    ordinals::{Rune, RuneId},
    std::{
        collections::HashMap,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread::{self},
        time::Duration,
    },
    titan_types::{
        AddressData, AddressTxOut, Block, Event, InscriptionId, MempoolEntry, Pagination,
        PaginationResponse, RuneAmount, Transaction, TransactionStatus, TxOutEntry,
    },
    tokio::{runtime::Runtime, sync::mpsc::Sender},
    tracing::{error, info, warn},
};

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("store error: {0}")]
    StoreError(#[from] StoreError),
    #[error("invalid index: {0}")]
    InvalidIndex(String),
    #[error("rpc client error: {0}")]
    RpcClientError(#[from] RpcClientError),
    #[error("rpc api error: {0}")]
    RpcApiError(#[from] bitcoincore_rpc::Error),
    #[error("updater error: {0}")]
    UpdaterError(#[from] UpdaterError),
}

type Result<T> = std::result::Result<T, IndexError>;

pub struct Index {
    db: Arc<dyn Store + Send + Sync>,
    settings: Settings,
    updater: Arc<Updater>,
    shutdown_flag: Arc<AtomicBool>,

    zmq_manager: Arc<ZmqManager>,
}

impl Index {
    pub fn new(
        db: Arc<dyn Store + Send + Sync>,
        settings: Settings,
        sender: Option<Sender<Event>>,
    ) -> Self {
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let metrics = Metrics::new();
        metrics.start(shutdown_flag.clone());

        let zmq_manager = ZmqManager::new(settings.zmq_endpoint.clone());
        Self {
            db: db.clone(),
            settings: settings.clone(),
            updater: Arc::new(Updater::new(
                db.clone(),
                settings.clone(),
                &metrics,
                shutdown_flag.clone(),
                sender,
            )),
            shutdown_flag,
            zmq_manager: Arc::new(zmq_manager),
        }
    }

    pub fn validate_index(&self) -> Result<()> {
        let db_index_addresses = self.db.is_index_addresses()?;
        match (self.settings.index_addresses, db_index_addresses) {
            (true, Some(false)) => {
                return Err(IndexError::InvalidIndex(
                    "index_addresses is not set. Disable index_addresses in settings or clean up the database".to_string(),
                ));
            }
            (true, None) => {
                self.db.set_index_addresses(true)?;
            }
            (false, Some(true)) | (false, None) => {
                self.db.set_index_addresses(false)?;
            }
            _ => {}
        }

        let db_index_bitcoin_transactions = self.db.is_index_bitcoin_transactions()?;
        match (
            self.settings.index_bitcoin_transactions,
            db_index_bitcoin_transactions,
        ) {
            (true, Some(false)) => {
                return Err(IndexError::InvalidIndex(
                    "index_bitcoin_transactions is not set. Disable index_bitcoin_transactions in settings or clean up the database".to_string(),
                ));
            }
            (true, None) => {
                self.db.set_index_bitcoin_transactions(true)?;
            }
            (false, Some(true)) | (false, None) => {
                self.db.set_index_bitcoin_transactions(false)?;
            }
            _ => {}
        }

        Ok(())
    }

    pub fn shutdown(&self) {
        self.shutdown_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.zmq_manager.shutdown();
    }

    pub fn index(&self) {
        loop {
            if self.shutdown_flag.load(Ordering::SeqCst) {
                info!("Indexer received shutdown signal, stopping...");
                break;
            }

            match self.updater.update_to_tip() {
                Ok(()) => (),
                Err(UpdaterError::BitcoinReorg(ReorgError::Unrecoverable)) => {
                    error!("Unrecoverable reorg detected. stopping indexer loop.");
                    break;
                }
                Err(UpdaterError::BitcoinReorg(ReorgError::Recoverable {
                    height: _,
                    depth: _,
                })) => {
                    continue;
                }
                Err(UpdaterError::BitcoinRpc(_)) => {
                    warn!("We're getting network connection issues, retrying...");
                    continue;
                }
                Err(e) => {
                    error!("Failed to update to tip: {}", e);
                    break;
                }
            }

            match self.updater.index_mempool() {
                Ok(_) => (),
                Err(UpdaterError::BitcoinRpc(_)) => {
                    warn!("We're getting network connection issues, retrying...");
                    continue;
                }
                Err(e) => {
                    error!("Failed to index mempool: {}", e);
                    break;
                }
            }

            match self.updater.notify_tx_updates(false) {
                Ok(_) => (),
                Err(UpdaterError::InvalidMainChainTip) => {
                    warn!("Invalid main chain tip, when sending tx updates...");
                    continue;
                }
                Err(e) => {
                    error!("Failed to notify tx updates: {}", e);
                }
            }

            thread::sleep(Duration::from_millis(self.settings.main_loop_interval));
        }

        let rt = Runtime::new().expect("Failed to create runtime");
        rt.block_on(self.zmq_manager.join_zmq_listener());
        info!("Closing indexer");
    }

    pub async fn start_zmq_listener(&self) {
        self.zmq_manager
            .start_zmq_listener(self.updater.clone())
            .await;
    }

    pub fn get_block_count(&self) -> Result<u64> {
        Ok(self.db.get_block_count()?)
    }

    pub fn get_block_hash(&self, height: u64) -> Result<BlockHash> {
        Ok(self.db.get_block_hash(height)?)
    }

    pub fn get_block_by_hash(&self, hash: &BlockHash) -> Result<Block> {
        Ok(self.db.get_block_by_hash(hash)?)
    }

    pub fn get_mempool_txids(&self) -> Result<Vec<Txid>> {
        Ok(self.db.get_mempool_txids()?.keys().cloned().collect())
    }

    pub fn get_mempool_entry(&self, txid: &Txid) -> Result<MempoolEntry> {
        Ok(self.db.get_mempool_entry(txid)?)
    }

    pub fn get_mempool_entries(
        &self,
        txids: &Vec<Txid>,
    ) -> Result<HashMap<Txid, Option<MempoolEntry>>> {
        Ok(self.db.get_mempool_entries(txids)?)
    }

    pub fn get_tx_out(&self, outpoint: &OutPoint) -> Result<TxOutEntry> {
        Ok(self
            .db
            .get_tx_out_with_mempool_spent_update(outpoint, None)?)
    }

    pub fn get_tx_outs(&self, outpoints: &Vec<OutPoint>) -> Result<HashMap<OutPoint, TxOutEntry>> {
        Ok(self
            .db
            .get_tx_outs_with_mempool_spent_update(outpoints, None)?)
    }

    pub fn get_rune(&self, rune_id: &RuneId) -> Result<RuneEntry> {
        Ok(self.db.get_rune(rune_id)?)
    }

    pub fn get_runes(
        &self,
        pagination: Pagination,
    ) -> Result<PaginationResponse<(RuneId, RuneEntry)>> {
        Ok(self.db.get_runes(pagination)?)
    }

    pub fn get_rune_id(&self, rune: &Rune) -> Result<RuneId> {
        Ok(self.db.get_rune_id(rune)?)
    }

    pub fn get_runes_count(&self) -> Result<u64> {
        Ok(self.db.get_runes_count()?)
    }

    pub fn get_inscription(&self, inscription_id: &InscriptionId) -> Result<Inscription> {
        Ok(self.db.get_inscription(inscription_id)?)
    }

    pub fn get_last_rune_transactions(
        &self,
        rune_id: &RuneId,
        pagination: Option<Pagination>,
        mempool: Option<bool>,
    ) -> Result<PaginationResponse<Txid>> {
        Ok(self
            .db
            .get_last_rune_transactions(rune_id, pagination, mempool)?)
    }

    pub fn get_script_pubkey_outpoints(&self, address: &Address) -> Result<AddressData> {
        let script_pubkey = address.script_pubkey();
        let outpoints = self.db.get_script_pubkey_outpoints(&script_pubkey, None)?;
        let outpoints_to_tx_out: HashMap<OutPoint, TxOutEntry> = self
            .db
            .get_tx_outs_with_mempool_spent_update(&outpoints, None)?;

        if outpoints.len() != outpoints_to_tx_out.len() {
            error!(
                "Address {} has {} outpoints but {} txouts",
                address,
                outpoints.len(),
                outpoints_to_tx_out.len()
            );
        }

        let outpoint_txns: Vec<Txid> = outpoints.iter().map(|outpoint| outpoint.txid).collect();
        let txns_confirming_block = self.db.get_transaction_confirming_blocks(&outpoint_txns)?;

        let mut runes = HashMap::new();
        let mut value = 0;
        let mut outputs = Vec::new();
        for (outpoint, tx_out) in outpoints_to_tx_out {
            for rune in tx_out.runes.iter() {
                runes
                    .entry(rune.rune_id)
                    .and_modify(|amount| *amount += rune.amount)
                    .or_insert(rune.amount);
            }

            for rune in tx_out.risky_runes.iter() {
                runes
                    .entry(rune.rune_id)
                    .and_modify(|amount| *amount += rune.amount)
                    .or_insert(rune.amount);
            }

            value += tx_out.value;

            let output = AddressTxOut::from((
                outpoint,
                tx_out,
                block_id_to_transaction_status(
                    txns_confirming_block
                        .get(&outpoint.txid)
                        .and_then(|x| x.as_ref()),
                ),
            ));

            outputs.push(output);
        }

        Ok(AddressData {
            value,
            runes: runes
                .into_iter()
                .map(|(rune_id, amount)| RuneAmount::from((rune_id, amount)))
                .collect(),
            outputs,
        })
    }

    pub fn is_indexing_bitcoin_transactions(&self) -> bool {
        self.settings.index_bitcoin_transactions
    }

    pub fn get_transaction_raw(&self, txid: &Txid) -> Result<Vec<u8>> {
        Ok(self.db.get_transaction_raw(txid, None)?)
    }

    pub fn get_transaction(&self, txid: &Txid) -> Result<Transaction> {
        Ok(self.db.get_transaction(txid, None)?)
    }

    pub fn get_transaction_status(&self, txid: &Txid) -> Result<TransactionStatus> {
        let result = self.db.get_transaction_confirming_block(txid);
        match result {
            Ok(block_id) => Ok(block_id.into_transaction_status()),
            Err(StoreError::NotFound(_)) => {
                // If the transaction is not found, we will return a not found error.
                self.db.get_transaction_raw(txid, None)?;

                // If it's found, then it's unconfirmed.
                Ok(TransactionStatus::unconfirmed())
            }
            Err(e) => Err(IndexError::StoreError(e)),
        }
    }

    pub fn pre_index_new_submitted_transaction(&self, txid: &Txid) -> Result<()> {
        Ok(self.updater.pre_index_new_submitted_transaction(txid)?)
    }

    pub fn remove_pre_index_new_submitted_transaction(&self, txid: &Txid) -> Result<()> {
        Ok(self
            .updater
            .remove_pre_index_new_submitted_transaction(txid)?)
    }

    pub fn index_new_submitted_transaction(
        &self,
        txid: &Txid,
        tx: &BitcoinTransaction,
        mempool_entry: MempoolEntry,
    ) {
        match self.updater.index_new_submitted_tx(txid, tx, mempool_entry) {
            Ok(_) => (),
            Err(e) => {
                error!("Failed to index new transaction after broadcast: {}", e);
            }
        }
    }
}
