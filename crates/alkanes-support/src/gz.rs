use anyhow::Result;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::prelude::*;

pub fn decompress(binary: Vec<u8>) -> Result<Vec<u8>> {
    let mut result = Vec::<u8>::new();
    let mut reader = GzDecoder::new(&binary[..]);
    reader.read_to_end(&mut result)?;
    Ok(result)
}

pub fn compress(binary: Vec<u8>) -> Result<Vec<u8>> {
    let mut writer = GzEncoder::new(Vec::<u8>::with_capacity(binary.len()), Compression::best());
    writer.write_all(&binary)?;
    Ok(writer.finish()?)
}
