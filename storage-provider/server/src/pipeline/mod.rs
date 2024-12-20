pub mod types;

use std::{path::PathBuf, sync::Arc};

use polka_storage_proofs::{
    porep::{
        sealer::{prepare_piece, BlstrsProof, PreCommitOutput, Sealer, SubstrateProof},
        PoRepError, PoRepParameters,
    },
    post::{self, PoStError, PoStParameters, ReplicaInfo},
};
use polka_storage_provider_common::rpc::ServerInfo;
use primitives::{
    commitment::{CommD, CommP, CommR, Commitment},
    proofs::derive_prover_id,
    randomness::{draw_randomness, DomainSeparationTag},
    sector::SectorNumber,
};
use storagext::{
    types::{
        market::DealProposal,
        storage_provider::{
            PartitionState, PoStProof, ProveCommitSector, SectorPreCommitInfo,
            SubmitWindowedPoStParams,
        },
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
    ProvenSector, SubmitWindowedPoStMessage, UnsealedSector,
};

use crate::db::{DBError, DealDB};

// TODO(@th7nder,#622,02/12/2024): query it from the chain.
const SECTOR_EXPIRATION_MARGIN: u64 = 20;

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    PoRepError(#[from] PoRepError),
    #[error(transparent)]
    PoStError(#[from] PoStError),
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
    #[error("current deadline or storage provider not found")]
    DeadlineNotFound,
    #[error("deadline of given index does not have a state")]
    DeadlineStateNotFound,
    #[error(transparent)]
    SendError(#[from] SendError<PipelineMessage>),
    #[error("failed to schedule windowed PoSt")]
    SchedulingError,
    #[error("Proving cancelled")]
    ProvingCancelled,
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
    pub post_parameters: Arc<PoStParameters>,

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
    fn prove_commit(
        &self,
        state: Arc<PipelineState>,
        msg: ProveCommitMessage,
        token: CancellationToken,
    );
    fn submit_windowed_post(
        &self,
        state: Arc<PipelineState>,
        msg: SubmitWindowedPoStMessage,
        token: CancellationToken,
    );
    fn schedule_posts(&self, state: Arc<PipelineState>);
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

    fn prove_commit(
        &self,
        state: Arc<PipelineState>,
        msg: ProveCommitMessage,
        token: CancellationToken,
    ) {
        let ProveCommitMessage { sector_number } = msg;
        self.spawn(async move {
            match prove_commit(state, sector_number, token).await {
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

    fn submit_windowed_post(
        &self,
        state: Arc<PipelineState>,
        msg: SubmitWindowedPoStMessage,
        token: CancellationToken,
    ) {
        let SubmitWindowedPoStMessage { deadline_index } = msg;
        self.spawn(async move {
            tokio::select! {
                // SubmitWindowedPoSt is not cancellation safe.
                res = submit_windowed_post(state, deadline_index) => {
                    match res {
                        Ok(_) => {
                            tracing::info!(
                                "SubmitWindowedPoSt for deadline {} finished successfully.",
                                deadline_index
                            )
                        }
                        Err(err) => {
                            tracing::error!(%err, "SubmitWindowedPoSt failed for deadline: {}", deadline_index)
                        }
                    }
                },
                () = token.cancelled() => {
                    tracing::warn!("submit_windowed_post for deadline {} has been cancelled.", deadline_index);
                }
            }
        });
    }

    fn schedule_posts(&self, state: Arc<PipelineState>) {
        self.spawn(async move {
            match schedule_posts(state).await {
                Ok(_) => {
                    tracing::info!("Scheduled Windowed PoSts...");
                }
                Err(err) => {
                    tracing::error!(%err, "Schedule PoSts failed");
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
        PipelineMessage::ProveCommit(msg) => {
            tracker.prove_commit(state.clone(), msg, token.clone())
        }
        PipelineMessage::SubmitWindowedPoStMessage(msg) => {
            tracker.submit_windowed_post(state.clone(), msg, token.clone())
        }
        PipelineMessage::SchedulePoSts => tracker.schedule_posts(state.clone()),
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
#[tracing::instrument(skip(state, deal, commitment))]
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

#[tracing::instrument(skip(state))]
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

    let cache_path = state.sealing_cache_dir.join(sector_number.to_string());
    let sealed_path = state.sealed_sectors_dir.join(sector_number.to_string());
    tokio::fs::create_dir_all(&cache_path).await?;
    tokio::fs::File::create_new(&sealed_path).await?;

    // TODO(@th7nder,31/10/2024): what happens if some of the process fails? SP will be slashed, and there is no error reporting? what about retries?
    let sealing_handle: JoinHandle<Result<PreCommitOutput, _>> = {
        let prover_id = derive_prover_id(state.xt_keypair.account_id());
        let cache_dir = cache_path.clone();
        let unsealed_path = sector.unsealed_path.clone();
        let sealed_path = sealed_path.clone();

        let piece_infos = sector.piece_infos.clone();
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
        cache_path,
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

#[tracing::instrument(skip(state, token))]
async fn prove_commit(
    state: Arc<PipelineState>,
    sector_number: SectorNumber,
    token: CancellationToken,
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
    tokio::select! {
        res = state.xt_client.wait_for_height(prove_commit_block, true) => {
            res?;
        },
        () = token.cancelled() => {
            tracing::warn!("Cancelled while waiting to get randomness at block {}", prove_commit_block);
            return Err(PipelineError::ProvingCancelled);
        }
    };

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
        let cache_dir = sector.cache_path.clone();
        let sealed_path = sector.sealed_path.clone();
        let piece_infos = sector.piece_infos.clone();

        tokio::task::spawn_blocking(move || {
            sealer.prove_sector(
                porep_params.as_ref(),
                cache_dir,
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

    let proofs = tokio::select! {
        // Up to this point everything is retryable.
        // Pipeline ends up being in an inconsistent state if we prove commit to the chain, and don't wait for it, so the sector's not persisted in the DB.
        res = sealing_handle => {
            res??
        },
        () = token.cancelled() => {
            return Err(PipelineError::ProvingCancelled);
        }
    };

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

#[tracing::instrument(skip(state))]
async fn submit_windowed_post(
    state: Arc<PipelineState>,
    deadline_index: u64,
) -> Result<(), PipelineError> {
    tracing::info!("Getting deadline info for {} deadline", deadline_index);
    let deadline = state
        .xt_client
        .deadline_info(&state.xt_keypair.account_id().into(), deadline_index)
        .await?;
    let Some(deadline) = deadline else {
        tracing::error!("there is no such deadline...");
        return Err(PipelineError::DeadlineNotFound);
    };

    tracing::debug!("Deadline Info: {:?}", deadline);
    tracing::info!(
        "Wait for challenge_block {}, start: {}, for deadline challenge",
        deadline.challenge_block,
        deadline.start
    );
    state
        .xt_client
        .wait_for_height(deadline.start, true)
        .await?;
    tracing::info!("Waiting finished (block: {}), let's go", deadline.start);

    let Some(digest) = state
        .xt_client
        .get_randomness(deadline.challenge_block)
        .await?
    else {
        tracing::error!("Randomness for the block not available.");
        return Err(PipelineError::RandomnessNotAvailable);
    };
    let entropy = state.xt_keypair.account_id().encode();
    let randomness = draw_randomness(
        &digest,
        DomainSeparationTag::WindowedPoStChallengeSeed,
        deadline.challenge_block,
        &entropy,
    );

    let Some(deadline_state) = state
        .xt_client
        .deadline_state(&state.xt_keypair.account_id().into(), deadline_index)
        .await?
    else {
        tracing::error!("Something went catastrophic, there is no current deadline state");
        return Err(PipelineError::DeadlineStateNotFound);
    };

    if deadline_state.partitions.len() > 1 {
        todo!("I don't know what to do: polka-storage#595");
    }
    if deadline_state.partitions.len() == 0 {
        tracing::info!("There are not partitions in this deadline yet. Nothing to prove here.");
        schedule_post(state, deadline_index)?;
        return Ok(());
    }

    let partitions = deadline_state.partitions.keys().cloned().collect();
    let (_partition_number, PartitionState { sectors }) = deadline_state
        .partitions
        .first_key_value()
        .expect("1 partition to be there");

    if sectors.len() == 0 {
        tracing::info!("Every sector expired... Nothing to prove here.");
        schedule_post(state, deadline_index)?;
        return Ok(());
    }

    let mut replicas = Vec::new();
    for sector_number in sectors {
        let sector = state
            .db
            .get_sector::<ProvenSector>(*sector_number)?
            .ok_or(PipelineError::SectorNotFound)?;

        replicas.push(ReplicaInfo {
            sector_id: *sector_number,
            comm_r: sector.comm_r.raw(),
            cache_path: sector.cache_path.clone(),
            replica_path: sector.sealed_path.clone(),
        });
    }
    let prover_id = derive_prover_id(state.xt_keypair.account_id());

    tracing::info!("Proving PoSt partitions... {:?}", partitions);
    let handle: JoinHandle<Result<Vec<BlstrsProof>, _>> = {
        let post_params = state.post_parameters.clone();
        let post_proof = state.server_info.post_proof;

        tokio::task::spawn_blocking(move || {
            post::generate_window_post(post_proof, &post_params, randomness, prover_id, replicas)
        })
    };
    let proofs = handle.await??;

    // TODO(@th7nder,#595,06/12/2024): how many proofs are for how many partitions and why
    // don't now why yet, need to figure this out
    let proof: SubstrateProof = proofs[0]
        .clone()
        .try_into()
        .expect("converstion between rust-fil-proofs and polka-storage-proofs to work");
    let proof = codec::Encode::encode(&proof);

    tracing::info!("Generated PoSt proof for partitions: {:?}", partitions);

    tracing::info!("Wait for block {} for open deadline", deadline.start,);
    state
        .xt_client
        .wait_for_height(deadline.start, true)
        .await?;

    let result = state
        .xt_client
        .submit_windowed_post(
            &state.xt_keypair,
            SubmitWindowedPoStParams {
                deadline: deadline_index,
                partitions: partitions,
                proof: PoStProof {
                    post_proof: state.server_info.post_proof,
                    proof_bytes: proof,
                },
            },
            true,
        )
        .await?
        .expect("waiting for finalization should always give results");

    let posts = result
        .events
        .find::<storagext::runtime::storage_provider::events::ValidPoStSubmitted>()
        .map(|result| result.map_err(|err| subxt::Error::from(err)))
        .collect::<Result<Vec<_>, _>>()?;

    tracing::info!("Successfully submitted PoSt on-chain: {:?}", posts);

    schedule_post(state, deadline_index)?;

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn schedule_posts(state: Arc<PipelineState>) -> Result<(), PipelineError> {
    let proving_period = state.xt_client.proving_period_info()?;

    for deadline_index in 0..proving_period.deadlines {
        schedule_post(state.clone(), deadline_index)?;
    }

    Ok(())
}

#[tracing::instrument(skip(state))]
fn schedule_post(state: Arc<PipelineState>, deadline_index: u64) -> Result<(), PipelineError> {
    state
        .pipeline_sender
        .send(PipelineMessage::SubmitWindowedPoStMessage(
            SubmitWindowedPoStMessage { deadline_index },
        ))
        .map_err(|err| {
            tracing::error!(%err, "failed to send a messsage to the pipeline");
            PipelineError::SchedulingError
        })?;

    tracing::info!("Scheduled Windowed PoSt for deadline: {}", deadline_index);

    Ok(())
}
