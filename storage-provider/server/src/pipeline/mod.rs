pub mod types;

use std::{collections::BTreeSet, path::PathBuf, sync::Arc};

use polka_storage_proofs::{
    porep::{
        sealer::{BlstrsProof, SubstrateProof},
        PoRepError, PoRepParameters,
    },
    post::{self, PoStError, PoStParameters, ReplicaInfo},
};
use polka_storage_provider_common::{
    rpc::ServerInfo,
    sector::{PreCommittedSector, ProvenSector, SectorError, UnsealedSector},
};
use primitives::{
    commitment::{CommP, Commitment},
    proofs::derive_prover_id,
    randomness::{draw_randomness, DomainSeparationTag},
    sector::SectorNumber,
};
use storagext::{
    types::{
        market::DealProposal,
        storage_provider::{PartitionState, PoStProof, SubmitWindowedPoStParams},
    },
    RandomnessClientExt, StorageProviderClientExt, SystemClientExt,
};
use subxt::{ext::codec::Encode, tx::Signer};
use tokio::{
    sync::{
        mpsc::{error::SendError, UnboundedReceiver, UnboundedSender},
        Semaphore,
    },
    task::{JoinError, JoinHandle},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use types::{
    AddPieceMessage, PipelineMessage, PreCommitMessage, ProveCommitMessage,
    SubmitWindowedPoStMessage,
};

use crate::db::{DBError, DealDB};

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    PoRepError(#[from] PoRepError),
    #[error(transparent)]
    SectorError(#[from] SectorError),
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
    pub prove_commit_throttle: Arc<Semaphore>,
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
    let sector =
        UnsealedSector::create(state.server_info.seal_proof, sector_number, unsealed_path).await?;

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

    tracing::info!("Adding a piece...");
    sector
        .add_piece(deal_id, deal, piece_path, commitment)
        .await?;
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

    let Some(sector) = state.db.get_sector::<UnsealedSector>(sector_number)? else {
        tracing::error!("Tried to precommit non-existing sector");
        return Err(PipelineError::SectorNotFound);
    };

    let cache_dir_path = state.sealing_cache_dir.join(sector_number.to_string());
    let sealed_path = state.sealed_sectors_dir.join(sector_number.to_string());

    let sector = sector
        .pre_commit(
            state.xt_client.clone(),
            &state.xt_keypair,
            cache_dir_path,
            sealed_path,
        )
        .await?;

    state.db.save_sector(sector.sector_number, &sector)?;

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
    let Some(sector) = state.db.get_sector::<PreCommittedSector>(sector_number)? else {
        tracing::error!("Tried to precommit non-existing sector");
        return Err(PipelineError::SectorNotFound);
    };

    let sector = sector
        .prove_commit(
            state.xt_client.clone(),
            &state.xt_keypair,
            state.porep_parameters.clone(),
            state.prove_commit_throttle.clone(),
            token,
        )
        .await?;

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

    if deadline_state.partitions.len() == 0 {
        tracing::info!("There are not partitions in this deadline yet. Nothing to prove here.");
        schedule_post(state, deadline_index)?;
        return Ok(());
    }

    let partitions = deadline_state.partitions.keys().cloned().collect();
    let all_sectors = BTreeSet::from_iter(
        deadline_state
            .partitions
            .into_iter()
            .flat_map(|(_, PartitionState { sectors })| sectors),
    );

    if all_sectors.len() == 0 {
        tracing::info!("Every sector expired... Nothing to prove here.");
        schedule_post(state, deadline_index)?;
        return Ok(());
    }

    let mut replicas = Vec::new();
    for sector_number in all_sectors {
        let sector = state
            .db
            .get_sector::<ProvenSector>(sector_number)?
            .ok_or(PipelineError::SectorNotFound)?;

        replicas.push(ReplicaInfo {
            sector_id: sector_number,
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
    tracing::info!("Generated PoSt proof for partitions: {:?}", partitions);

    let proofs = proofs
        .into_iter()
        .map(|p| PoStProof {
            post_proof: state.server_info.post_proof,
            proof_bytes: codec::Encode::encode(
                &TryInto::<SubstrateProof>::try_into(p.clone())
                    .expect("converstion between rust-fil-proofs and polka-storage-proofs to work"),
            ),
        })
        .collect::<Vec<_>>();

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
                partitions,
                proofs,
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
