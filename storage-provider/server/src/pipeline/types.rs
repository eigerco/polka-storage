use std::{path::PathBuf, sync::Arc};

use polka_storage_proofs::porep::{sealer::{prepare_piece, PreCommitOutput, Sealer}, PoRepError};
use primitives::{
    commitment::{piece::PieceInfo, CommD, CommP, CommR, Commitment}, proofs::{derive_prover_id, RegisteredSealProof}, randomness::{draw_randomness, DomainSeparationTag}, sector::SectorNumber, DealId
};
use serde::{Deserialize, Serialize};
use storagext::{types::{market::DealProposal, storage_provider::SectorPreCommitInfo}, RandomnessClientExt, StorageProviderClientExt, SystemClientExt};
use tokio::task::{JoinError, JoinHandle};
use subxt::{tx::Signer, ext::codec::Encode};

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

#[derive(Debug, thiserror::Error)]
pub enum SectorError {
    #[error(transparent)]
    PoRepError(#[from] PoRepError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
}

/// Unsealed Sector which still accepts deals and pieces.
/// When sealed it's converted into [`PreCommittedSector`].
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct UnsealedSector {
    seal_proof: RegisteredSealProof,

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


// TODO(@th7nder,#622,02/12/2024): query it from the chain.
const SECTOR_EXPIRATION_MARGIN: u64 = 20;

impl UnsealedSector {
    /// Creates a new sector and empty file at the provided path.
    ///
    /// Sector Number must be unique - generated by [`crate::db::DealDB::next_sector_number`]
    /// otherwise the data will be overwritten.
    pub async fn create(
        seal_proof: RegisteredSealProof,
        sector_number: SectorNumber,
        unsealed_path: std::path::PathBuf,
    ) -> Result<UnsealedSector, std::io::Error> {
        tokio::fs::File::create_new(&unsealed_path).await?;

        Ok(Self {
            seal_proof,
            sector_number,
            occupied_sector_space: 0,
            piece_infos: vec![],
            deals: vec![],
            unsealed_path,
        })
    }

    pub async fn add_piece(
        &mut self,
        deal_id: u64,
        deal: DealProposal,
        piece_path: PathBuf,
        commitment: Commitment<CommP>,
    ) -> Result<(), SectorError> {
        self.deals.push((deal_id, deal));
        let sealer = Sealer::new(self.seal_proof);

        // would love to use something like scoped spawn blocking
        let pieces = self.piece_infos.clone();
        let unsealed_path = self.unsealed_path.clone();
        let handle: JoinHandle<Result<(PieceInfo, u64), SectorError>> =
            tokio::task::spawn_blocking(move || {
                let unsealed_sector = std::fs::File::options().append(true).open(unsealed_path)?;

                tracing::info!("Preparing piece...");
                let (padded_reader, piece_info) = prepare_piece(piece_path, commitment)?;
                tracing::info!("Adding piece...");
                let occupied_piece_space =
                    sealer.add_piece(padded_reader, piece_info, &pieces, unsealed_sector)?;

                Ok((piece_info, occupied_piece_space))
            });

        let (piece_info, occupied_piece_space) = handle.await??;
        self.piece_infos.push(piece_info);
        self.occupied_sector_space = self.occupied_sector_space + occupied_piece_space;

        Ok(())
    }

    pub async fn pre_commit(mut self,
        xt_client: Arc<storagext::Client>,
        xt_keypair: &storagext::multipair::MultiPairSigner,
        cache_dir_path: PathBuf,
        sealed_path: PathBuf,
    ) -> Result<PreCommittedSector, SectorError> {
        let sealer: Sealer = Sealer::new(self.seal_proof);

        tokio::fs::create_dir_all(&cache_dir_path).await?;
        tokio::fs::File::create_new(&sealed_path).await?;

        // Pad sector so CommD can be properly calculated.
        self.piece_infos = sealer.pad_sector(&self.piece_infos, self.occupied_sector_space)?;
        tracing::debug!("piece_infos: {:?}", self.piece_infos);
        tracing::info!("Padded sector, commencing pre-commit and getting last finalized block");

        let current_block = xt_client.height(true).await?;
        tracing::info!("Current block: {current_block}");

        let digest = xt_client
            .get_randomness(current_block)
            .await?
            .expect("randomness to be available as we wait for it");

        let entropy = xt_keypair.account_id().encode();
        // Must match pallet's logic or otherwise proof won't be verified:
        // https://github.com/eigerco/polka-storage/blob/af51a9b121c9b02e0bf6f02f5e835091ab46af76/pallets/storage-provider/src/lib.rs#L1539
        let ticket = draw_randomness(
            &digest,
            DomainSeparationTag::SealRandomness,
            current_block,
            &entropy,
        );

        let sealing_handle: JoinHandle<Result<PreCommitOutput, _>> = {
            let prover_id = derive_prover_id(xt_keypair.account_id());
            let cache_dir = cache_dir_path.clone();
            let unsealed_path = self.unsealed_path.clone();
            let sealed_path = sealed_path.clone();
            let sector_number = self.sector_number;

            let piece_infos = self.piece_infos.clone();
            tokio::task::spawn_blocking(move || {
                sealer.precommit_sector(
                    cache_dir,
                    unsealed_path,
                    sealed_path,
                    prover_id,
                    sector_number,
                    ticket,
                    &piece_infos,
                )
            })
        };
        let sealing_output = sealing_handle.await??;

        tracing::info!(
            "Created sector's replica, CommD: {}, CommR: {}",
            sealing_output.comm_d.cid(),
            sealing_output.comm_r.cid()
        );

        let sealing_output_commr = Commitment::<CommR>::from(sealing_output.comm_r);
        let sealing_output_commd = Commitment::<CommD>::from(sealing_output.comm_d);


        tracing::debug!("Precommiting at block: {}", current_block);
        let result = xt_client
            .pre_commit_sectors(
                xt_keypair,
                vec![SectorPreCommitInfo {
                    deal_ids: self.deals.iter().map(|(id, _)| *id).collect(),
                    expiration: self
                        .deals
                        .iter()
                        .map(|(_, deal)| deal.end_block)
                        .max()
                        .expect("always at least 1 deal in a sector")
                        + SECTOR_EXPIRATION_MARGIN,
                    sector_number: self.sector_number,
                    seal_proof: self.seal_proof,
                    sealed_cid: sealing_output_commr.cid(),
                    unsealed_cid: sealing_output_commd.cid(),
                    seal_randomness_height: current_block,
                }],
                true,
            )
            .await?
            .expect("we're waiting for the result");

        let precommited_sectors = result
            .events
            .find::<storagext::runtime::storage_provider::events::SectorsPreCommitted>()
            // `.find` returns subxt_core::Error which while it is convertible to subxt::Error as shown
            // it can't be converted by a single ? on the collect, so the type system tries instead
            // subxt_core::Error -> PipelineError
            .map(|result| result.map_err(|err| subxt::Error::from(err)))
            .collect::<Result<Vec<_>, _>>()?;

        tracing::info!(
            "Successfully pre-commited sectors on-chain: {:?}",
            precommited_sectors
        );

        Ok(PreCommittedSector::create(
            self,
            cache_dir_path,
            sealed_path,
            sealing_output_commr,
            sealing_output_commd,
            current_block,
            precommited_sectors[0].block,
        )
        .await?)
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
