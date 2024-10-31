pub mod types;

use std::{path::PathBuf, sync::Arc};

use polka_storage_proofs::porep::{
    sealer::{prepare_piece, PreCommitOutput, Sealer},
    PoRepError,
};
use polka_storage_provider_common::rpc::ServerInfo;
use primitives_commitment::Commitment;
use primitives_proofs::{derive_prover_id, SectorNumber};
use storagext::{
    types::{market::DealProposal, storage_provider::SectorPreCommitInfo},
    StorageProviderClientExt, SystemClientExt,
};
use subxt::tx::Signer;
use tokio::{
    sync::mpsc::{error::SendError, UnboundedReceiver, UnboundedSender},
    task::{JoinError, JoinHandle},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::{
    db::{DBError, DealDB},
    pipeline::types::SectorState,
};
use types::{AddPieceMessage, PipelineMessage, PreCommitMessage};

use self::types::Sector;

// PLACEHOLDERS!!!!!
// TODO(@th7nder,29/10/2024): get from pallet randomness
const TICKET: [u8; 32] = [12u8; 32];
// const SEED: [u8; 32] = [13u8; 32];
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
    NotExistentSector,
    #[error(transparent)]
    SendError(#[from] SendError<PipelineMessage>),
}
/// Pipeline shared state.
pub struct PipelineState {
    pub server_info: ServerInfo,
    pub db: Arc<DealDB>,
    pub unsealed_sectors_dir: Arc<PathBuf>,
    pub sealed_sectors_dir: Arc<PathBuf>,
    pub sealing_cache_dir: Arc<PathBuf>,

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

fn process(
    tracker: &TaskTracker,
    msg: PipelineMessage,
    state: Arc<PipelineState>,
    token: CancellationToken,
) {
    match msg {
        PipelineMessage::AddPiece(AddPieceMessage {
            deal,
            published_deal_id,
            piece_path,
            piece_cid,
        }) => {
            tracker.spawn(async move {
                tokio::select! {
                    // AddPiece is cancellation safe, as it can be retried and the state will be fine.
                    res = add_piece(state, piece_path, piece_cid, deal, published_deal_id) => {
                        match res {
                            Ok(_) => tracing::info!("Add Piece for piece {:?}, deal id {}, finished successfully.", piece_cid, published_deal_id),
                            Err(err) => tracing::error!(%err, "Add Piece for piece {:?}, deal id {}, failed!", piece_cid, published_deal_id),
                        }
                    },
                    () = token.cancelled() => {
                        tracing::warn!("AddPiece has been cancelled.");
                    }
                }
            });
        }
        PipelineMessage::PreCommit(PreCommitMessage {
            sector_number: sector_id,
        }) => {
            tracker.spawn(async move {
                // Precommit is not cancellation safe.
                match precommit(state, sector_id).await {
                    Ok(_) => tracing::info!("Precommit for sector {} finished successfully.", sector_id),
                    Err(err) => tracing::error!(%err),
                }
            });
        }
    }
}

async fn find_sector_for_piece(state: &Arc<PipelineState>) -> Result<Sector, PipelineError> {
    // TODO(@th7nder,30/10/2024): simplification, we're always creating a new sector for storing a piece.
    // It should not work like that, sectors should be filled with pieces according to *some* algorithm.
    let sector_number = state.db.next_sector_number();
    let unsealed_path = state.unsealed_sectors_dir.join(sector_number.to_string());
    let sealed_path = state.sealed_sectors_dir.join(sector_number.to_string());
    let sector = Sector::create(sector_number, unsealed_path, sealed_path).await?;

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
    piece_cid: Commitment,
    deal: DealProposal,
    deal_id: u64,
) -> Result<(), PipelineError> {
    let mut sector = find_sector_for_piece(&state).await?;
    sector.deals.push((deal_id, deal));

    let sealer = Sealer::new(state.server_info.seal_proof);
    let handle: JoinHandle<Result<Sector, PipelineError>> =
        tokio::task::spawn_blocking(move || {
            let unsealed_sector = std::fs::File::open(&sector.unsealed_path)?;

            let (padded_reader, piece_info) = prepare_piece(piece_path, piece_cid)?;
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
    let sector: Sector = handle.await??;
    state.db.save_sector(&sector)?;

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
/// This is *NOT CANCELLATION SAFE*.
/// I.e. when interrupted when waiting for extrinsic call to return, the state on-chain will be inconsistent with state in Storage Provider.
async fn precommit(
    state: Arc<PipelineState>,
    sector_number: SectorNumber,
) -> Result<(), PipelineError> {
    tracing::info!("Starting pre-commit");

    let sealer = Sealer::new(state.server_info.seal_proof);
    let Some(mut sector) = state.db.get_sector(sector_number)? else {
        tracing::error!("Tried to precommit non-existing sector");
        return Err(PipelineError::NotExistentSector);
    };
    // Pad sector so CommD can be properly calculated.
    sector.piece_infos = sealer.pad_sector(&sector.piece_infos, sector.occupied_sector_space)?;

    tracing::debug!("Padded sector, commencing pre-commit.");
    // TODO(@th7nder,31/10/2024): what happens if some of the process fails? SP will be slashed, and there is no error reporting? what about retries?
    let sealing_handle: JoinHandle<Result<PreCommitOutput, _>> = {
        let prover_id = derive_prover_id(state.xt_keypair.account_id());
        let cache_dir = state.sealing_cache_dir.clone();
        let unsealed_path = sector.unsealed_path.clone();
        let sealed_path = sector.sealed_path.clone();
        let piece_infos = sector.piece_infos.clone();
        tokio::task::spawn_blocking(move || {
            sealer.precommit_sector(
                cache_dir.as_ref(),
                unsealed_path,
                sealed_path,
                prover_id,
                sector_number,
                TICKET,
                &piece_infos,
            )
        })
    };
    let sealing_output = sealing_handle.await??;
    tracing::info!("Created sector's replica: {:?}", sealing_output);

    let current_block = state.xt_client.height(false).await?;
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
                    + 10,
                sector_number: sector_number,
                seal_proof: state.server_info.seal_proof,
                sealed_cid: primitives_commitment::Commitment::new(
                    sealing_output.comm_r,
                    primitives_commitment::CommitmentKind::Replica,
                )
                .cid(),
                unsealed_cid: primitives_commitment::Commitment::new(
                    sealing_output.comm_d,
                    primitives_commitment::CommitmentKind::Data,
                )
                .cid(),
                seal_randomness_height: current_block,
            }],
        )
        .await?;

    sector.state = SectorState::Sealed;
    state.db.save_sector(&sector)?;

    let precommited_sectors = result
        .events
        .find::<storagext::runtime::storage_provider::events::SectorsPreCommitted>()
        .collect::<Result<Vec<_>, _>>()?;

    tracing::info!(
        "Successfully pre-commited sectors on-chain: {:?}",
        precommited_sectors
    );

    Ok(())
}
