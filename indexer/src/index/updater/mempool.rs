use bitcoin::{Transaction, Txid};
use bitcoincore_rpc::json::GetMempoolEntryResult;
use bitcoincore_rpc::RpcApi;
use rayon::prelude::*;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::error;

use crate::index::{RpcClientError, RpcClientProvider};

#[derive(thiserror::Error, Debug)]
pub enum MempoolError {
    #[error("rpc error: {0}")]
    RpcError(#[from] bitcoincore_rpc::Error),
    #[error("rpc client error: {0}")]
    RpcClientError(#[from] RpcClientError),
    #[error("cycle detected in transaction dependencies")]
    CycleDetected,
}

pub fn fetch_transactions(
    client_provider: Arc<dyn RpcClientProvider + Send + Sync>,
    txids: &Vec<Txid>,
    interrupt: Arc<AtomicBool>,
) -> HashMap<Txid, Transaction> {
    txids
        .par_iter()
        .filter_map(|txid| {
            if interrupt.load(Ordering::SeqCst) {
                return None;
            }

            let client = client_provider.get_new_rpc_client();

            if let Ok(client) = client {
                match client.get_raw_transaction(txid, None) {
                    Ok(tx) => Some((*txid, tx)),
                    Err(e) => {
                        error!("Failed to fetch transaction {}: {}", txid, e);
                        None
                    }
                }
            } else {
                error!("Failed to get new rpc client");
                None
            }
        })
        .collect()
}

pub fn sort_transaction_order(
    mempool_entries: &HashMap<Txid, GetMempoolEntryResult>,
    tx_map: &HashMap<Txid, Transaction>,
) -> Result<Vec<Txid>, MempoolError> {
    // Step 2: Build dependency graph and indegree count
    let mut graph: HashMap<Txid, Vec<Txid>> = HashMap::new();
    let mut indegree: HashMap<Txid, usize> = HashMap::new();

    // Initialize graph nodes
    for &txid in tx_map.keys() {
        graph.insert(txid, Vec::new());
        indegree.insert(txid, 0);
    }

    // Populate graph with edges and compute indegrees
    for (&txid, tx) in tx_map.iter() {
        for input in &tx.input {
            let dep_txid = input.previous_output.txid;
            // Only consider dependencies that are part of our tx_map and in mempool_entries
            if tx_map.contains_key(&dep_txid) && mempool_entries.contains_key(&dep_txid) {
                graph.entry(dep_txid).or_default().push(txid);
                *indegree.entry(txid).or_insert(0) += 1;
            }
        }
    }

    // Step 3: Topological sort using Kahn's algorithm
    let mut queue: VecDeque<Txid> = indegree
        .iter()
        .filter_map(|(&txid, &deg)| if deg == 0 { Some(txid) } else { None })
        .collect();

    let mut sorted = Vec::with_capacity(tx_map.len());

    while let Some(node) = queue.pop_front() {
        sorted.push(node);

        if let Some(neighbors) = graph.get(&node) {
            for &neighbor in neighbors {
                if let Some(count) = indegree.get_mut(&neighbor) {
                    *count -= 1;
                    if *count == 0 {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    // If sorted transactions count doesn't match input transactions count, a cycle exists.
    if sorted.len() != tx_map.len() {
        return Err(MempoolError::CycleDetected);
    }

    Ok(sorted)
}
