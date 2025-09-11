use crate::message::AlkaneMessageContext;
#[allow(unused_imports)]
use crate::precompiled::{
    alkanes_std_genesis_alkane_dogecoin_build, alkanes_std_genesis_alkane_fractal_build,
    alkanes_std_genesis_alkane_luckycoin_build, alkanes_std_genesis_alkane_mainnet_build,
    alkanes_std_genesis_alkane_regtest_build, alkanes_std_genesis_alkane_upgraded_mainnet_build,
    alkanes_std_genesis_alkane_upgraded_regtest_build, fr_btc_build, fr_sigil_build,
};
use crate::utils::pipe_storagemap_to;
use crate::view::simulate_parcel;
use crate::vm::utils::sequence_pointer;
use alkanes_support::cellpack::Cellpack;
use alkanes_support::gz::compress;
use alkanes_support::id::AlkaneId;
use anyhow::Result;
use bitcoin::{Block, OutPoint, Transaction};
use crate::index_pointer::{AtomicPointer, IndexPointer};
use metashrew_support::index_pointer::KeyValuePointer;
use crate::traits::PersistentRecord;
use protorune_support::message::{MessageContext, MessageContextParcel};
use crate::tables::{RuneTable, RUNES};
use protorune_support::balance_sheet::{BalanceSheet, BalanceSheetOperations};
use protorune_support::utils::{outpoint_encode, tx_hex_to_txid};
use std::sync::Arc;


pub fn fr_btc_bytes() -> Vec<u8> {
    fr_btc_build::get_bytes()
}

pub fn fr_sigil_bytes() -> Vec<u8> {
    fr_sigil_build::get_bytes()
}

#[cfg(feature = "mainnet")]
pub fn genesis_alkane_bytes() -> Vec<u8> {
    alkanes_std_genesis_alkane_mainnet_build::get_bytes()
}

//use if regtest
#[cfg(all(
    not(feature = "mainnet"),
    not(feature = "dogecoin"),
    not(feature = "bellscoin"),
    not(feature = "fractal"),
    not(feature = "luckycoin")
))]
pub fn genesis_alkane_bytes() -> Vec<u8> {
    alkanes_std_genesis_alkane_regtest_build::get_bytes()
}

#[cfg(feature = "dogecoin")]
pub fn genesis_alkane_bytes() -> Vec<u8> {
    alkanes_std_genesis_alkane_dogecoin_build::get_bytes()
}

#[cfg(feature = "bellscoin")]
pub fn genesis_alkane_bytes() -> Vec<u8> {
    alkanes_std_genesis_alkane_dogecoin_build::get_bytes()
}

#[cfg(feature = "fractal")]
pub fn genesis_alkane_bytes() -> Vec<u8> {
    alkanes_std_genesis_alkane_fractal_build::get_bytes()
}

#[cfg(feature = "luckycoin")]
pub fn genesis_alkane_bytes() -> Vec<u8> {
    alkanes_std_genesis_alkane_luckycoin_build::get_bytes()
}

#[cfg(feature = "mainnet")]
pub fn genesis_alkane_upgrade_bytes() -> Vec<u8> {
    alkanes_std_genesis_alkane_upgraded_mainnet_build::get_bytes()
}

//use if regtest
#[cfg(all(
    not(feature = "mainnet"),
    not(feature = "dogecoin"),
    not(feature = "bellscoin"),
    not(feature = "fractal"),
    not(feature = "luckycoin")
))]
pub fn genesis_alkane_upgrade_bytes() -> Vec<u8> {
    alkanes_std_genesis_alkane_upgraded_regtest_build::get_bytes()
}

//use if regtest
#[cfg(all(
    not(feature = "mainnet"),
    not(feature = "dogecoin"),
    not(feature = "bellscoin"),
    not(feature = "fractal"),
    not(feature = "luckycoin")
))]
pub mod genesis {
    pub const GENESIS_BLOCK: u64 = 0;
    pub const GENESIS_OUTPOINT: &str =
        "3977b30a97c9b9d609afb4b7cc138e17b21d1e0c5e360d25debf1441de933bf4";
    pub const GENESIS_OUTPOINT_BLOCK_HEIGHT: u64 = 0;
    pub const GENESIS_UPGRADE_BLOCK_HEIGHT: u32 = 0;
}

#[cfg(feature = "mainnet")]
pub mod genesis {
    pub const GENESIS_BLOCK: u64 = 880_000;
    pub const GENESIS_OUTPOINT: &str =
        "3977b30a97c9b9d609afb4b7cc138e17b21d1e0c5e360d25debf1441de933bf4";
    pub const GENESIS_OUTPOINT_BLOCK_HEIGHT: u64 = 872_101;
    pub const GENESIS_UPGRADE_BLOCK_HEIGHT: u32 = 908_888;
}

