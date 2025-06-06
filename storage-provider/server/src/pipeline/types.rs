use std::path::PathBuf;

use primitives::{
    commitment::{CommP, Commitment},
    sector::SectorNumber,
};
use storagext::types::storage_provider::DealProposal;

/// Represents a task to be executed on the Storage Provider Pipeline
#[derive(Debug)]
pub enum PipelineMessage {
    /// Adds a deal to a sector selected by the storage provider.
    AddPiece(AddPieceMessage),
    /// Pads, seals a sector and pre-commits it on-chain.
    PreCommit(PreCommitMessage),
    /// Generates a PoRep for a sector and verifies the proof on-chain.
    ProveCommit(ProveCommitMessage),
    /// Fetches partitions and sectors from the chain and generates a Windowed PoSt proof.
    SubmitWindowedPoStMessage(SubmitWindowedPoStMessage),
    /// Schedules WindowPoSt for each deadline in the proving period.
    SchedulePoSts,
}

impl PipelineMessage {
    pub fn pre_commit(sector_number: SectorNumber) -> Self {
        Self::PreCommit(PreCommitMessage { sector_number })
    }
}

/// Deal to be added to a sector with its contents.
#[derive(Debug)]
pub struct AddPieceMessage {
    /// Published deal
    pub deal: DealProposal,
    /// Deal id received as a result of `publish_storage_deals` extrinsic
    pub published_deal_id: u64,
    /// Path where the deal data (.car archive) is stored
    pub piece_path: PathBuf,
    /// CommP of the .car archive stored at `piece_path`
    pub commitment: Commitment<CommP>,
}

/// Sector to be sealed and pre-commited to the chain
#[derive(Debug)]
pub struct PreCommitMessage {
    /// Number of an existing sector
    pub sector_number: SectorNumber,
}

#[derive(Debug)]
pub struct ProveCommitMessage {
    /// Number of an existing, pre-committed sector
    pub sector_number: SectorNumber,
}

#[derive(Debug)]
pub struct SubmitWindowedPoStMessage {
    pub deadline_index: u64,
}
