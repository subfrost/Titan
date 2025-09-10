#[derive(Debug, PartialEq, Clone, BorshSerialize, BorshDeserialize)]
pub struct PendingEtch {
    pub divisibility: u8,
    pub premine: u128,
    pub spaced_rune: SpacedRune,
    pub symbol: Option<char>,
    pub terms: Option<Terms>,
    pub inscription_id: Option<InscriptionId>,
    pub timestamp: u64,
    pub turbo: bool,
}
