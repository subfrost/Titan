use crate::network::{genesis::GENESIS_BLOCK, is_active};
use crate::trace::save_trace;
use crate::utils::{credit_balances, debit_balances, pipe_storagemap_to};
use crate::vm::{
    fuel::{FuelTank, VirtualFuelBytes},
    runtime::AlkanesRuntimeContext,
    utils::{prepare_context, run_after_special, run_special_cellpacks},
};
use alkanes_support::{
    cellpack::Cellpack,
    response::ExtendedCallResponse,
    trace::{TraceContext, TraceEvent, TraceResponse},
};
use anyhow::{anyhow, Result};
use bitcoin::OutPoint;
use metashrew_core::index_pointer::IndexPointer;
#[allow(unused_imports)]
use metashrew_core::{
    println,
    stdio::{stdout, Write},
};
use metashrew_support::index_pointer::KeyValuePointer;
use crate::traits::MintableDebit;
use protorune_support::message::{MessageContext, MessageContextParcel};
use protorune_support::balance_sheet::BalanceSheetOperations;
use protorune_support::{
    balance_sheet::BalanceSheet, rune_transfer::RuneTransfer, utils::decode_varint_list,
};
use std::io::Cursor;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct AlkaneMessageContext(());

// TODO: import MessageContextParcel

