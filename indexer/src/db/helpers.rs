use ordinals::RuneId;

use super::util::rune_id_to_bytes;

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
