use crate::index_pointer::AtomicPointer;
use crate::message::AlkaneMessageContext;
use crate::network::{genesis, genesis_alkane_upgrade_bytes, is_genesis};
use crate::tables::RUNES;
use log;
use crate::vm::fuel::FuelTank;
use crate::vm::host_functions::clear_diesel_mints_cache;
use alkanes_support::gz::compress;
use alkanes_support::id::AlkaneId;
use anyhow::Result;
use bitcoin::blockdata::block::Block;
use crate::store::AlkanesProtoruneStore;
use protorune_support::protostone::Protostone;
use protorune_support::rune_transfer::RuneTransfer;
use protorune_support::balance_sheet::{BalanceSheet, BalanceSheetOperations};
use protorune_support::message::{MessageContext, MessageContextParcel};
use std::collections::BTreeSet;
use crate::init::index_unique_protorunes;
use crate::index_pointer::IndexPointer;
#[allow(unused_imports)]
use metashrew_support::index_pointer::KeyValuePointer;
use protorune_support::network::{set_network, NetworkParams};
use std::sync::Arc;

#[cfg(all(
    not(feature = "mainnet"),
    not(feature = "testnet"),
    not(feature = "luckycoin"),
    not(feature = "dogecoin"),
    not(feature = "bellscoin")
))]
pub fn configure_network() {
    set_network(NetworkParams {
        bech32_prefix: String::from("bcrt"),
        p2pkh_prefix: 0x64,
        p2sh_prefix: 0xc4,
    });
}
#[cfg(feature = "mainnet")]
pub fn configure_network() {
    set_network(NetworkParams {
        bech32_prefix: String::from("bc"),
        p2sh_prefix: 0x05,
        p2pkh_prefix: 0x00,
    });
}
#[cfg(feature = "testnet")]
pub fn configure_network() {
    set_network(NetworkParams {
        bech32_prefix: String::from("tb"),
        p2pkh_prefix: 0x6f,
        p2sh_prefix: 0xc4,
    });
}
#[cfg(feature = "luckycoin")]
pub fn configure_network() {
    set_network(NetworkParams {
        bech32_prefix: String::from("lky"),
        p2pkh_prefix: 0x2f,
        p2sh_prefix: 0x05,
    });
}

#[cfg(feature = "dogecoin")]
pub fn configure_network() {
    set_network(NetworkParams {
        bech32_prefix: String::from("dc"),
        p2pkh_prefix: 0x1e,
        p2sh_prefix: 0x16,
    });
}
#[cfg(feature = "bellscoin")]
pub fn configure_network() {
    set_network(NetworkParams {
        bech32_prefix: String::from("bel"),
        p2pkh_hash: 0x19,
        p2sh_hash: 0x1e,
    });
}

#[cfg(feature = "cache")]
use crate::view::protorunes_by_address;
#[cfg(feature = "cache")]
use protobuf::{Message, MessageField};
#[cfg(feature = "cache")]
use protorune_support::tables::{CACHED_FILTERED_WALLET_RESPONSE, CACHED_WALLET_RESPONSE};
#[cfg(feature = "cache")]
use protorune_support::proto::protorune::ProtorunesWalletRequest;
#[cfg(feature = "cache")]
use std::sync::Arc;

