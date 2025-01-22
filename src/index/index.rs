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
        models::{
            Block, Inscription, InscriptionId, Pagination, PaginationResponse, RuneEntry,
            TxOutEntry,
        },
    },
    bitcoin::{BlockHash, OutPoint, Txid},
    ordinals::{Rune, RuneId},
    std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread::{self},
        time::Duration,
    },
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
    pub fn new(db: Arc<dyn Store + Send + Sync>, settings: Settings) -> Self {
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
            )),
            shutdown_flag,
            zmq_manager: Arc::new(zmq_manager),
        }
    }

    pub fn validate_index(&self) -> Result<()> {
        let db_index_spent_outputs = self.db.is_index_spent_outputs()?;
        match (self.settings.index_spent_outputs, db_index_spent_outputs) {
            (true, Some(false)) => {
                return Err(IndexError::InvalidIndex(
                    "index_spent_outputs is not set. Disable index_spent_outputs in settings or clean up the database".to_string(),
                ));
            }
            (true, None) => {
                self.db.set_index_spent_outputs(true)?;
            }
            (false, Some(true)) | (false, None) => {
                self.db.set_index_spent_outputs(false)?;
            }
            _ => {}
        }

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

            if self.updater.is_at_tip() && !self.shutdown_flag.load(Ordering::SeqCst) {
                self.updater
                    .index_mempool()
                    .expect("Failed to index mempool");
            }

            thread::sleep(Duration::from_millis(self.settings.main_loop_interval));
        }

        self.zmq_manager.join_zmq_listener();
        info!("Closing indexer");
    }

    pub fn start_zmq_listener(&self) {
        self.zmq_manager.start_zmq_listener(self.updater.clone());
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
        Ok(self.db.get_mempool_txids()?.into_iter().collect())
    }

    pub fn get_tx_out(&self, outpoint: &OutPoint) -> Result<TxOutEntry> {
        Ok(self.db.get_tx_out(outpoint, None)?)
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
    ) -> Result<PaginationResponse<String>> {
        Ok(self
            .db
            .get_last_rune_transactions(rune_id, pagination, mempool)?)
    }
}
