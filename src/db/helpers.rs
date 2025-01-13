
pub fn rune_index_key(rune_id: &str) -> String {
    format!("rune_index:{}", rune_id)
}

pub fn rune_transaction_key(rune_id: &str, index: u64) -> Vec<u8> {
    // Build something like "rune:<id>:\x??\x??...\x??"
    let mut v = Vec::with_capacity(rune_id.len() + 6 + 8);
    v.extend_from_slice(b"rune:");
    v.extend_from_slice(rune_id.as_bytes());
    v.push(b':');
    // Now push the index in little-endian
    v.extend_from_slice(&index.to_le_bytes());
    v
}
