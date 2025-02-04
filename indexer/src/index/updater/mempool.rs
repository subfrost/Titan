use bitcoin::{Transaction, Txid};
use bitcoincore_rpc::json::GetMempoolEntryResult;
use bitcoincore_rpc::{Client, RpcApi};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, error};

#[derive(thiserror::Error, Debug)]
pub enum MempoolError {
    #[error("rpc error: {0}")]
    RpcError(#[from] bitcoincore_rpc::Error),
    #[error("cycle detected in transaction dependencies")]
    CycleDetected,
}

pub fn fetch_transactions(
    client: &Client,
    txids: &Vec<Txid>,
    interrupt: Arc<AtomicBool>,
) -> HashMap<Txid, Transaction> {
    txids
        .par_iter()
        .filter_map(|txid| {
            if interrupt.load(Ordering::SeqCst) {
                return None;
            }

            match client.get_raw_transaction(txid, None) {
                Ok(tx) => Some((*txid, tx)),
                Err(e) => {
                    error!("Failed to fetch transaction {}: {}", txid, e);
                    None
                }
            }
        })
        .collect()
}

pub fn sort_transaction_order(
    client: &Client,
    tx_map: &HashMap<Txid, Transaction>,
    interrupt: Arc<AtomicBool>,
) -> Result<Vec<Txid>, MempoolError> {
    // Step 1: Pre-fetch mempool entries in parallel for dependency resolution
    let mempool_entry_txids: HashSet<Txid> = tx_map
        .values()
        .flat_map(|tx| tx.input.iter().map(|input| input.previous_output.txid))
        .collect();

    debug!("Fetching {} mempool entries", mempool_entry_txids.len());

    let mempool_entries: HashMap<Txid, GetMempoolEntryResult> = mempool_entry_txids
        .par_iter()
        .filter_map(|txid| {
            if interrupt.load(Ordering::SeqCst) {
                return None;
            }
            client
                .get_mempool_entry(txid)
                .ok()
                .map(|entry| (*txid, entry))
        })
        .collect();

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
