use bitcoin::hashes::Hash;
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
use std::collections::{BTreeSet, BTreeMap};
use crate::balance_sheet::PersistentRecord;
use bitcoin::Transaction;
use ordinals::{Artifact, Runestone};
use crate::init::index_unique_protorunes;
use crate::index_pointer::IndexPointer;
#[allow(unused_imports)]
use metashrew_support::index_pointer::KeyValuePointer;
use protorune_support::network::{set_network, NetworkParams};
use std::sync::Arc;
use protorune_support::utils::outpoint_encode;
use bitcoin::OutPoint;
use protobuf::{Message, SpecialFields};
use protorune_support::proto;


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

pub fn save_balances<T: MessageContext<AlkanesProtoruneStore>>(
    height: u64,
    atomic: &mut AtomicPointer,
    tx: &Transaction,
    map: &BTreeMap<u32, BalanceSheet<AlkanesProtoruneStore>>,
) -> Result<()> {
    for i in 0..tx.output.len() {
        if tx.output[i].script_pubkey.is_op_return() {
            continue;
        }

        let sheet = map
            .get(&(i as u32))
            .map(|v| v.clone())
            .unwrap_or_else(|| BalanceSheet::default());
        let outpoint = OutPoint {
            txid: tx.compute_txid(),
            vout: i as u32,
        };
        sheet.save(
            &mut atomic.derive(
                &crate::tables::RuneTable::for_protocol(T::protocol_tag())
                    .OUTPOINT_TO_RUNES
                    .select(&outpoint_encode(&outpoint)?),
            ),
            false,
        );
    }
    index_unique_protorunes::<T>(
        atomic,
        height,
        map.iter()
            .try_fold(
                BalanceSheet::default(),
                |mut r, (_k, v)| -> Result<BalanceSheet<AlkanesProtoruneStore>> {
                    v.pipe(&mut r)?;
                    Ok(r)
                },
            )?
            .get_protorunes(),
    );
    Ok(())
}


pub fn index_protorunes<T: MessageContext<AlkanesProtoruneStore>>(
    block: Block,
    height: u64,
) -> Result<BTreeSet<Vec<u8>>> {
    log::debug!("Processing block at height {}", height);
    for (txindex, tx) in block.txdata.iter().enumerate() {
        if let Some(Artifact::Runestone(runestone)) = Runestone::decipher(&tx) {
            log::debug!("Found runestone in tx {}: {:?}", txindex, runestone);
            let mut atomic = AtomicPointer::default();
            // a lot more logic will go here
            atomic.commit();
        }
    }
    Ok(BTreeSet::new())
}

pub fn index_outpoints(block: &Block, height: u64) -> Result<()> {
    let mut atomic = AtomicPointer::default();
    for tx in &block.txdata {
        for i in 0..tx.output.len() {
            let outpoint_bytes = outpoint_encode(
                &(OutPoint {
                    txid: tx.compute_txid(),
                    vout: i as u32,
                }),
            )?;
            atomic
                .derive(&crate::tables::RUNES.OUTPOINT_TO_HEIGHT.select(&outpoint_bytes))
                .set_value(height);
            atomic
                .derive(&crate::tables::OUTPOINT_TO_OUTPUT.select(&outpoint_bytes))
                .set(Arc::new(
                    (proto::protorune::Output {
                        script: tx.output[i].clone().script_pubkey.into_bytes(),
                        value: tx.output[i].clone().value.to_sat(),
                        special_fields: SpecialFields::new(),
                    })
                    .write_to_bytes()?,
                ));
        }
    }
    atomic.commit();
    Ok(())
}

pub fn index_block(block: &Block, height: u32) -> Result<()> {
    log::debug!("Indexing block at height {}", height);
    configure_network();
    clear_diesel_mints_cache();
    index_outpoints(block, height as u64)?;
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
            .append(Arc::new(tx.compute_txid().as_byte_array().to_vec()));
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
