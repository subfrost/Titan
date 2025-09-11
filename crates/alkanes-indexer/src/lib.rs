pub mod index_pointer;

#[allow(unused_imports)]
use metashrew_support::block::AuxpowBlock;
#[allow(unused_imports)]
use metashrew_support::index_pointer::KeyValuePointer;
pub mod block;
pub mod store;
pub mod etl;
pub mod traits;
pub mod init;
pub mod tables;
pub mod indexer;
pub mod message;
pub mod network;
pub mod precompiled;
#[cfg(any(test, feature = "test-utils", feature = "debug-log"))]
pub mod trace;
pub mod unwrap;
pub mod utils;
pub mod view;
pub mod vm;

/*
All the #[no_mangle] configs will fail during github action cargo test step
due to duplicate symbol:
  rust-lld: error: duplicate symbol: runesbyheight
  >>> defined in /home/runner/work/alkanes-rs/alkanes-rs/target/wasm32-unknown-unknown/debug/deps/alkanes-5b647d16704125c9.alkanes.7a19fa39330b2460-cgu.05.rcgu.o
  >>> defined in /home/runner/work/alkanes-rs/alkanes-rs/target/wasm32-unknown-unknown/debug/deps/libalkanes.rlib(alkanes.alkanes.2dae95da706e3a8c-cgu.09.rcgu.o)

This is because both
[lib]
crate-type = ["cdylib", "rlib"]

are defined in Cargo.toml since we want to build both the wasm and rust library.

Running cargo test will compile an additional test harness binary that:
Links libalkanes.rlib
Compiles #[no_mangle] functions again into the test binary
Then links everything together, leading to duplicate symbols

Thus, going to add not(test) to all these functions
*/


#[cfg(test)]
mod unit_tests {
    use crate::indexer::configure_network;
    use bitcoin::OutPoint;
    use crate::message::AlkaneMessageContext;
    use protobuf::{Message, SpecialFields};
    use crate::view::{protorunes_by_outpoint, protorunes_by_address, protorunes_by_height};
    use crate::indexer::index_protorunes;
    use protorune_support::proto::protorune::{RunesByHeightRequest, Uint128, WalletRequest};
    use std::fs;
    use std::path::PathBuf;
    use bitcoin::Block;
    use metashrew_support::utils::consensus_decode;
    use std::io::Cursor;

    #[test]
    pub fn test_decode_block() {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("src/tests/static/849236.txt");
        let block_data = fs::read(&path).unwrap();

        assert!(block_data.len() > 0);

        let height = 849236;
        let block: Block =
            consensus_decode::<Block>(&mut Cursor::new(block_data)).unwrap();

        // calling index_block directly fails since genesis(&block).unwrap(); gets segfault
        // index_block(&block, height).unwrap();
        configure_network();
        index_protorunes::<AlkaneMessageContext>(block.clone(), height as u64).unwrap();

        let req_height: Vec<u8> = (RunesByHeightRequest {
            height: 849236,
            special_fields: SpecialFields::new(),
        })
        .write_to_bytes()
        .unwrap();
        let runes = protorunes_by_height(&req_height).unwrap();
        assert!(runes.runes.len() == 2);

        // TODO: figure out what address to use for runesbyaddress
        let req_wallet: Vec<u8> = (WalletRequest {
            wallet: String::from("bc1pfs5dhzwk32xa53cjx8fx4dqy7hm4m6tys8zyvemqffz8ua4tytqs8vjdgr")
                .as_bytes()
                .to_vec(),
            special_fields: SpecialFields::new(),
        })
        .write_to_bytes()
        .unwrap();

        let runes_for_addr = protorunes_by_address(&req_wallet).unwrap();
        // assert!(runes_for_addr.balances > 0);
        eprintln!("RUNES by addr: {:?}", runes_for_addr);

        let outpoint_res = protorunes_by_outpoint(
            &protorune_support::utils::outpoint_encode(&OutPoint {
                txid: block.txdata[298].compute_txid(),
                vout: 2,
            })
            .unwrap(),
        )
        .unwrap();
        let quorum_rune = outpoint_res.balances.unwrap().entries[0].clone();
        let balance = quorum_rune.balance.0.unwrap();
        let mut expected_balance = Uint128::new();
        expected_balance.lo = 21000000;
        assert!(*balance == expected_balance);
        // TODO: Assert rune
        eprintln!(" with rune {:?}", quorum_rune.rune.0);

        // assert!(false);
    }
}