pub fn index_protorunes<T: MessageContext<AlkanesProtoruneStore>>(
    block: Block,
    height: u64,
) -> Result<BTreeSet<Vec<u8>>> {
    let mut updated_addresses: BTreeSet<Vec<u8>> = BTreeSet::new();
    log::debug!("Processing block at height {}", height);
    for (txindex, tx) in block.txdata.iter().enumerate() {
        log::debug!("Scanning transaction {}", txindex);
        let protostones = Protostone::scan(&tx);
        for protostone in protostones.into_iter() {
            if protostone.protocol_tag != T::protocol_tag() {
                continue;
            }
            log::debug!("Found protostone: {:?}", protostone);
            let mut sheets = BalanceSheet::<AlkanesProtoruneStore>::new();
            sheets.init_with_protostone(&protostone, height)?;
            let parcel = MessageContextParcel {
                store: AlkanesProtoruneStore::default(),
                runes: protostone.edicts.into_iter().map(|v| v.into()).collect(),
                transaction: tx.clone(),
                block: block.clone(),
                height,
                pointer: protostone.pointer.unwrap_or_default(),
                refund_pointer: protostone.refund.unwrap_or_default(),
                calldata: protostone.message,
                sheets: Box::new(sheets),
                txindex: txindex as u32,
                vout: 0,
                runtime_balances: Box::new(BalanceSheet::default()),
            };
            let (mut transfers, balances) = T::handle(&parcel)?;
            let mut atomic = parcel.store.0.derive(&IndexPointer::default());
            balances.apply()?;
            index_unique_protorunes::<T>(&mut atomic, height, balances.get_protorunes());
            transfers
                .iter_mut()
                .for_each(|v: &mut RuneTransfer| v.output = tx.output.len() as u32);
            let mut out_sheets = BalanceSheet::<AlkanesProtoruneStore>::new();
            out_sheets.init_with_transfers(transfers, height)?;
            out_sheets.apply()?;
            out_sheets
                .get_addresses()
                .into_iter()
                .for_each(|v| {
                    updated_addresses.insert(v);
                });
            balances.get_addresses().into_iter().for_each(|v| {
                updated_addresses.insert(v);
            });
        }
    }
    Ok(updated_addresses)
}

pub fn index_block(block: &Block, height: u32) -> Result<()> {
    log::debug!("Indexing block at height {}", height);
    configure_network();
    clear_diesel_mints_cache();
    let really_is_genesis = is_genesis(height.into());
    if really_is_genesis {
        log::debug!("Block is genesis block, running genesis indexing");
        genesis(&block).unwrap();
    }
    if height >= genesis::GENESIS_UPGRADE_BLOCK_HEIGHT {
        let mut upgrade_ptr = IndexPointer::from_keyword("/genesis-upgraded");
        if upgrade_ptr.get().len() == 0 {
            upgrade_ptr.set_value::<u8>(0x01);
            IndexPointer::from_keyword("/alkanes/")
                .select(&(AlkaneId { block: 2, tx: 0 }).into())
                .set(Arc::new(compress(genesis_alkane_upgrade_bytes())?));
        }
    }
    FuelTank::initialize(&block, height);
    let mut atomic = AtomicPointer::default();
    for tx in block.txdata.iter() {
        atomic
            .derive(&RUNES.HEIGHT_TO_TRANSACTION_IDS.select_value(height as u64))
            .append(Arc::new(tx.txid().as_byte_array().to_vec()));
    }
    atomic.commit();

    // Get the set of updated addresses from the indexing process
    let _updated_addresses =
        index_protorunes::<AlkaneMessageContext>(block.clone(), height.into())?;

    #[cfg(feature = "cache")]
    {
        // Cache the WalletResponse for each updated address
        for address in _updated_addresses {
            // Skip empty addresses
            if address.is_empty() {
                continue;
            }

            // Create a request for this address
            let mut request = ProtorunesWalletRequest::new();
            request.wallet = address.clone();
            request.protocol_tag = Some(<u128 as Into<
                protorune_support::proto::protorune::Uint128,
            >>::into(AlkaneMessageContext::protocol_tag()))
            .into();

            // Get the WalletResponse for this address (full set of spendable outputs)
            match protorunes_by_address(&request.write_to_bytes()?) {
                Ok(full_response) => {
                    // Cache the serialized full WalletResponse
                    CACHED_WALLET_RESPONSE
                        .select(&address)
                        .set(Arc::new(full_response.write_to_bytes()?));

                    // Create a filtered version with only outpoints that have runes
                    let mut filtered_response = full_response.clone();
                    filtered_response.outpoints = filtered_response
                        .outpoints
                        .into_iter()
                        .filter_map(|v| {
                            if v.balances()
                                .unwrap_or_else(|| {
                                    protorune_support::proto::protorune::BalanceSheet::new()
                                })
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

                    // Cache the serialized filtered WalletResponse
                    CACHED_FILTERED_WALLET_RESPONSE
                        .select(&address)
                        .set(Arc::new(filtered_response.write_to_bytes()?));
                }
                Err(e) => {
                    println!("Error caching wallet response for address: {:?}", e);
                }
            }
        }
    }

    Ok(())
}
