mod add_piece;
pub mod types;

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use chrono::{DateTime, Utc};
use polka_storage_proofs::{
    porep::PoRepParameters,
    post::{PoStError, PoStParameters},
};
use polka_storage_provider_common::{
    deadline::Deadline,
    rpc::ServerInfo,
    sector::{PreCommittedSector, ProvenSector, SectorError},
};
use primitives::sector::SectorNumber;
use storagext::StorageProviderClientExt;
use tokio::sync::{
    mpsc::{error::SendError, UnboundedReceiver, UnboundedSender},
    oneshot, Mutex, Semaphore,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{error, warn};
use types::{
    AddPieceMessage, PipelineMessage, PreCommitMessage, ProveCommitMessage,
    SubmitWindowedPoStMessage,
};

use crate::{
    db::{DBError, DealDB},
    indexer::IndexerMessage,
    pipeline::add_piece::add_piece,
};

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    SectorError(#[from] SectorError),
    #[error(transparent)]
    PoStError(#[from] PoStError),
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    DBError(#[from] DBError),
    #[error("sector does not exist")]
    SectorNotFound,
    #[error(transparent)]
    SendError(#[from] SendError<PipelineMessage>),
    #[error("failed to schedule windowed PoSt")]
    SchedulingError,
    #[error("Custom error: {0}")]
    CustomError(String),
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
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

    pub indexer_tx: UnboundedSender<IndexerMessage>,

    // NOTE(@jmg-duarte,10/02/2025):
    // This is the wrong way of implementing serialization for `add_piece`!
    // However, the right way involves a major refactor :(
    //
    // To improve on this, `add_piece` needs it's own task and a message queue to ensure it
    // can't act on more than a single message at a time; instead of a new task being spawned for
    // each incoming add_piece request.
    //
    // To have multiple add_piece running concurrently, we need to make use of RocksDB's
    // transactions + get_for_update(exclusive: true)
    // This is not trivial and requires a refactor on the DB side!
    pub add_piece_serializer: Mutex<()>,

    // Store the estimated date of execution of the task and its abort handle for re-scheduling
    pub scheduled_pre_commits: Mutex<HashMap<SectorNumber, (DateTime<Utc>, oneshot::Sender<()>)>>,
}

impl PipelineState {
    /// Sends a [`PreCommitMessage`](crate::pipeline::types::PreCommitMessage) to the pipeline.
    fn send_pre_commit(&self, sector_number: SectorNumber) -> Result<(), PipelineError> {
        Ok(self
            .pipeline_sender
            .send(PipelineMessage::pre_commit(sector_number))?)
    }
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
        let tracker = self.clone();
        self.spawn(async move {
            tokio::select! {
                // AddPiece is NOT cancellation safe, cancelling it will make the program state inconsistent.
                res = add_piece(tracker, state, piece_path, commitment, deal, published_deal_id) => {
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

    fn precommit(
        &self,
        state: Arc<PipelineState>,
        PreCommitMessage { sector_number }: PreCommitMessage,
    ) {
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
        let indexer_tx = state.indexer_tx.clone();

        let ProveCommitMessage { sector_number } = msg;
        self.spawn(async move {
            match prove_commit(state, sector_number, token).await {
                Ok(sector) => {
                    tracing::info!(
                        "ProveCommit for sector {} finished successfully.",
                        sector_number
                    );

                    // Start indexing the sector
                    if let Err(err) = indexer_tx.send(IndexerMessage::IndexSector(sector)) {
                        error!(?err, "error occurred while messaging the indexer");
                    }
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
        PipelineMessage::AddPiece(msg) => {
            tracker.add_piece(state.clone(), msg, token.child_token());
        }
        PipelineMessage::PreCommit(msg) => tracker.precommit(state.clone(), msg),
        PipelineMessage::ProveCommit(msg) => {
            tracker.prove_commit(state.clone(), msg, token.child_token())
        }
        PipelineMessage::SubmitWindowedPoStMessage(msg) => {
            tracker.submit_windowed_post(state.clone(), msg, token.child_token())
        }
        PipelineMessage::SchedulePoSts => tracker.schedule_posts(state.clone()),
    }
}

/// Creates a replica and calls pre-commit on-chain.
///
/// This method is *NOT CANCELLATION SAFE*.
/// When interrupted while waiting for the extrinsic call to return,
/// the Storage Provider is not consistent of the on-chain state,
/// cancelling this task effectively breaks the state sync.
#[tracing::instrument(skip(state))]
async fn precommit(
    state: Arc<PipelineState>,
    sector_number: SectorNumber,
) -> Result<(), PipelineError> {
    tracing::info!("Starting pre-commit");

    // While this scope is executing, we know that no pieces are being added to
    // the sectors. We know that because of the locked `add_piece_serializer`.
    // While we hold the lock, we move the sector to the sealing pending. That
    // is needed, so that after the lock is dropped. The new pieces being added
    // wont consider the current sector as viable.
    {
        let _lock = state.add_piece_serializer.lock().await;

        let Some(sector) = state.db.remove_unsealed_sector(sector_number)? else {
            tracing::warn!(%sector_number, "Tried to precommit non-existing unsealed sector");
            return Err(PipelineError::SectorNotFound);
        };

        state.db.insert_pending_sealing_sector(&sector)?;
    }

    {
        // We remove ourselves from the scheduled pre-commits
        let mut scheduled_pre_commits = state.scheduled_pre_commits.lock().await;
        if scheduled_pre_commits.remove(&sector_number).is_none() {
            tracing::warn!(%sector_number, "No task was found! Skipping pre-commiting as sector should have been pre-commited before.");
            return Ok(());
        }
    }

    // This unit of work effectively works as a "block", since `remove_pending_sealing_sector`
    // blocks the row it removes, meaning that even if two tasks race here,
    // the DB will stop one from doing an outdated read
    let state_for_task = state.clone();
    let sector = tokio::task::spawn_blocking(move || {
        match state_for_task
            .db
            .remove_pending_sealing_sector(sector_number)?
        {
            Some(sector) => Ok(sector),
            None => {
                // This is a partial error since the sector may *just* have been pre-committed
                tracing::warn!(%sector_number, "Tried to precommit non-existing unsealed sector");
                Err(PipelineError::SectorNotFound)
            }
        }
    })
    .await??;

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
) -> Result<ProvenSector, PipelineError> {
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

    Ok(sector)
}

#[tracing::instrument(skip(state))]
async fn submit_windowed_post(
    state: Arc<PipelineState>,
    deadline_index: u64,
) -> Result<(), PipelineError> {
    let deadline = Deadline::new(deadline_index, state.server_info.post_proof);
    let sector_storage = |sector_number| match state.db.get_sector::<ProvenSector>(sector_number) {
        Ok(sector) => sector,
        Err(e) => {
            tracing::error!("failed to get sector: {}", e);
            None
        }
    };

    if let Err(e) = deadline
        .submit_windowed_post(
            state.xt_client.clone(),
            &state.xt_keypair,
            state.post_parameters.clone(),
            sector_storage,
        )
        .await
    {
        tracing::error!("failed to submit post for deadline, {}", e);
    } else {
        tracing::info!("completed post submission");
    }

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
