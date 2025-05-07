use crate::{
    commitment::{piece::PaddedPieceSize, CommD, CommR, Commitment},
    sector::SectorNumber,
};

// If this is changed, also update SectorData in `storage-provider/client/src/commands/proofs.rs`.
#[derive(Debug)]
pub struct SectorData {
    pub sector_number: SectorNumber,
    pub padded_piece_size: PaddedPieceSize,
    pub comm_r: Commitment<CommR>,
    pub comm_d: Commitment<CommD>,
    pub porep_proof: &'static [u8],
}
