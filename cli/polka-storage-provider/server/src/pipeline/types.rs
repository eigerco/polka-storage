use std::path::PathBuf;

use primitives_commitment::{piece::PieceInfo, Commitment};
use primitives_proofs::{DealId, SectorNumber};
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
    pub piece_cid: Commitment,
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

/// Sector State serialized and stored in the RocksDB database
/// It is used for tracking the sector lifetime, precommiting and proving.
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct Sector {
    /// [`SectorNumber`] which identifies a sector in the Storage Provider.
    ///
    /// It *should be centrally generated* by the Storage Provider, currently by [`crate::db::DealDB::next_sector_number`].
    pub sector_number: SectorNumber,
    /// Initially the sector is in [`SectorState::Unsealed`] state, should be changed after each of the sealing steps.
    pub state: SectorState,
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
    /// Path of an existing file where the sealed sector data is stored.
    ///
    /// File at this path is initially created by [`Sector::create`], however it's empty.
    ///
    /// Only after pipeline [`PipelineMessage::PreCommit`],
    /// the file has contents which should not be touched and are used for later steps.
    pub sealed_path: std::path::PathBuf,
    /// CID of the sealed sector.
    ///
    /// Available at [`SectorState::Sealed`]/[`PipelineMessage::PreCommit`] and later.
    pub comm_r: Option<Commitment>,
    /// CID of the unsealed data of the sector.
    ///
    /// Available at [`SectorState::Sealed`]/[`PipelineMessage::PreCommit`] and later.
    pub comm_d: Option<Commitment>,
    /// Block at which randomness has been fetched to perform [`PipelineMessage::PreCommit`].
    ///
    /// It is used as a randomness seed to create a replica.
    /// Available at [`SectorState::Sealed`] and later.
    pub seal_randomness_height: Option<u64>,
    /// Block at which the sector was precommitted (extrinsic submitted on-chain).
    ///
    /// It is used as a randomness seed to create a PoRep.
    /// Available at [`SectorState::Precommitted`] and later.
    pub precommit_block: Option<u64>,
}

impl Sector {
    /// Creates a new sector and empty files at the provided paths.
    ///
    /// Sector Number must be unique - generated by [`crate::db::DealDB::next_sector_number`]
    /// otherwise the data will be overwritten.
    pub async fn create_unsealed(
        sector_number: SectorNumber,
        unsealed_path: std::path::PathBuf,
        sealed_path: std::path::PathBuf,
    ) -> Result<Sector, std::io::Error> {
        tokio::fs::File::create_new(&unsealed_path).await?;
        tokio::fs::File::create_new(&sealed_path).await?;

        Ok(Self {
            sector_number,
            state: SectorState::Unsealed,
            occupied_sector_space: 0,
            piece_infos: vec![],
            deals: vec![],
            unsealed_path,
            sealed_path,
            comm_r: None,
            comm_d: None,
            seal_randomness_height: None,
            precommit_block: None,
        })
    }
}

/// Represents a Sector's lifetime, sequentially.
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub enum SectorState {
    /// When sector still has remaining space to add pieces or has not been sealed yet.
    Unsealed,
    /// After sector has been filled with pieces, padded and a replica with CommR has been created out of it.
    Sealed,
    /// Sealed sector has been published on-chain, so now the PoRep must be generated for it.
    Precommitted,
    /// After a PoRep for a sector has been generated.
    Proven,
    /// Generated PoRep for a sector has been published and verified on-chain.
    ProveCommitted,
}
