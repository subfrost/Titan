use bitcoin::ScriptBuf;
use ordinals::RuneId;
use std::convert::TryInto;

use titan_types::SerializedOutPoint;

/// Creates an `OutPoint` from a 36-byte slice.
/// Returns an error if the slice is not exactly 36 bytes long.
pub fn outpoint_from_bytes(bytes: &[u8]) -> Result<SerializedOutPoint, &'static str> {
    if bytes.len() != 36 {
        return Err("Invalid length for OutPoint, expected 36 bytes");
    }

    // Extract Txid (first 32 bytes) and vout (last 4 bytes)
    let vout = u32::from_le_bytes(bytes[32..36].try_into().unwrap());

    Ok(SerializedOutPoint::new(&bytes[0..32], vout))
}

/// Converts an `RuneId` to a 12-byte Vec<u8>.
pub fn rune_id_to_bytes(rune_id: &RuneId) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::with_capacity(12);
    buffer.extend_from_slice(&rune_id.block.to_le_bytes());
    buffer.extend_from_slice(&rune_id.tx.to_le_bytes());
    buffer
}

/// Creates an `RuneId` from a 12-byte slice.
/// Returns an error if the slice is not exactly 12 bytes long.
pub fn rune_id_from_bytes(bytes: &[u8]) -> Result<RuneId, &'static str> {
    if bytes.len() != 12 {
        return Err("Invalid length for RuneId, expected 12 bytes");
    }

    Ok(RuneId {
        block: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
        tx: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
    })
}

pub fn script_pubkey_outpoint_to_bytes(
    script_pubkey: &ScriptBuf,
    outpoint: &SerializedOutPoint,
) -> Vec<u8> {
    let prefix = script_pubkey_search_key(script_pubkey);
    let mut buffer: Vec<u8> = Vec::with_capacity(prefix.len() + 36);
    buffer.extend_from_slice(&prefix);
    buffer.extend_from_slice(outpoint.as_ref());
    buffer
}

pub fn script_pubkey_search_key(script_pubkey: &ScriptBuf) -> Vec<u8> {
    let mut buffer: Vec<u8> = Vec::with_capacity(script_pubkey.len() + 1);
    buffer.extend_from_slice(script_pubkey.as_bytes());
    buffer.push(b':');
    buffer
}

pub fn parse_outpoint_from_script_pubkey_key(
    key: &[u8],
) -> Result<SerializedOutPoint, &'static str> {
    // Get the script_pubkey length from the search key
    let script_pubkey_len = key.len() - 36; // total length minus outpoint length

    // The delimiter should be right after the script pubkey
    if key[script_pubkey_len - 1] != b':' {
        return Err("Invalid key format: missing delimiter");
    }

    // Take exactly 36 bytes from the end
    let outpoint_bytes = &key[script_pubkey_len..];
    outpoint_from_bytes(outpoint_bytes)
}

pub fn rune_index_key(rune_id: &RuneId) -> Vec<u8> {
    let mut v = Vec::with_capacity(rune_id_to_bytes(rune_id).len() + 10);
    v.extend_from_slice(b"rune_index:");
    v.extend_from_slice(&rune_id_to_bytes(rune_id));
    v
}

pub fn rune_transaction_key(rune_id: &RuneId, index: u64) -> Vec<u8> {
    let rune_id_bytes = rune_id_to_bytes(rune_id);
    // Build something like "rune:<id>:\x??\x??...\x??"
    let mut v = Vec::with_capacity(rune_id_bytes.len() + 6 + 8);
    v.extend_from_slice(b"rune:");
    v.extend_from_slice(&rune_id_bytes);
    v.push(b':');
    // Now push the index in little-endian
    v.extend_from_slice(&index.to_le_bytes());
    v
}
