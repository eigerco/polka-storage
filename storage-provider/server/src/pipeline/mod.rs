pub mod types;

use std::{path::PathBuf, sync::Arc};

use polka_storage_proofs::{
    porep::PoRepParameters,
    post::{PoStError, PoStParameters},
};
use polka_storage_provider_common::{
    deadline::Deadline,
    rpc::ServerInfo,
    sector::{PreCommittedSector, ProvenSector, SectorError, UnsealedSector},
};
use primitives::{
    commitment::{CommP, Commitment},
    sector::SectorNumber,
};
use storagext::{types::market::DealProposal, StorageProviderClientExt};
use tokio::sync::{
    mpsc::{error::SendError, UnboundedReceiver, UnboundedSender},
    Mutex, Semaphore,
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::Instrument;
use types::{
    AddPieceMessage, PipelineMessage, PreCommitMessage, ProveCommitMessage,
    SubmitWindowedPoStMessage,
};

use crate::db::{DBError, DealDB};

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
                // AddPiece is cancellation safe, as it can be retried and the state will be fine.
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
        PipelineMessage::AddPiece(msg) => {
            tracker.add_piece(state.clone(), msg, token.clone());
        }
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

/// Finds or creates a sector for the given piece. Returns the [`UnsealedSector`] and a `bool`
/// indicating if the sector was created or not.
///
/// * If no sectors exist, it creates one and returns it.
/// * If no sectors with enough size to harbor the piece exist, it creates one and returns it.
/// * If a sector with enough size to harbor the piece exists, it returns it.
#[tracing::instrument(skip_all)]
async fn find_or_create_sector_for_piece(
    state: &Arc<PipelineState>,
    deal: &DealProposal, // We pass the DealProposal instead of the sector number for better logs
) -> Result<(UnsealedSector, bool), PipelineError> {
    tracing::debug!(
        "Searching for sector for piece with size: {}",
        deal.piece_size
    );
    // Find the first sector with enough space for the piece
    let sector = state.db.iter_unsealed_sectors().find(|res| match res {
        Ok(unsealed_sector) => {
            let free_space = unsealed_sector.free_space();
            tracing::debug!(sector_number = %unsealed_sector.sector_number, free_space = free_space, "Checking sector...");
            free_space > deal.piece_size
        },
        // Errors return false since, well, they're not valid sectors
        _ => false,
    });

    // If we found a sector with space, we return it, otherwise, we'll create a new one
    // NOTE(@jmg-duarte,03/02/2025): we can't keep creating sectors forever just because they dont fit
    // (or maybe we can) but FC keeps a limit on new sectors, I don't have a solution for this NOW
    // but we can keep this here while this implementation develops
    // (possible) SOLUTION: pre-check sector availability when receiving deals and decide there
    // whether we're taking the deal or not
    if let Some(sector) = sector {
        // NOTE(@jmg-duarte,03/02/2025): as per our filter, errors return false, as such an error couldn't be returned
        return sector
            .map(|sector| {
                tracing::debug!(
                    sector_number = %sector.sector_number,
                    "Found sector for piece!",
                );
                (sector, false)
            })
            .map_err(PipelineError::from);
    }

    // NOTE(@jmg-duarte,03/02/2025): comment below no longer applies but im keeping it until
    // we have a full implementation in place
    // TODO(@th7nder,30/10/2024): simplification, we're always creating a new sector for storing a piece.
    // It should not work like that, sectors should be filled with pieces according to *some* algorithm.
    let sector_number = state
        .db
        .next_sector_number()
        .map_err(|err| PipelineError::CustomError(err.to_string()))?;
    tracing::debug!(%sector_number, "Could not find a sector for piece, creating a new one...");

    let unsealed_path = state.unsealed_sectors_dir.join(sector_number.to_string());
    let sector =
        UnsealedSector::create(state.server_info.seal_proof, sector_number, unsealed_path).await?;

    Ok((sector, true))
}

/// Finds a sector to which a piece will fit and adds it to the sector.
/// This function is *cancellation safe* as if future is dropped,
/// it can be dropped only when waiting for `spawn_blocking`.
/// When dropped when waiting, the sector state won't be preserved and adding piece can be retried.
#[tracing::instrument(skip(state, deal, commitment))]
async fn add_piece(
    tracker: TaskTracker,
    state: Arc<PipelineState>,
    piece_path: PathBuf,
    commitment: Commitment<CommP>,
    deal: DealProposal,
    deal_id: u64,
) -> Result<(), PipelineError> {
    // This guard MUST be scoped to the entire add_piece logic!
    // Otherwise, depending on the fill percentage it can happen that two tasks try to send
    // pre_commit messages that would later result in processing issues.
    // Given a fill_threshold that is low enough, without a lock on this entire method,
    // if two pieces are added past the fill threshold, two pre commit messages will be sent
    // which will then race and create issues
    let _guard = state.add_piece_serializer.lock().await;

    tracing::info!("Adding a piece...");
    let (mut sector, created) = find_or_create_sector_for_piece(&state, &deal).await?;

    sector
        .add_piece(deal_id, deal, piece_path, commitment)
        .await?;
    tracing::info!("Finished adding a piece");

    // Update the database with the latest sector information
    state
        .db
        .insert_unsealed_sector(sector.sector_number, &sector)?;

    let fill_percentage = sector.fill_percentage();
    let fill_threshold = state.server_info.sealing_configuration.fill_threshold as u64;
    if fill_percentage > fill_threshold {
        tracing::debug!(
            "Occupation level at {}%, above limit of {}% - pre-committing",
            fill_percentage,
            fill_threshold,
        );
        // TODO(@th7nder,30/10/2024): simplification, as we're always scheduling a precommit just after adding a piece and creating a new sector.
        // Ideally sector won't be finalized after one piece has been added and the precommit will depend on the start_block?
        return Ok(state
            .pipeline_sender
            .send(PipelineMessage::PreCommit(PreCommitMessage {
                sector_number: sector.sector_number,
            }))?);
    }
    tracing::debug!(
        sector_number = %sector.sector_number,
        "Occupation at {}; not pre-committing yet",
        fill_percentage
    );

    // If the sector is new, we schedule it's pre-commit task submission
    if created {
        schedule_pre_commit(state.clone(), tracker, sector);
    }

    Ok(())
}

fn schedule_pre_commit(state: Arc<PipelineState>, tracker: TaskTracker, sector: UnsealedSector) {
    let delay = state.server_info.sealing_configuration.wait_deals_delay;
    let span = tracing::info_span!("add_piece");
    tracing::info!(
        // Since the span is moved for the instrument call, we can't span.enter()
        // doing a span.in_scope(|| ...) is also a bit overkill for a log
        parent: &span,
        "Sector was created, launching task to pre-commit in {} seconds",
        delay.as_secs()
    );
    // We don't care for the returned JoinHandle, but that's ok because the TaskTracker has it!
    let _ = tracker.spawn(
        async move {
            tokio::time::sleep(delay).await;
            let sector_number = sector.sector_number;
            tracing::info!(%sector_number, "Awoken from sleep, submitting pre-commit task!");
            match state
                .pipeline_sender
                .send(PipelineMessage::pre_commit(sector_number))
            {
                Ok(()) => tracing::info!(%sector_number, "Successfully submitted task!"),
                // Not sure what we can do if the task fails, maybe try to re-submit?
                // Not even sure when this can happen asides from an OOM since we're using an unbounded channel
                Err(err) => tracing::error!(%sector_number, "Failed to submit task: {}", err),
            }
        }
        .instrument(span),
    );
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

    // This unit of work effectively works as a "block", since `remove_unsealed_sector`
    // blocks the row it removes, meaning that even if two tasks race here,
    // the DB will stop one from doing an outdated read
    let state_for_task = state.clone();
    let sector = tokio::task::spawn_blocking(move || {
        match state_for_task.db.remove_unsealed_sector(sector_number)? {
            Some(sector) => return Ok(sector),
            None => {
                // This is a partial error since the sector may *just* have been pre-committed
                tracing::warn!(%sector_number, "Tried to precommit non-existing unsealed sector");
                return Err(PipelineError::SectorNotFound);
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
