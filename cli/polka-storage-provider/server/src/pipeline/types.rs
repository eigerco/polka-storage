use primitives_commitment::{
    piece::{PieceInfo, UnpaddedPieceSize},
    Commitment,
};
use primitives_proofs::{DealId, SectorNumber};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use storagext::types::market::DealProposal;

#[derive(Debug)]
pub struct AddPieceMessage {
    pub deal: DealProposal,
    pub published_deal_id: u64,
    pub piece_path: PathBuf,
    pub piece_cid: Commitment,
}

#[derive(Debug)]
pub struct PreCommitMessage {
    pub sector_id: SectorNumber,
}

#[derive(Debug)]
pub enum PipelineMessage {
    AddPiece(AddPieceMessage),
    PreCommit(PreCommitMessage),
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Sector {
    pub id: SectorNumber,
    pub state: SectorState,
    pub occupied_sector_space: UnpaddedPieceSize,
    pub piece_infos: Vec<PieceInfo>,
    pub deals: Vec<(DealId, DealProposal)>,
    pub unsealed_path: std::path::PathBuf,
    pub sealed_path: std::path::PathBuf,
}

impl Sector {
    pub async fn create(
        id: SectorNumber,
        unsealed_path: std::path::PathBuf,
        sealed_path: std::path::PathBuf,
    ) -> Result<Sector, std::io::Error> {
        tokio::fs::File::create(&unsealed_path).await?;
        tokio::fs::File::create(&sealed_path).await?;

        Ok(Self {
            id,
            state: SectorState::Unsealed,
            occupied_sector_space: UnpaddedPieceSize::ZERO,
            piece_infos: vec![],
            deals: vec![],
            unsealed_path,
            sealed_path,
        })
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum SectorState {
    Unsealed,
    Sealed,
    Precommitted,
    Proven,
}
