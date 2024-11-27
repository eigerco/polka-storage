pub mod types;

use std::{path::PathBuf, sync::Arc};

use polka_storage_proofs::porep::{
    sealer::{prepare_piece, BlstrsProof, PreCommitOutput, Sealer, SubstrateProof},
    PoRepError, PoRepParameters,
};
use polka_storage_provider_common::rpc::ServerInfo;
use primitives_commitment::{CommD, CommP, CommR, Commitment};
use primitives_proofs::{
    derive_prover_id,
    randomness::{draw_randomness, DomainSeparationTag},
    SectorNumber,
};
use storagext::{
    types::{
        market::DealProposal,
        storage_provider::{ProveCommitSector, SectorPreCommitInfo},
    },
    RandomnessClientExt, StorageProviderClientExt, SystemClientExt,
};
use subxt::{ext::codec::Encode, tx::Signer};
use tokio::{
    sync::mpsc::{error::SendError, UnboundedReceiver, UnboundedSender},
    task::{JoinError, JoinHandle},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use types::{
    AddPieceMessage, PipelineMessage, PreCommitMessage, PreCommittedSector, ProveCommitMessage,
    ProvenSector, UnsealedSector,
};

use crate::db::{DBError, DealDB};

const SECTOR_EXPIRATION_MARGIN: u64 = 20;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    PoRepError(#[from] PoRepError),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    DBError(#[from] DBError),
    #[error("sector does not exist")]
    SectorNotFound,
    #[error("precommit scheduled too early, randomness not available")]
    RandomnessNotAvailable,
    #[error(transparent)]
    SendError(#[from] SendError<PipelineMessage>),
    #[error("Custom error: {0}")]
    CustomError(String),
}
/// Pipeline shared state.
pub struct PipelineState {
    pub server_info: ServerInfo,
    pub db: Arc<DealDB>,
    pub unsealed_sectors_dir: Arc<PathBuf>,
    pub sealed_sectors_dir: Arc<PathBuf>,
    pub sealing_cache_dir: Arc<PathBuf>,
    pub porep_parameters: Arc<PoRepParameters>,

    pub xt_client: Arc<storagext::Client>,
    pub xt_keypair: storagext::multipair::MultiPairSigner,
    pub pipeline_sender: UnboundedSender<PipelineMessage>,
}

#[tracing::instrument(skip_all)]
pub async fn start_pipeline(
    state: Arc<PipelineState>,
    mut receiver: UnboundedReceiver<PipelineMessage>,
    token: CancellationToken,
) -> Result<(), std::io::Error> {
    let tracker = TaskTracker::new();

    loop {
        tokio::select! {
            msg = receiver.recv() => {
                tracing::debug!("Received msg: {:?}", msg);
                match msg {
                    Some(msg) => {
                        process(&tracker, msg, state.clone(), token.clone());
                    },
                    None => {
                        tracing::info!("Channel has been closed...");
                        break;
                    },
                }
            },
            _ = token.cancelled() => {
                tracing::info!("Pipeline has been stopped by the cancellation token...");
                break;
            },
        }
    }

    tracker.close();
    tracker.wait().await;

    Ok(())
}

trait PipelineOperations {
    fn add_piece(&self, state: Arc<PipelineState>, msg: AddPieceMessage, token: CancellationToken);
    fn precommit(&self, state: Arc<PipelineState>, msg: PreCommitMessage);
    fn prove_commit(&self, state: Arc<PipelineState>, msg: ProveCommitMessage);
}

impl PipelineOperations for TaskTracker {
    fn add_piece(&self, state: Arc<PipelineState>, msg: AddPieceMessage, token: CancellationToken) {
        let AddPieceMessage {
            deal,
            published_deal_id,
            piece_path,
            commitment,
        } = msg;
        self.spawn(async move {
            tokio::select! {
                // AddPiece is cancellation safe, as it can be retried and the state will be fine.
                res = add_piece(state, piece_path, commitment, deal, published_deal_id) => {
                    match res {
                        Ok(_) => tracing::info!("Add Piece for piece {}, deal id {}, finished successfully.", commitment, published_deal_id),
                        Err(err) => tracing::error!(%err, "Add Piece for piece {}, deal id {}, failed!", commitment, published_deal_id),
                    }
                },
                () = token.cancelled() => {
                    tracing::warn!("AddPiece has been cancelled.");
                }
            }
        });
    }

    fn precommit(&self, state: Arc<PipelineState>, msg: PreCommitMessage) {
        let PreCommitMessage { sector_number } = msg;
        self.spawn(async move {
            // Precommit is not cancellation safe.
            // TODO(@th7nder,#501, 04/11/2024): when it's cancelled, it can hang and user will have to wait for it to finish.
            // If they don't the state can be corrupted, we could improve that situation.
            // One of the ideas is to store state as 'Precommitting' so then we know we can retry that after some time.
            match precommit(state, sector_number).await {
                Ok(_) => {
                    tracing::info!(
                        "Precommit for sector {} finished successfully.",
                        sector_number
                    )
                }
                Err(err) => {
                    tracing::error!(%err, "Failed PreCommit for Sector: {}", sector_number)
                }
            }
        });
    }

    fn prove_commit(&self, state: Arc<PipelineState>, msg: ProveCommitMessage) {
        let ProveCommitMessage { sector_number } = msg;
        self.spawn(async move {
            // ProveCommit is not cancellation safe.
            match prove_commit(state, sector_number).await {
                Ok(_) => {
                    tracing::info!(
                        "ProveCommit for sector {} finished successfully.",
                        sector_number
                    )
                }
                Err(err) => {
                    tracing::error!(%err, "Failed ProveCommit for Sector: {}", sector_number)
                }
            }
        });
    }
}

fn process(
    tracker: &TaskTracker,
    msg: PipelineMessage,
    state: Arc<PipelineState>,
    token: CancellationToken,
) {
    match msg {
        PipelineMessage::AddPiece(msg) => tracker.add_piece(state.clone(), msg, token.clone()),
        PipelineMessage::PreCommit(msg) => tracker.precommit(state.clone(), msg),
        PipelineMessage::ProveCommit(msg) => tracker.prove_commit(state.clone(), msg),
    }
}

async fn find_sector_for_piece(
    state: &Arc<PipelineState>,
) -> Result<UnsealedSector, PipelineError> {
    // TODO(@th7nder,30/10/2024): simplification, we're always creating a new sector for storing a piece.
    // It should not work like that, sectors should be filled with pieces according to *some* algorithm.
    let sector_number = state
        .db
        .next_sector_number()
        .map_err(|err| PipelineError::CustomError(err.to_string()))?;
    let unsealed_path = state.unsealed_sectors_dir.join(sector_number.to_string());
    let sector = UnsealedSector::create(sector_number, unsealed_path).await?;

    Ok(sector)
}

/// Finds a sector to which a piece will fit and adds it to the sector.
/// This function is *cancellation safe* as if future is dropped,
/// it can be dropped only when waiting for `spawn_blocking`.
/// When dropped when waiting, the sector state won't be preserved and adding piece can be retried.
#[tracing::instrument(skip_all, fields(piece_cid, deal_id))]
async fn add_piece(
    state: Arc<PipelineState>,
    piece_path: PathBuf,
    commitment: Commitment<CommP>,
    deal: DealProposal,
    deal_id: u64,
) -> Result<(), PipelineError> {
    let mut sector = find_sector_for_piece(&state).await?;
    sector.deals.push((deal_id, deal));

    tracing::info!("Adding a piece...");

    let sealer = Sealer::new(state.server_info.seal_proof);
    let handle: JoinHandle<Result<UnsealedSector, PipelineError>> =
        tokio::task::spawn_blocking(move || {
            let unsealed_sector = std::fs::File::options()
                .append(true)
                .open(&sector.unsealed_path)?;

            tracing::info!("Preparing piece...");
            let (padded_reader, piece_info) = prepare_piece(piece_path, commitment)?;
            tracing::info!("Adding piece...");
            let occupied_piece_space = sealer.add_piece(
                padded_reader,
                piece_info,
                &sector.piece_infos,
                unsealed_sector,
            )?;

            sector.piece_infos.push(piece_info);
            sector.occupied_sector_space = sector.occupied_sector_space + occupied_piece_space;

            Ok(sector)
        });
    let sector: UnsealedSector = handle.await??;

    tracing::info!("Finished adding a piece");
    state.db.save_sector(sector.sector_number, &sector)?;

    // TODO(@th7nder,30/10/2024): simplification, as we're always scheduling a precommit just after adding a piece and creating a new sector.
    // Ideally sector won't be finalized after one piece has been added and the precommit will depend on the start_block?
    state
        .pipeline_sender
        .send(PipelineMessage::PreCommit(PreCommitMessage {
            sector_number: sector.sector_number,
        }))?;

    Ok(())
}

#[tracing::instrument(skip_all, fields(sector_number))]
/// Creates a replica and calls pre-commit on-chain.
///
/// This method is *NOT CANCELLATION SAFE*.
/// When interrupted while waiting for the extrinsic call to return,
/// the Storage Provider is not consistent of the on-chain state,
/// cancelling this task effectively breaks the state sync.
async fn precommit(
    state: Arc<PipelineState>,
    sector_number: SectorNumber,
) -> Result<(), PipelineError> {
    tracing::info!("Starting pre-commit");

    let sealer = Sealer::new(state.server_info.seal_proof);
    let Some(mut sector) = state.db.get_sector::<UnsealedSector>(sector_number)? else {
        tracing::error!("Tried to precommit non-existing sector");
        return Err(PipelineError::SectorNotFound);
    };
    // Pad sector so CommD can be properly calculated.
    sector.piece_infos = sealer.pad_sector(&sector.piece_infos, sector.occupied_sector_space)?;
    tracing::debug!("piece_infos: {:?}", sector.piece_infos);

    tracing::info!("Padded sector, commencing pre-commit and getting last finalized block");

    let current_block = state.xt_client.height(true).await?;
    tracing::info!("Current block: {current_block}");

    let digest = state
        .xt_client
        .get_randomness(current_block)
        .await?
        .expect("randomness to be available as we wait for it");

    let entropy = state.xt_keypair.account_id().encode();
    // Must match pallet's logic or otherwise proof won't be verified:
    // https://github.com/eigerco/polka-storage/blob/af51a9b121c9b02e0bf6f02f5e835091ab46af76/pallets/storage-provider/src/lib.rs#L1539
    let ticket = draw_randomness(
        &digest,
        DomainSeparationTag::SealRandomness,
        current_block,
        &entropy,
    );

    let sealed_path = state.sealed_sectors_dir.join(sector_number.to_string());
    tokio::fs::File::create_new(&sealed_path).await?;

    // TODO(@th7nder,31/10/2024): what happens if some of the process fails? SP will be slashed, and there is no error reporting? what about retries?
    let sealing_handle: JoinHandle<Result<PreCommitOutput, _>> = {
        let prover_id = derive_prover_id(state.xt_keypair.account_id());
        let cache_dir = state.sealing_cache_dir.clone();
        let unsealed_path = sector.unsealed_path.clone();
        let sealed_path = sealed_path.clone();

        let piece_infos = sector.piece_infos.clone();
        tokio::task::spawn_blocking(move || {
            sealer.precommit_sector(
                cache_dir.as_ref(),
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
    let result = state
        .xt_client
        .pre_commit_sectors(
            &state.xt_keypair,
            vec![SectorPreCommitInfo {
                deal_ids: sector.deals.iter().map(|(id, _)| *id).collect(),
                expiration: sector
                    .deals
                    .iter()
                    .map(|(_, deal)| deal.end_block)
                    .max()
                    .expect("always at least 1 deal in a sector")
                    + SECTOR_EXPIRATION_MARGIN,
                sector_number: sector_number,
                seal_proof: state.server_info.seal_proof,
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

    let sector = PreCommittedSector::create(
        sector,
        sealed_path,
        sealing_output_commr,
        sealing_output_commd,
        current_block,
        precommited_sectors[0].block,
    )
    .await?;
    state.db.save_sector(sector.sector_number, &sector)?;

    tracing::info!(
        "Successfully pre-commited sectors on-chain: {:?}",
        precommited_sectors
    );

    state
        .pipeline_sender
        .send(PipelineMessage::ProveCommit(ProveCommitMessage {
            sector_number: sector.sector_number,
        }))?;

    Ok(())
}

#[tracing::instrument(skip_all, fields(sector_number))]
async fn prove_commit(
    state: Arc<PipelineState>,
    sector_number: SectorNumber,
) -> Result<(), PipelineError> {
    tracing::info!("Starting prove commit");

    let sealer = Sealer::new(state.server_info.seal_proof);
    let Some(sector) = state.db.get_sector::<PreCommittedSector>(sector_number)? else {
        tracing::error!("Tried to precommit non-existing sector");
        return Err(PipelineError::SectorNotFound);
    };

    let seal_randomness_height = sector.seal_randomness_height;
    let Some(digest) = state
        .xt_client
        .get_randomness(seal_randomness_height)
        .await?
    else {
        tracing::error!("Out-of-the-state transition, this SHOULD NOT happen");
        return Err(PipelineError::RandomnessNotAvailable);
    };
    let entropy = state.xt_keypair.account_id().encode();
    // Must match pallet's logic or otherwise proof won't be verified:
    // https://github.com/eigerco/polka-storage/blob/af51a9b121c9b02e0bf6f02f5e835091ab46af76/pallets/storage-provider/src/lib.rs#L1539
    let ticket = draw_randomness(
        &digest,
        DomainSeparationTag::SealRandomness,
        seal_randomness_height,
        &entropy,
    );

    // TODO(@th7nder,04/11/2024):
    // https://github.com/eigerco/polka-storage/blob/5edd4194f08f29d769c277577ccbb70bb6ff63bc/runtime/src/configs/mod.rs#L360
    // 10 blocks = 1 minute, only testnet
    const PRECOMMIT_CHALLENGE_DELAY: u64 = 10;
    let prove_commit_block = sector.precommit_block + PRECOMMIT_CHALLENGE_DELAY;

    tracing::info!("Wait for block {} to get randomness", prove_commit_block);
    state
        .xt_client
        .wait_for_height(prove_commit_block, true)
        .await?;
    let Some(digest) = state.xt_client.get_randomness(prove_commit_block).await? else {
        tracing::error!("Randomness for the block not available.");
        return Err(PipelineError::RandomnessNotAvailable);
    };
    let seed = draw_randomness(
        &digest,
        DomainSeparationTag::InteractiveSealChallengeSeed,
        prove_commit_block,
        &entropy,
    );

    let prover_id = derive_prover_id(state.xt_keypair.account_id());
    tracing::debug!("Performing prove commit for, seal_randomness_height {}, pre_commit_block: {}, prove_commit_block: {}, entropy: {}, ticket: {}, seed: {}, prover id: {}, sector_number: {}",
        seal_randomness_height, sector.precommit_block, prove_commit_block, hex::encode(entropy), hex::encode(ticket), hex::encode(seed), hex::encode(prover_id), sector_number);

    let sealing_handle: JoinHandle<Result<Vec<BlstrsProof>, _>> = {
        let porep_params = state.porep_parameters.clone();
        let cache_dir = state.sealing_cache_dir.clone();
        let sealed_path = sector.sealed_path.clone();
        let piece_infos = sector.piece_infos.clone();

        tokio::task::spawn_blocking(move || {
            sealer.prove_sector(
                porep_params.as_ref(),
                cache_dir.as_ref(),
                sealed_path,
                prover_id,
                sector_number,
                ticket,
                Some(seed),
                PreCommitOutput {
                    comm_r: sector.comm_r,
                    comm_d: sector.comm_d,
                },
                &piece_infos,
            )
        })
    };
    let proofs = sealing_handle.await??;

    // We use sector size 2KiB only at this point, which guarantees to have 1 proof, because it has 1 partition in the config.
    // That's why `prove_commit` will always generate a 1 proof.
    let proof: SubstrateProof = proofs[0]
        .clone()
        .try_into()
        .expect("converstion between rust-fil-proofs and polka-storage-proofs to work");
    let proof = codec::Encode::encode(&proof);
    tracing::info!("Proven sector: {}", sector_number);

    let result = state
        .xt_client
        .prove_commit_sectors(
            &state.xt_keypair,
            vec![ProveCommitSector {
                sector_number,
                proof,
            }],
            true,
        )
        .await?
        .expect("waiting for finalization should always give results");

    let proven_sectors = result
        .events
        .find::<storagext::runtime::storage_provider::events::SectorsProven>()
        .map(|result| result.map_err(|err| subxt::Error::from(err)))
        .collect::<Result<Vec<_>, _>>()?;

    tracing::info!("Successfully proven sectors on-chain: {:?}", proven_sectors);

    let sector = ProvenSector::create(sector);
    state.db.save_sector(sector.sector_number, &sector)?;

    Ok(())
}