pub fn handle_message(
    parcel: &MessageContextParcel<crate::store::AlkanesProtoruneStore>,
) -> Result<(Vec<RuneTransfer>, BalanceSheet<crate::store::AlkanesProtoruneStore>)> {
    let cellpack: Cellpack =
        decode_varint_list(&mut Cursor::new(parcel.calldata.clone()))?.try_into()?;

    #[cfg(feature = "debug-log")]
    {
        // Log cellpack information at the beginning of transaction processing
        println!("=== TRANSACTION CELLPACK INFO ===");
        println!(
            "Transaction index: {}, Transaction height: {}, vout: {}, txid: {}",
            parcel.txindex,
            parcel.height,
            parcel.vout,
            parcel.transaction.compute_txid()
        );
        println!(
            "Target contract: [block={}, tx={}]",
            cellpack.target.block, cellpack.target.tx
        );
        println!("Input count: {}", cellpack.inputs.len());
        if !cellpack.inputs.is_empty() {
            println!("First opcode: {}", cellpack.inputs[0]);

            // Print all inputs for detailed debugging
            println!("All inputs: {:?}", cellpack.inputs);
        }
        println!("================================");
    }

    let target = cellpack.target.clone();
    let context = Arc::new(Mutex::new(AlkanesRuntimeContext::from_parcel_and_cellpack(
        parcel, &cellpack,
    )));
    let mut atomic = parcel.store.0.derive(&IndexPointer::default());
    let (caller, myself, binary) = run_special_cellpacks(context.clone(), &cellpack)?;

    #[cfg(feature = "debug-log")]
    {
        // Log the resolved contract addresses
        println!("Caller: [block={}, tx={}]", caller.block, caller.tx);
        println!(
            "Target resolved to: [block={}, tx={}]",
            myself.block, myself.tx
        );
        println!("Parcel runes: {:?}", parcel.runes);
    }

    credit_balances(&mut atomic, &myself, &parcel.runes)?;
    prepare_context(context.clone(), &caller, &myself, false);
    let txsize = parcel.transaction.vfsize() as u64;
    if FuelTank::is_top() {
        FuelTank::fuel_transaction(txsize, parcel.txindex, parcel.height as u32);
    } else if FuelTank::should_advance(parcel.txindex) {
        FuelTank::refuel_block();
        FuelTank::fuel_transaction(txsize, parcel.txindex, parcel.height as u32);
    }
    let fuel = FuelTank::start_fuel();
    // NOTE: we  want to keep unwrap for cases where we lock a mutex guard,
    // it's better if it panics, so then metashrew will retry that block again
    // whereas if we do .map_err(|e| anyhow!("Mutex lock poisoned: {}", e))?
    // it could produce inconsistent indexes if the unlocking fails due to concurrency problem
    // but may pass on retry
    let inner = context.lock().unwrap().flat();
    let trace = context.lock().unwrap().trace.clone();
    trace.clock(TraceEvent::EnterCall(TraceContext {
        inner,
        target,
        fuel,
    }));
    run_after_special(context.clone(), binary, fuel)
        .and_then(|(response, gas_used)| {
            FuelTank::consume_fuel(gas_used)?;
            pipe_storagemap_to(
                &response.storage,
                &mut atomic.derive(
                    &IndexPointer::from_keyword("/alkanes/").select(&myself.clone().into()),
                ),
            );
            let mut combined = parcel.runtime_balances.as_ref().clone();
            let mut runes_sheet = BalanceSheet::new();
            runes_sheet.init_with_transfers(parcel.runes.clone(), parcel.height)?;
            runes_sheet.pipe(&mut combined)?;
            let mut sheet = BalanceSheet::new();
            sheet.init_with_transfers(response.alkanes.clone().into(), parcel.height)?;
            combined.debit_mintable(&sheet, &mut atomic)?;
            debit_balances(&mut atomic, &myself, &response.alkanes)?;
            let cloned = context.clone().lock().unwrap().trace.clone();
            let response_alkanes = response.alkanes.clone();
            cloned.clock(TraceEvent::ReturnContext(TraceResponse {
                inner: response.into(),
                fuel_used: gas_used,
            }));
            save_trace(
                &OutPoint {
                    txid: parcel.transaction.compute_txid(),
                    vout: parcel.vout,
                },
                parcel.height,
                trace.clone(),
            )?;

            Ok((response_alkanes.into(), combined))
        })
        .or_else(|e| {
            #[cfg(feature = "debug-log")]
            {
                // Log detailed error information
                println!("=== TRANSACTION ERROR ===");
                println!("Transaction index: {}", parcel.txindex);
                println!(
                    "Target contract: [block={}, tx={}]",
                    cellpack.target.block, cellpack.target.tx
                );
                println!(
                    "Resolved target: [block={}, tx={}]",
                    myself.block, myself.tx
                );
                println!("Error: {}", e);

                // If it's a fuel-related error, provide more context
                if e.to_string().contains("fuel") || e.to_string().contains("gas") {
                    println!("This appears to be a fuel-related error.");
                    println!(
                        "Contract at [block={}, tx={}] with opcode {} consumed too much fuel.",
                        myself.block,
                        myself.tx,
                        if !cellpack.inputs.is_empty() {
                            cellpack.inputs[0].to_string()
                        } else {
                            "unknown".to_string()
                        }
                    );
                }
                println!("========================");
            }

            FuelTank::drain_fuel();
            let mut response = ExtendedCallResponse::default();

            response.data = vec![0x08, 0xc3, 0x79, 0xa0];
            response.data.extend(e.to_string().as_bytes());
            let cloned = context.clone().lock().unwrap().trace.clone();
            cloned.clock(TraceEvent::RevertContext(TraceResponse {
                inner: response,
                fuel_used: u64::MAX,
            }));
            save_trace(
                &OutPoint {
                    txid: parcel.transaction.compute_txid(),
                    vout: parcel.vout,
                },
                parcel.height,
                cloned,
            )?;
            Err(e)
        })
}

impl MessageContext<crate::store::AlkanesProtoruneStore> for AlkaneMessageContext {
    fn protocol_tag() -> u128 {
        1
    }
    fn handle(
        _parcel: &MessageContextParcel<crate::store::AlkanesProtoruneStore>,
    ) -> Result<(Vec<RuneTransfer>, BalanceSheet<crate::store::AlkanesProtoruneStore>)> {
        if is_active(_parcel.height) {
            match handle_message(_parcel) {
                Ok((outgoing, runtime)) => Ok((outgoing, runtime)),
                Err(e) => {
                    println!("{:?}", e);
                    Err(e) // Print the error
                }
            }
        } else {
            Err(anyhow!(
                "subprotocol inactive until block {}",
                GENESIS_BLOCK
            ))
        }
    }
}