#[cfg(feature = "fractal")]
pub mod genesis {
    pub const GENESIS_BLOCK: u64 = 400_000;
    pub const GENESIS_OUTPOINT: &str =
        "cf2b52ffaaf1c094df22f190b888fb0e474fe62990547a34e144ec9f8e135b07";
    pub const GENESIS_OUTPOINT_BLOCK_HEIGHT: u64 = 228_194;
    pub const GENESIS_UPGRADE_BLOCK_HEIGHT: u32 = 759_865;
}

#[cfg(feature = "dogecoin")]
pub mod genesis {
    pub const GENESIS_BLOCK: u64 = 6_000_000;
    pub const GENESIS_OUTPOINT: &str =
        "cf2b52ffaaf1c094df22f190b888fb0e474fe62990547a34e144ec9f8e135b07";
    pub const GENESIS_OUTPOINT_BLOCK_HEIGHT: u64 = 872_101;
    pub const GENESIS_UPGRADE_BLOCK_HEIGHT: u32 = 5_730_675;
}

#[cfg(feature = "luckycoin")]
pub mod genesis {
    pub const GENESIS_BLOCK: u64 = 400_000;
    pub const GENESIS_OUTPOINT: &str =
        "cf2b52ffaaf1c094df22f190b888fb0e474fe62990547a34e144ec9f8e135b07";
    pub const GENESIS_OUTPOINT_BLOCK_HEIGHT: u64 = 872_101;
    pub const GENESIS_UPGRADE_BLOCK_HEIGHT: u32 = 1_664_317;
}

#[cfg(feature = "bellscoin")]
pub mod genesis {
    pub const GENESIS_BLOCK: u64 = 500_000;
    pub const GENESIS_OUTPOINT: &str =
        "2c58484a86e117a445c547d8f3acb56b569f7ea036637d909224d52a5b990259";
    pub const GENESIS_OUTPOINT_BLOCK_HEIGHT: u64 = 288_906;
    pub const GENESIS_UPGRADE_BLOCK_HEIGHT: u32 = 533_970;
}

pub fn is_active(height: u64) -> bool {
    height >= genesis::GENESIS_BLOCK
}

static mut _VIEW: bool = false;

pub fn set_view_mode() {
    unsafe {
        _VIEW = true;
    }
}

pub fn get_view_mode() -> bool {
    unsafe { _VIEW }
}

pub fn is_genesis(height: u64) -> bool {
    let mut init_ptr = IndexPointer::from_keyword("/seen-genesis");
    let has_not_seen_genesis = init_ptr.get().len() == 0;
    let is_genesis = if has_not_seen_genesis {
        get_view_mode() || height >= genesis::GENESIS_BLOCK
    } else {
        false
    };
    if is_genesis {
        init_ptr.set_value::<u8>(0x01);
    }
    is_genesis
}

