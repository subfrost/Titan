use {
    super::{
        metrics::Metrics,
        settings::Settings,
        store::{Store, StoreError},
        updater::Updater,
        zmq::zmq_listener,
    },
    crate::models::{
        Block, Inscription, InscriptionId, Pagination, PaginationResponse, RuneEntry, TxOutEntry,
    },
    bitcoin::{BlockHash, OutPoint, Txid},
    ordinals::{Rune, RuneId},
    std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread::{self, JoinHandle},
        time::Duration,
    },
    tracing::{error, info},
};

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("store error: {0}")]
    StoreError(#[from] StoreError),
}

type Result<T> = std::result::Result<T, IndexError>;

pub struct Index {
    db: Arc<dyn Store + Send + Sync>,
    settings: Settings,
    updater: Arc<Updater>,
    shutdown_flag: Arc<AtomicBool>,
}

impl Index {
    pub fn new(db: Arc<dyn Store + Send + Sync>, settings: Settings) -> Self {
        let shutdown_flag = Arc::new(AtomicBool::new(false));
        let metrics = Metrics::new();
        metrics.start(shutdown_flag.clone());

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
        }
    }

    pub fn shutdown(&self) {
        self.shutdown_flag
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn initialize_zmq_listener(&self) -> JoinHandle<()> {
        let zmq_endpoint = self.settings.zmq_endpoint.clone();
        let updater = self.updater.clone();
        let shutdown_flag = self.shutdown_flag.clone();

        let zmq_listener_handle = thread::spawn(move || {
            if let Err(e) = zmq_listener(updater, &zmq_endpoint, shutdown_flag) {
                error!("ZMQ listener thread error: {:?}", e);
            }
        });

        zmq_listener_handle
    }

    pub fn index(&self) {
        loop {
            if self.shutdown_flag.load(Ordering::SeqCst) {
                info!("Indexer received shutdown signal, stopping...");
                break;
            }

            if self.updater.is_halted() {
                info!("Updater is halted, stopping indexer loop.");
                break;
            }

            if !self.updater.is_at_tip() {
                self.updater
                    .update_to_tip()
                    .expect("Failed to update to tip");
            }

            if !self.updater.is_at_tip() && !self.shutdown_flag.load(Ordering::SeqCst) {
                self.updater
                    .index_mempool()
                    .expect("Failed to index mempool");
            }

            thread::sleep(Duration::from_millis(self.settings.main_loop_interval));
        }

        info!("Closing indexer");
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
