pub mod types;

use std::{path::PathBuf, sync::Arc};

use polka_storage_proofs::{
    porep::PoRepParameters,
    post::{PoStError, PoStParameters},
};
use polka_storage_provider_common::{
    deadline::Deadline,
    rpc::ServerInfo,
    sector::{PreCommittedSector, SectorError, UnsealedSector},
};
use primitives::{
    commitment::{CommP, Commitment},
    sector::SectorNumber,
};
use storagext::{types::market::DealProposal, StorageProviderClientExt, SystemClientExt};
use tokio::sync::{
    mpsc::{error::SendError, UnboundedReceiver, UnboundedSender},
    Semaphore,
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

    // Ideally, these two are queried from the chain, however, subxt ships them based on the runtime
    // as such, beware what runtime you compiled subxt for!
    #[allow(unused)]
    // unused for now, useful later when figuring out the proper bounds between deals
    // namely, for PreCommitBatchSlack in the Sealing configuration
    // reference: https://lotus.filecoin.io/storage-providers/advanced-configurations/sealing/
    pub min_sector_expiration: u64,
    pub max_sector_expiration: u64,
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

    let current_block = state.xt_client.height(true).await?;
    // When adding a piece/deal to a sector, we must ensure the sector remains valid
    // i.e. no invariants are broken; as such we must ensure that the deal being added
    // does not expire beyond the maximum sector expiration.
    //
    // NOTE(@jmg-duarte,31/01/2025): there's an hidden issue here that we can't address just now
    // the min/max sector expirations are moving targets, calculated from the current block
    // this means that we can only truly validate the invariants when submitting the pre-commit
    // Only when addressing issue #671 we will be able to fully solve this, since as soon as a deal
    // is added to a sector the clock starts ticking, if we wait too long the minimum expiration
    // may itself "expire".
    // The FC codebase doesn't really have any clues how this is solved, being probably left as an
    // "invisible" agreement between the client and SP that it just should work
    // The most useful piece of source is in:
    // https://github.com/filecoin-project/lotus/blob/a526c480d40898a079c806748639e8db07aa2298/storage/pipeline/input.go#L566
    if deal.end_block - current_block < state.max_sector_expiration {
        return Err(PipelineError::CustomError(
            "deal expires after the maximum sector expiration".to_string(),
        ));
    }

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
    let deadline = Deadline::new(deadline_index, state.server_info.post_proof);
    let sector_storage = |sector_number| match state.db.get_sector(sector_number) {
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
        tracing::info!("submitted post successfully");
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
