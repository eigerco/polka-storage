/// Byte representation of the entity that was signing the proof.
/// It must match the ProverId used for Proving.
pub type ProverId = [u8; 32];
/// Byte representation of a commitment - CommR or CommD.
pub type Commitment = [u8; 32];
/// Byte representation of randomness seed, it's used for challenge generation.
pub type Ticket = [u8; 32];

#[derive(Clone, Debug)]
pub struct PieceInfo {
    pub size: u64,
    pub commitment: Commitment,
}

#[cfg(feature = "std")]
impl From<filecoin_proofs::PieceInfo> for PieceInfo {
    fn from(p: filecoin_proofs::PieceInfo) -> Self {
        Self {
            size: p.size.into(),
            commitment: p.commitment,
        }
    }
}

#[cfg(feature = "std")]
impl Into<filecoin_proofs::PieceInfo> for PieceInfo {
    fn into(self) -> filecoin_proofs::PieceInfo {
        filecoin_proofs::PieceInfo::new(
            self.commitment,
            filecoin_proofs::UnpaddedBytesAmount(self.size),
        )
        .expect("commitment not to be empty")
    }
}
