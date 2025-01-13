use {
    crate::models::{InscriptionId, RuneEntry},
    bitcoin::Txid,
    ordinals::{RuneId, SpacedRune},
    serde::{Deserialize, Serialize},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct MintResponse {
    pub start: Option<u64>,
    pub end: Option<u64>,
    pub mintable: bool,
    pub cap: u128,
    pub amount: u128,
    pub mints: u128,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RuneResponse {
    pub id: RuneId,
    pub block: u64,
    pub burned: u128,
    pub divisibility: u8,
    pub etching: Txid,
    pub number: u64,
    pub premine: u128,
    pub supply: u128,
    pub max_supply: u128,
    pub spaced_rune: SpacedRune,
    pub symbol: Option<char>,
    pub mint: Option<MintResponse>,
    pub pending_burns: u128,
    pub pending_mints: u128,
    pub inscription_id: Option<InscriptionId>,
    pub timestamp: u64,
    pub turbo: bool,
}

impl RuneResponse {
    pub fn new(id: RuneId, entry: RuneEntry, height: u64) -> Self {
        let mintable = match entry.mintable(height) {
            Ok(_) => true,
            Err(_) => false,
        };

        let mint: Option<MintResponse> = if mintable {
            Some(MintResponse {
                start: entry.start(),
                end: entry.end(),
                mintable,
                cap: entry.terms.unwrap().cap.unwrap_or_default(),
                amount: entry.terms.unwrap().amount.unwrap_or_default(),
                mints: entry.mints,
            })
        } else {
            None
        };

        Self {
            id,
            block: entry.block,
            burned: entry.burned,
            divisibility: entry.divisibility,
            etching: entry.etching,
            number: entry.number,
            premine: entry.premine,
            supply: entry.supply(),
            max_supply: entry.max_supply(),
            spaced_rune: entry.spaced_rune,
            symbol: entry.symbol,
            mint,
            pending_burns: entry.pending_burns,
            pending_mints: entry.pending_mints,
            inscription_id: entry.inscription_id,
            timestamp: entry.timestamp,
            turbo: entry.turbo,
        }
    }
}
