use bitcoin::{hashes::Hash, OutPoint, Txid};
use ordinals::RuneId;
use std::convert::TryInto;

use crate::models::InscriptionId;

/// Converts an `Txid` to a 32-byte Vec<u8>.
pub fn txid_to_bytes(txid: &Txid) -> [u8; 32] {
    txid.as_raw_hash().to_byte_array()
}

/// Converts a 32-byte array to a `Txid`.
pub fn txid_from_bytes(bytes: &[u8]) -> Result<Txid, &'static str> {
    if bytes.len() != 32 {
        return Err("Invalid length for Txid, expected 32 bytes");
    }

    Ok(Txid::from_slice(bytes).unwrap())
}

/// Converts an `OutPoint` to a 36-byte Vec<u8>.
pub fn outpoint_to_bytes(outpoint: &OutPoint) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::with_capacity(36);
    buffer.extend_from_slice(&txid_to_bytes(&outpoint.txid)); // Add 32 bytes of the Txid
    buffer.extend_from_slice(&outpoint.vout.to_le_bytes()); // Add 4 bytes of the vout in little-endian
    buffer
}

/// Creates an `OutPoint` from a 36-byte slice.
/// Returns an error if the slice is not exactly 36 bytes long.
pub fn outpoint_from_bytes(bytes: &[u8]) -> Result<OutPoint, &'static str> {
    if bytes.len() != 36 {
        return Err("Invalid length for OutPoint, expected 36 bytes");
    }

    // Extract Txid (first 32 bytes) and vout (last 4 bytes)
    let txid = txid_from_bytes(&bytes[0..32])?;
    let vout = u32::from_le_bytes(bytes[32..36].try_into().unwrap());

    Ok(OutPoint { txid, vout })
}

/// Converts an `InscriptionId` to a 36-byte Vec<u8>.
pub fn inscription_id_to_bytes(inscription_id: &InscriptionId) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::with_capacity(36);
    buffer.extend_from_slice(&txid_to_bytes(&inscription_id.txid));
    buffer.extend_from_slice(&inscription_id.index.to_le_bytes());
    buffer
}

/// Creates an `InscriptionId` from a 36-byte slice.
/// Returns an error if the slice is not exactly 36 bytes long.
pub fn inscription_id_from_bytes(bytes: &[u8]) -> Result<InscriptionId, &'static str> {
    if bytes.len() != 36 {
        return Err("Invalid length for InscriptionId, expected 36 bytes");
    }

    Ok(InscriptionId {
        txid: txid_from_bytes(&bytes[0..32])?,
        index: u32::from_le_bytes(bytes[32..36].try_into().unwrap()),
    })
}

/// Converts an `RuneId` to a 16-byte Vec<u8>.
pub fn rune_id_to_bytes(rune_id: &RuneId) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::with_capacity(16);
    buffer.extend_from_slice(&rune_id.block.to_le_bytes());
    buffer.extend_from_slice(&rune_id.tx.to_le_bytes());
    buffer
}

/// Creates an `RuneId` from a 16-byte slice.
/// Returns an error if the slice is not exactly 16 bytes long.
pub fn rune_id_from_bytes(bytes: &[u8]) -> Result<RuneId, &'static str> {
    if bytes.len() != 16 {
        return Err("Invalid length for RuneId, expected 16 bytes");
    }

    Ok(RuneId {
        block: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
        tx: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    })
}
