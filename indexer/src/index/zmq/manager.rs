use {
    super::zmq_listener,
    crate::index::updater::Updater,
    std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        },
        thread::JoinHandle,
    },
    tracing::{error, info},
};

pub struct ZmqManager {
    zmq_thread_handle: Mutex<Option<JoinHandle<()>>>,
    zmq_shutdown_flag: Arc<AtomicBool>,

    zmq_endpoint: String,
}

impl ZmqManager {
    pub fn new(zmq_endpoint: String) -> Self {
        Self {
            zmq_thread_handle: Mutex::new(None),
            zmq_shutdown_flag: Arc::new(AtomicBool::new(false)),
            zmq_endpoint,
        }
    }

    pub fn shutdown(&self) {
        self.zmq_shutdown_flag.store(true, Ordering::SeqCst);
    }

    /// Start ZMQ thread and store the join handle so we can manage it later.
    pub fn start_zmq_listener(&self, updater: Arc<Updater>) {
        let shutdown_flag = self.zmq_shutdown_flag.clone();
        let zmq_endpoint = self.zmq_endpoint.clone();
        let handle = std::thread::spawn(move || {
            if let Err(e) = zmq_listener(updater, &zmq_endpoint, shutdown_flag) {
                error!("ZMQ listener thread error: {:?}", e);
            }
        });

        // Store it in our Mutex
        let mut guard = self.zmq_thread_handle.lock().unwrap();
        *guard = Some(handle);
    }

    /// Join the ZMQ listener thread
    pub fn join_zmq_listener(&self) {
        if let Some(handle) = self.zmq_thread_handle.lock().unwrap().take() {
            match handle.join() {
                Ok(_) => info!("ZMQ listener thread has shut down."),
                Err(e) => error!("Failed to join ZMQ listener thread: {:?}", e),
            }
        }
    }
}
