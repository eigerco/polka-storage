use cid::Cid;
use primitives_proofs::SectorNumber;
use std::path::PathBuf;
use storagext::types::market::DealProposal;

#[derive(Debug)]
pub struct AddPieceMessage {
    pub deal: DealProposal,
    pub published_deal_id: u64,
    pub piece_path: PathBuf,
    pub piece_cid: Cid,
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
