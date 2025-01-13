use std::sync::{atomic::AtomicBool, Arc};

use bitcoin::{consensus::encode, Block, BlockHash, Transaction};
use tracing::{debug, error, info};
use zmq::{Context, Error as ZmqError};

use super::updater::Updater;

pub fn zmq_listener(
    updater: Arc<Updater>,
    endpoint: &str,
    shutdown_flag: Arc<AtomicBool>,
) -> Result<(), ZmqError> {
    // 1. Create ZMQ context and subscriber socket
    let context = Context::new();
    let subscriber = context.socket(zmq::SUB)?;

    // 2. Connect to the configured endpoint
    info!("Connecting to ZMQ at: {endpoint}");
    subscriber.connect(endpoint)?;

    // 3. Subscribe to raw blocks and raw transactions
    subscriber.set_subscribe(b"rawblock")?;
    subscriber.set_subscribe(b"rawtx")?;
    debug!("Subscribed to topics: rawblock, rawtx");

    // 4. Main loop: block on incoming messages
    loop {
        if shutdown_flag.load(std::sync::atomic::Ordering::SeqCst) {
            info!("[ZMQ] Shutdown flag set, exiting ZMQ listener...");
            break;
        }

        // First frame = topic
        let topic_frame = subscriber.recv_msg(0)?;
        let topic = match topic_frame.as_str() {
            Some(s) => s,
            None => continue,
        };

        // Second frame = payload
        let payload_frame = subscriber.recv_msg(0)?;
        let payload = payload_frame.as_ref();

        match topic {
            "rawblock" => {
                debug!("[ZMQ] Received raw block, size={} bytes", payload.len());
                if let Err(e) = handle_raw_block(&updater, payload) {
                    error!("Failed to handle raw block: {:?}", e);
                }
            }
            "rawtx" => {
                debug!(
                    "[ZMQ] Received raw transaction, size={} bytes",
                    payload.len()
                );
                if let Err(e) = handle_raw_tx(&updater, payload) {
                    error!("Failed to handle raw transaction: {:?}", e);
                }
            }
            _ => {
                error!("[ZMQ] Unknown topic: {}", topic);
            }
        }

        // If there's a sequence frame (bitcoind may publish that if configured), consume it
        while subscriber.get_rcvmore()? {
            let _extra_frame = subscriber.recv_msg(0)?;
            // Usually, this extra frame is just a sequence number you can log or ignore.
        }
    }

    info!("[ZMQ] Listener closed.");
    Ok(())
}

/// Decode and store a raw block in RocksDB
fn handle_raw_block(
    updater: &Arc<Updater>,
    block_bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse the block using the `bitcoin` crate
    let block: Block = encode::deserialize(block_bytes)?;
    let block_hash: BlockHash = block.block_hash();

    if !updater.is_halted() && updater.is_at_tip() {
        updater.index_new_block(block, None)?;
        debug!("[ZMQ] Indexed block {}", block_hash);
    }

    Ok(())
}

/// Decode and store a raw transaction in RocksDB
fn handle_raw_tx(
    updater: &Arc<Updater>,
    tx_bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    // Parse the transaction
    let tx = encode::deserialize::<Transaction>(tx_bytes)?;
    let txid = tx.compute_txid();

    if !updater.is_halted() && updater.is_at_tip() {
        updater.index_new_tx(&txid, &tx)?;
        debug!("[ZMQ] Indexed tx {}", txid);
    }

    Ok(())
}
