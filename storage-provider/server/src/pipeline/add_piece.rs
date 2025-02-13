//! Module containing the implementation for the add piece phase.

use std::{path::PathBuf, sync::Arc, time::Duration};

use chrono::Utc;
use polka_storage_provider_common::sector::UnsealedSector;
use primitives::{
    commitment::{CommP, Commitment},
    sector::SectorNumber,
};
use storagext::{types::market::DealProposal, SystemClientExt};
use tokio::sync::oneshot;
use tokio_util::task::TaskTracker;
use tracing::Instrument;

use crate::{
    pipeline::{PipelineError, PipelineState},
    rpc::SECS_PER_BLOCK,
};

/// Finds a sector to which a piece will fit and adds it to the sector.
/// This function is *cancellation safe* as if future is dropped,
/// it can be dropped only when waiting for `spawn_blocking`.
/// When dropped when waiting, the sector state won't be preserved and adding piece can be retried.
#[tracing::instrument(skip(tracker, state, deal, commitment))]
pub async fn add_piece(
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

    let deal_start_block = deal.start_block;

    tracing::info!("Adding a piece...");
    let mut sector = find_or_create_sector_for_piece(&state, &deal).await?;
    sector
        .add_piece(deal_id, deal, piece_path, commitment)
        .await?;
    let sector_number = sector.sector_number;
    tracing::info!(%sector_number, "Finished adding a piece");

    // Update the database with the latest sector information
    state.db.insert_unsealed_sector(sector_number, &sector)?;

    let fill_percentage = sector.fill_percentage();
    let fill_threshold = state.server_info.sealing_configuration.fill_threshold as u64;
    if fill_percentage >= fill_threshold {
        tracing::debug!(
            %sector_number,
            "Occupation level at {}%, above limit of {}% - pre-committing",
            fill_percentage,
            fill_threshold,
        );
        // TODO(@th7nder,30/10/2024): simplification, as we're always scheduling a precommit just after adding a piece and creating a new sector.
        // Ideally sector won't be finalized after one piece has been added and the precommit will depend on the start_block?
        return state.send_pre_commit(sector_number);
    }
    tracing::debug!(
        %sector_number,
        "Occupation at {}; not pre-committing yet",
        fill_percentage
    );

    let current_block = state.xt_client.height(true).await?;
    let duration_to_deal_start =
        Duration::from_secs((deal_start_block - current_block) * SECS_PER_BLOCK);
    let when = std::cmp::min(
        state.server_info.sealing_configuration.wait_deals_delay,
        duration_to_deal_start,
    );
    // We always try to schedule a new pre-commit
    schedule_pre_commit(state.clone(), tracker, sector_number, when).await;

    Ok(())
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
) -> Result<UnsealedSector, PipelineError> {
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
            .inspect(|sector| {
                tracing::debug!(
                    sector_number = %sector.sector_number,
                    "Found sector for piece!",
                );
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

    Ok(sector)
}

/// Schedule a pre commit.
///
/// Calls to this function *MUST NOT* be made while holding a [`MutexGuard`] for `scheduled_pre_commits`.
///
/// If no pre-commit task has been previously scheduled, a new one is scheduled.
/// If an existing pre-commit task is scheduled to be executed *after* the new deadline,
/// that task will be cancelled and replaced with the new one.
/// Otherwise, this function is a no-op.
#[tracing::instrument(skip_all)]
async fn schedule_pre_commit(
    state: Arc<PipelineState>,
    tracker: TaskTracker,
    sector_number: SectorNumber,
    when: Duration,
) {
    let deadline = Utc::now() + when;
    let mut scheduled_pre_commits = state.scheduled_pre_commits.lock().await;
    match scheduled_pre_commits.get_mut(&sector_number) {
        Some((old_deadline, old_cancellation_sender)) if deadline < *old_deadline => {
            tracing::debug!(%sector_number, "Existing deadline is older than the new one, cancelling existing task: old_deadline = {}, new_deadline = {}", old_deadline, deadline);
            let state_for_task = state.clone();

            let (cancellation_sender, cancellation_receiver) = oneshot::channel();
            schedule_send_pre_commit(
                state_for_task,
                tracker,
                sector_number,
                when,
                cancellation_receiver,
            );

            *old_deadline = deadline;
            let old_abort_handle = std::mem::replace(old_cancellation_sender, cancellation_sender);
            if old_abort_handle.send(()).is_err() {
                tracing::error!("Failed to send cancellation value");
            }

            tracing::debug!(%sector_number, "Existing deadline & task have been replaced: new_deadline = {}", deadline);
        }
        Some((old_deadline, _)) => {
            tracing::debug!(%sector_number, "Existing deadline is newer than the new one: old_deadline = {}, new_deadline = {}", old_deadline, deadline);
        }
        None => {
            tracing::debug!(%sector_number, "No deadline existing existed, creating it.");
            let (cancellation_sender, cancellation_receiver) = oneshot::channel();
            let state_for_task = state.clone();
            schedule_send_pre_commit(
                state_for_task,
                tracker,
                sector_number,
                when,
                cancellation_receiver,
            );
            scheduled_pre_commits.insert(sector_number, (deadline, cancellation_sender));
        }
    }
}

/// *Unconditionally* schedule a pre commit.
fn schedule_send_pre_commit(
    state: Arc<PipelineState>,
    tracker: TaskTracker,
    sector_number: SectorNumber,
    when: Duration,
    cancellation_receiver: oneshot::Receiver<()>,
) {
    let span = tracing::info_span!("add_piece");
    tracing::info!(
        // Since the span is moved for the instrument call, we can't span.enter()
        // doing a span.in_scope(|| ...) is also a bit overkill for a log
        parent: &span,
        "Launching task to pre-commit in {} seconds",
        when.as_secs()
    );
    // We don't care for the returned JoinHandle, but that's ok because the TaskTracker has it!
    let _ = tracker.spawn(
        async move {
            match tokio::time::timeout(when, cancellation_receiver).await {
                Ok(Ok(())) => {
                    tracing::debug!(%sector_number, "Received cancellation signal, not sending message.");
                    return;
                },
                Ok(Err(err)) => {
                    tracing::error!(%sector_number, "Failed to receive message with error (will return): {err}");
                    return;
                },
                Err(_elapsed) => tracing::debug!(%sector_number, "No cancelation signal was received"),
            }

            tracing::info!(%sector_number, "Awoken from sleep, submitting pre-commit task!");
            match state.send_pre_commit(sector_number) {
                Ok(()) => tracing::info!(%sector_number, "Successfully submitted task!"),
                // Not sure what we can do if the task fails, maybe try to re-submit?
                // Not even sure when this can happen asides from an OOM since we're using an unbounded channel
                Err(err) => tracing::error!(%sector_number, "Failed to submit task: {}", err),
            }
        }
        .instrument(span),
    );
}
