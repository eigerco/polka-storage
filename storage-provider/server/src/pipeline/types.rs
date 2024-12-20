use std::path::PathBuf;

use primitives::{
    commitment::{piece::PieceInfo, CommD, CommP, CommR, Commitment},
    sector::SectorNumber,
    DealId,
};
use serde::{Deserialize, Serialize};
use storagext::types::market::DealProposal;

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

/// Unsealed Sector which still accepts deals and pieces.
/// When sealed it's converted into [`PreCommittedSector`].
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct UnsealedSector {
    /// [`SectorNumber`] which identifies a sector in the Storage Provider.
    ///
    /// It *should be centrally generated* by the Storage Provider, currently by [`crate::db::DealDB::next_sector_number`].
    pub sector_number: SectorNumber,

    /// Tracks how much bytes have been written into [`Sector::unsealed_path`]
    /// by [`polka_storage_proofs::porep::sealer::Sealer::add_piece`] which adds padding.
    ///
    /// It is used before precomit to calculate padding
    /// with zero pieces by [`polka_storage_proofs::porep::sealer::Sealer::pad_sector`].
    pub occupied_sector_space: u64,

    /// Tracks all of the pieces that has been added to the sector.
    /// Indexes match with corresponding deals in [`Sector::deals`].
    pub piece_infos: Vec<PieceInfo>,

    /// Tracks all of the deals that have been added to the sector.
    pub deals: Vec<(DealId, DealProposal)>,

    /// Path of an existing file where the pieces unsealed and padded data is stored.
    ///
    /// File at this path is created when the sector is created by [`Sector::create`].
    pub unsealed_path: std::path::PathBuf,
}

/// Sector which has been sealed and pre-committed on-chain.
/// When proven, it's converted into [`ProvenSector`].
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct PreCommittedSector {
    /// [`SectorNumber`] which identifies a sector in the Storage Provider.
    ///
    /// It *should be centrally generated* by the Storage Provider, currently by [`crate::db::DealDB::next_sector_number`].
    pub sector_number: SectorNumber,

    /// Tracks all of the pieces that has been added to the sector.
    /// Indexes match with corresponding deals in [`Sector::deals`].
    pub piece_infos: Vec<PieceInfo>,

    /// Tracks all of the deals that have been added to the sector.
    pub deals: Vec<(DealId, DealProposal)>,

    /// Cache directory of the sector.
    /// Each sector needs to have it's cache directory in a different place, because `p_aux` and `t_aux` are stored there.
    pub cache_path: std::path::PathBuf,

    /// Path of an existing file where the sealed sector data is stored.
    ///
    /// File at this path is initially created by [`Sector::create`], however it's empty.
    ///
    /// Only after pipeline [`PipelineMessage::PreCommit`],
    /// the file has contents which should not be touched and are used for later steps.
    pub sealed_path: std::path::PathBuf,

    /// Sealed sector commitment.
    pub comm_r: Commitment<CommR>,

    /// Data commitment of the sector.
    pub comm_d: Commitment<CommD>,

    /// Block at which randomness has been fetched to perform [`PipelineMessage::PreCommit`].
    ///
    /// It is used as a randomness seed to create a replica.
    /// Available at [`SectorState::Sealed`] and later.
    pub seal_randomness_height: u64,

    /// Block at which the sector was precommitted (extrinsic submitted on-chain).
    ///
    /// It is used as a randomness seed to create a PoRep.
    /// Available at [`SectorState::Precommitted`] and later.
    pub precommit_block: u64,
}

impl UnsealedSector {
    /// Creates a new sector and empty file at the provided path.
    ///
    /// Sector Number must be unique - generated by [`crate::db::DealDB::next_sector_number`]
    /// otherwise the data will be overwritten.
    pub async fn create(
        sector_number: SectorNumber,
        unsealed_path: std::path::PathBuf,
    ) -> Result<UnsealedSector, std::io::Error> {
        tokio::fs::File::create_new(&unsealed_path).await?;

        Ok(Self {
            sector_number,
            occupied_sector_space: 0,
            piece_infos: vec![],
            deals: vec![],
            unsealed_path,
        })
    }
}

impl PreCommittedSector {
    /// Transforms [`UnsealedSector`] and removes it's underlying data.
    ///
    /// Expects that file at `sealed_path` contains sealed_data.
    /// Should only be called after sealing and pre-commit process has ended.
    pub async fn create(
        unsealed: UnsealedSector,
        cache_path: std::path::PathBuf,
        sealed_path: std::path::PathBuf,
        comm_r: Commitment<CommR>,
        comm_d: Commitment<CommD>,
        seal_randomness_height: u64,
        precommit_block: u64,
    ) -> Result<Self, std::io::Error> {
        tokio::fs::remove_file(unsealed.unsealed_path).await?;

        Ok(Self {
            sector_number: unsealed.sector_number,
            piece_infos: unsealed.piece_infos,
            deals: unsealed.deals,
            cache_path,
            sealed_path,
            comm_r,
            comm_d,
            seal_randomness_height,
            precommit_block,
        })
    }
}

/// Sector which has been sealed, precommitted and proven on-chain.
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct ProvenSector {
    /// [`SectorNumber`] which identifies a sector in the Storage Provider.
    ///
    /// It *should be centrally generated* by the Storage Provider, currently by [`crate::db::DealDB::next_sector_number`].
    pub sector_number: SectorNumber,

    /// Tracks all of the pieces that has been added to the sector.
    /// Indexes match with corresponding deals in [`Sector::deals`].
    pub piece_infos: Vec<PieceInfo>,

    /// Tracks all of the deals that have been added to the sector.
    pub deals: Vec<(DealId, DealProposal)>,

    /// Cache directory of the sector.
    /// Each sector needs to have it's cache directory in a different place, because `p_aux` and `t_aux` are stored there.
    pub cache_path: std::path::PathBuf,

    /// Path of an existing file where the sealed sector data is stored.
    pub sealed_path: std::path::PathBuf,

    /// Sealed sector commitment.
    pub comm_r: Commitment<CommR>,

    /// Data commitment of the sector.
    pub comm_d: Commitment<CommD>,
}

impl ProvenSector {
    /// Creates a [`ProvenSector`] from a [`PreCommittedSector`].
    pub fn create(sector: PreCommittedSector) -> Self {
        Self {
            sector_number: sector.sector_number,
            piece_infos: sector.piece_infos,
            deals: sector.deals,
            cache_path: sector.cache_path,
            sealed_path: sector.sealed_path,
            comm_r: sector.comm_r,
            comm_d: sector.comm_d,
        }
    }
}