pub fn genesis(block: &Block) -> Result<()> {
    IndexPointer::from_keyword("/alkanes/")
        .select(&(AlkaneId { block: 2, tx: 0 }).into())
        .set(Arc::new(compress(genesis_alkane_bytes())?));
    IndexPointer::from_keyword("/alkanes/")
        .select(&(AlkaneId { block: 32, tx: 1 }).into())
        .set(Arc::new(compress(fr_sigil_bytes())?));
    IndexPointer::from_keyword("/alkanes/")
        .select(&(AlkaneId { block: 32, tx: 0 }).into())
        .set(Arc::new(compress(fr_btc_bytes())?));
    let mut atomic: AtomicPointer = AtomicPointer::default();
    sequence_pointer(&atomic).set_value::<u128>(1);
    let myself = AlkaneId { block: 2, tx: 0 };
    let fr_btc = AlkaneId { block: 32, tx: 0 };
    let fr_sigil = AlkaneId { block: 32, tx: 1 };
    let parcel = MessageContextParcel {
        store: crate::store::AlkanesProtoruneStore(atomic.derive(&IndexPointer::default())),
        runes: vec![],
        transaction: Transaction {
            version: bitcoin::blockdata::transaction::Version::ONE,
            input: vec![],
            output: vec![],
            lock_time: bitcoin::absolute::LockTime::ZERO,
        },
        block: block.clone(),
        height: genesis::GENESIS_BLOCK,
        pointer: 0,
        refund_pointer: 0,
        calldata: (Cellpack {
            target: myself.clone(),
            inputs: vec![0],
        })
        .encipher(),
        sheets: Box::<BalanceSheet<crate::store::AlkanesProtoruneStore>>::new(BalanceSheet::default()),
        txindex: 0,
        vout: 0,
        runtime_balances: Box::<BalanceSheet<crate::store::AlkanesProtoruneStore>>::new(BalanceSheet::default()),
    };
    let parcel2 = MessageContextParcel {
        store: crate::store::AlkanesProtoruneStore(atomic.derive(&IndexPointer::default())),
        runes: vec![],
        transaction: Transaction {
            version: bitcoin::blockdata::transaction::Version::ONE,
            input: vec![],
            output: vec![],
            lock_time: bitcoin::absolute::LockTime::ZERO,
        },
        block: block.clone(),
        height: genesis::GENESIS_BLOCK,
        pointer: 0,
        refund_pointer: 0,
        calldata: (Cellpack {
            target: fr_btc.clone(),
            inputs: vec![0],
        })
        .encipher(),
        sheets: Box::<BalanceSheet<crate::store::AlkanesProtoruneStore>>::new(BalanceSheet::default()),
        txindex: 0,
        vout: 0,
        runtime_balances: Box::<BalanceSheet<crate::store::AlkanesProtoruneStore>>::new(BalanceSheet::default()),
    };
    let parcel3 = MessageContextParcel {
        store: crate::store::AlkanesProtoruneStore(atomic.derive(&IndexPointer::default())),
        runes: vec![],
        transaction: Transaction {
            version: bitcoin::blockdata::transaction::Version::ONE,
            input: vec![],
            output: vec![],
            lock_time: bitcoin::absolute::LockTime::ZERO,
        },
        block: block.clone(),
        height: genesis::GENESIS_BLOCK,
        pointer: 0,
        refund_pointer: 0,
        calldata: (Cellpack {
            target: fr_sigil.clone(),
            inputs: vec![0, 1],
        })
        .encipher(),
        sheets: Box::<BalanceSheet<crate::store::AlkanesProtoruneStore>>::new(BalanceSheet::default()),
        txindex: 0,
        vout: 0,
        runtime_balances: Box::<BalanceSheet<crate::store::AlkanesProtoruneStore>>::new(BalanceSheet::default()),
    };
    let (response, _gas_used) = (match simulate_parcel(&parcel, u64::MAX) {
        Ok((a, b)) => Ok((a, b)),
        Err(e) => {
            eprintln!("{:?}", e);
            Err(e)
        }
    })?;
    let (response3, _gas_used3) = (match simulate_parcel(&parcel2, u64::MAX) {
        Ok((a, b)) => Ok((a, b)),
        Err(e) => {
            eprintln!("{:?}", e);
            Err(e)
        }
    })?;
    let (response2, _gas_used2) = (match simulate_parcel(&parcel3, u64::MAX) {
        Ok((a, b)) => Ok((a, b)),
        Err(e) => {
            eprintln!("{:?}", e);
            Err(e)
        }
    })?;
    let outpoint_bytes = outpoint_encode(&OutPoint {
        txid: tx_hex_to_txid(genesis::GENESIS_OUTPOINT)?,
        vout: 0,
    })?;
    let mut sheet = BalanceSheet::<crate::store::AlkanesProtoruneStore>::new();
    sheet.init_with_transfers(response.alkanes.into(), genesis::GENESIS_OUTPOINT_BLOCK_HEIGHT)?;
    sheet.save(
        &mut atomic.derive(
            &RuneTable::for_protocol(AlkaneMessageContext::protocol_tag())
                .OUTPOINT_TO_RUNES
                .select(&outpoint_bytes),
        ),
        false,
    );
    let mut sheet2 = BalanceSheet::<crate::store::AlkanesProtoruneStore>::new();
    sheet2.init_with_transfers(response2.alkanes.into(), genesis::GENESIS_OUTPOINT_BLOCK_HEIGHT)?;
    sheet2.save(
        &mut atomic.derive(
            &RuneTable::for_protocol(AlkaneMessageContext::protocol_tag())
                .OUTPOINT_TO_RUNES
                .select(&outpoint_bytes),
        ),
        false,
    );
    pipe_storagemap_to(
        &response.storage,
        &mut atomic.derive(&IndexPointer::from_keyword("/alkanes/").select(&myself.clone().into())),
    );
    pipe_storagemap_to(
        &response3.storage,
        &mut atomic.derive(&IndexPointer::from_keyword("/alkanes/").select(&fr_btc.clone().into())),
    );
    pipe_storagemap_to(
        &response2.storage,
        &mut atomic
            .derive(&IndexPointer::from_keyword("/alkanes/").select(&fr_sigil.clone().into())),
    );
    atomic
        .derive(&RUNES.OUTPOINT_TO_HEIGHT.select(&outpoint_bytes))
        .set_value(genesis::GENESIS_OUTPOINT_BLOCK_HEIGHT);
    atomic
        .derive(
            &RUNES
                .HEIGHT_TO_TRANSACTION_IDS
                .select_value::<u64>(genesis::GENESIS_OUTPOINT_BLOCK_HEIGHT),
        )
        .append(Arc::new(
            hex::decode(genesis::GENESIS_OUTPOINT)?
                .iter()
                .cloned()
                .rev()
                .collect::<Vec<u8>>(),
        ));
    atomic.commit();
    Ok(())
}
