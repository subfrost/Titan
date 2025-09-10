use crate::indexer::configure_network;
use crate::view::{meta_safe, multi_simulate_safe, parcel_from_protobuf, simulate_safe};
use alkanes_support::proto;
use bitcoin::OutPoint;
#[allow(unused_imports)]
use metashrew_core::{
    flush, input, println,
    stdio::{stdout, Write},
};
#[allow(unused_imports)]
use metashrew_support::block::AuxpowBlock;
use metashrew_support::compat::export_bytes;
#[allow(unused_imports)]
use metashrew_support::index_pointer::KeyValuePointer;
use metashrew_support::utils::{consume_sized_int, consume_to_end};
use protobuf::{Message, MessageField};
use std::io::Cursor;
use view::parcels_from_protobuf;
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

#[cfg(not(test))]
#[no_mangle]
pub fn multisimluate() -> i32 {
    configure_network();
    let data = input();
    let _height = u32::from_le_bytes((&data[0..4]).try_into().unwrap());
    let reader = &data[4..];
    let mut result: proto::alkanes::MultiSimulateResponse =
        proto::alkanes::MultiSimulateResponse::new();
    let responses = multi_simulate_safe(
        &parcels_from_protobuf(
            proto::alkanes::MultiSimulateRequest::parse_from_bytes(reader).unwrap(),
        ),
        u64::MAX,
    );

    for response in responses {
        let mut res = proto::alkanes::SimulateResponse::new();
        match response {
            Ok((response, gas_used)) => {
                res.execution = MessageField::some(response.into());
                res.gas_used = gas_used;
            }
            Err(e) => {
                result.error = e.to_string();
            }
        }
        result.responses.push(res);
    }

    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn simulate() -> i32 {
    configure_network();
    let data = input();
    let _height = u32::from_le_bytes((&data[0..4]).try_into().unwrap());
    let reader = &data[4..];
    let mut result: proto::alkanes::SimulateResponse = proto::alkanes::SimulateResponse::new();
    match simulate_safe(
        &parcel_from_protobuf(
            proto::alkanes::MessageContextParcel::parse_from_bytes(reader).unwrap(),
        ),
        u64::MAX,
    ) {
        Ok((response, gas_used)) => {
            result.execution = MessageField::some(response.into());
            result.gas_used = gas_used;
        }
        Err(e) => {
            result.error = e.to_string();
        }
    }
    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn sequence() -> i32 {
    export_bytes(view::sequence().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn meta() -> i32 {
    configure_network();
    let data = input();
    let _height = u32::from_le_bytes((&data[0..4]).try_into().unwrap());
    let reader = &data[4..];
    match meta_safe(&parcel_from_protobuf(
        proto::alkanes::MessageContextParcel::parse_from_bytes(reader).unwrap(),
    )) {
        Ok(response) => export_bytes(response),
        Err(_) => export_bytes(vec![]),
    }
}

#[cfg(not(test))]
#[no_mangle]
pub fn runesbyaddress() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    let result: protorune_support::proto::protorune::WalletResponse =
        crate::view::protorunes_by_address(&consume_to_end(&mut data).unwrap())
            .unwrap_or_else(|_| protorune_support::proto::protorune::WalletResponse::new());
    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn unwrap() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let height = consume_sized_int::<u32>(&mut data).unwrap();
    export_bytes(view::unwrap(height.into()).unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn runesbyoutpoint() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    let result: protorune_support::proto::protorune::OutpointResponse =
        crate::view::protorunes_by_outpoint(&consume_to_end(&mut data).unwrap())
            .unwrap_or_else(|_| protorune_support::proto::protorune::OutpointResponse::new());
    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn spendablesbyaddress() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    let result: protorune_support::proto::protorune::WalletResponse =
        view::protorunes_by_address(&consume_to_end(&mut data).unwrap())
            .unwrap_or_else(|_| protorune_support::proto::protorune::WalletResponse::new());
    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn protorunesbyaddress() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    let input_data = consume_to_end(&mut data).unwrap();
    //  let _request = protorune_support::proto::protorune::ProtorunesWalletRequest::parse_from_bytes(&input_data).unwrap();

    let mut result: protorune_support::proto::protorune::WalletResponse =
        view::protorunes_by_address(&input_data)
            .unwrap_or_else(|_| protorune_support::proto::protorune::WalletResponse::new());

    result.outpoints = result
        .outpoints
        .into_iter()
        .filter_map(|v| {
            if v.clone()
                .balances
                .unwrap_or_else(|| protorune_support::proto::protorune::BalanceSheet::new())
                .entries
                .len()
                == 0
            {
                None
            } else {
                Some(v)
            }
        })
        .collect::<Vec<protorune_support::proto::protorune::OutpointResponse>>();

    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn getblock() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    let input_data = consume_to_end(&mut data).unwrap();
    export_bytes(view::getblock(&input_data).unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn protorunesbyheight() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    let result: protorune_support::proto::protorune::RunesResponse =
        view::protorunes_by_height(&consume_to_end(&mut data).unwrap())
            .unwrap_or_else(|_| protorune_support::proto::protorune::RunesResponse::new());
    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn alkanes_id_to_outpoint() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    // first 4 bytes come in as height, not used
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    let data_vec = consume_to_end(&mut data).unwrap();
    let result: alkanes_support::proto::alkanes::AlkaneIdToOutpointResponse =
        view::alkanes_id_to_outpoint(&data_vec).unwrap_or_else(|err| {
            eprintln!("Error in alkanes_id_to_outpoint: {:?}", err);
            alkanes_support::proto::alkanes::AlkaneIdToOutpointResponse::new()
        });
    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn traceblock() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let height = consume_sized_int::<u32>(&mut data).unwrap();
    export_bytes(view::traceblock(height).unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn trace() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    let outpoint: OutPoint = protorune_support::proto::protorune::Outpoint::parse_from_bytes(
        &consume_to_end(&mut data).unwrap(),
    )
    .unwrap()
    .try_into()
    .unwrap();
    export_bytes(view::trace(&outpoint).unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn getbytecode() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    export_bytes(view::getbytecode(&consume_to_end(&mut data).unwrap()).unwrap_or_default())
}

#[cfg(not(test))]
#[no_mangle]
pub fn protorunesbyoutpoint() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    let result: protorune_support::proto::protorune::OutpointResponse =
        view::protorunes_by_outpoint(&consume_to_end(&mut data).unwrap())
            .unwrap_or_else(|_| protorune_support::proto::protorune::OutpointResponse::new());

    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn runesbyheight() -> i32 {
    configure_network();
    let mut data: Cursor<Vec<u8>> = Cursor::new(input());
    let _height = consume_sized_int::<u32>(&mut data).unwrap();
    let result: protorune_support::proto::protorune::RunesResponse =
        crate::view::protorunes_by_height(&consume_to_end(&mut data).unwrap())
            .unwrap_or_else(|_| protorune_support::proto::protorune::RunesResponse::new());
    export_bytes(result.write_to_bytes().unwrap())
}

// TODO: this function needs to improve the way it stores all alkane ids, it doesn't handle duplicates right now
#[cfg(not(test))]
#[no_mangle]
pub fn getinventory() -> i32 {
    let data = input();
    let _height = u32::from_le_bytes((&data[0..4]).try_into().unwrap());
    let reader = &data[4..];
    let result = view::getinventory(
        &proto::alkanes::AlkaneInventoryRequest::parse_from_bytes(reader)
            .unwrap()
            .into(),
    )
    .unwrap();
    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(not(test))]
#[no_mangle]
pub fn getstorageat() -> i32 {
    let data = input();
    let _height = u32::from_le_bytes((&data[0..4]).try_into().unwrap());
    let reader = &data[4..];
    let result = view::getstorageat(
        &proto::alkanes::AlkaneStorageRequest::parse_from_bytes(reader)
            .unwrap()
            .into(),
    )
    .unwrap();
    export_bytes(result.write_to_bytes().unwrap())
}

#[cfg(all(target_arch = "wasm32", not(test)))]
#[no_mangle]
pub fn _start() {
    let data = input();
    let height = u32::from_le_bytes((&data[0..4]).try_into().unwrap());
    let reader = &data[4..];
    #[cfg(any(feature = "dogecoin", feature = "luckycoin", feature = "bellscoin"))]
    let block: Block = AuxpowBlock::parse(&mut Cursor::<Vec<u8>>::new(reader.to_vec()))
        .unwrap()
        .to_consensus();
    #[cfg(not(any(feature = "dogecoin", feature = "luckycoin", feature = "bellscoin")))]
    let block: Block =
        consensus_decode::<Block>(&mut Cursor::<Vec<u8>>::new(reader.to_vec())).unwrap();

    index_block(&block, height).unwrap();
    etl::index_extensions(height, &block);
    flush();
}

#[cfg(test)]
mod unit_tests {
    use super::*;
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

        let data = block_data;
        let height = u32::from_le_bytes((&data[0..4]).try_into().unwrap());
        let reader = &data[4..];
        let block: Block =
            consensus_decode::<Block>(&mut Cursor::<Vec<u8>>::new(reader.to_vec())).unwrap();
        assert!(height == 849236);

        // calling index_block directly fails since genesis(&block).unwrap(); gets segfault
        // index_block(&block, height).unwrap();
        configure_network();
        index_protorunes::<AlkaneMessageContext>(block.clone(), height.into()).unwrap();

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
        std::println!("RUNES by addr: {:?}", runes_for_addr);

        let outpoint_res = protorunes_by_outpoint(
            &(OutPoint {
                txid: block.txdata[298].compute_txid(),
                vout: 2,
            }),
        )
        .unwrap();
        let quorum_rune = outpoint_res.balances.unwrap().entries[0].clone();
        let balance = quorum_rune.balance.0.unwrap();
        let mut expected_balance = Uint128::new();
        expected_balance.lo = 21000000;
        assert!(*balance == expected_balance);
        // TODO: Assert rune
        std::println!(" with rune {:?}", quorum_rune.rune.0);

        // assert!(false);
    }
}

