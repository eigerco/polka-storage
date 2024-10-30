pub mod types;

use std::{path::PathBuf, sync::Arc};

use cid::Cid;
use polka_storage_proofs::porep::{
    sealer::{prepare_piece, PreCommitOutput, Sealer},
    PoRepError,
};
use polka_storage_provider_common::rpc::ServerInfo;
use primitives_proofs::{derive_prover_id, RawCommitment, RegisteredSealProof, SectorNumber};
use storagext::{
    types::{market::DealProposal, storage_provider::SectorPreCommitInfo},
    StorageProviderClientExt, SystemClientExt,
};
use subxt::tx::Signer;
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, error::SendError},
    task::{JoinError, JoinHandle},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::db::{DBError, DealDB};
use types::{AddPieceMessage, PipelineMessage, PreCommitMessage};

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
    SendError(#[from] SendError<PipelineMessage>)
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
                    res = add_piece(state, piece_path, piece_cid, deal, published_deal_id, token.clone()) => {
                        match res {
                            Ok(_) => tracing::info!("Add Piece for piece {}, deal id {}, finished successfully.", piece_cid, published_deal_id),
                            Err(err) => tracing::error!(%err),
                        }
                    },
                    () = token.cancelled() => {
                        tracing::warn!("AddPiece has been cancelled.");
                    }
                }
            });
        }
        PipelineMessage::PreCommit(PreCommitMessage { sector_id }) => {
            tracker.spawn(async move {
                tokio::select! {
                    res = precommit(state, sector_id, token.clone()) => {
                        match res {
                            Ok(_) => tracing::info!("Precommit for sector {} finished successfully.", sector_id),
                            Err(err) => tracing::error!(%err),
                        }
                    },
                    () = token.cancelled() => {
                        tracing::warn!("PreCommit has been cancelled.");
                    }
                }
            });
        }
    }
}

#[tracing::instrument(skip_all, fields(piece_cid, deal_id))]
async fn add_piece(
    state: Arc<PipelineState>,
    piece_path: PathBuf,
    piece_cid: Cid,
    deal: DealProposal,
    deal_id: u64,
    token: CancellationToken,
) -> Result<(), PipelineError> {
    // TODO(@th7nder,30/10/2024): simplification, we're always creating a new sector for storing a piece.
    // It should not work like that, sectors should be filled with pieces according to *some* algorithm.
    let mut sector = state.db.create_new_sector();
    sector.deals.push((deal_id, deal));


    let piece_commitment: RawCommitment = piece_cid
        .hash()
        .digest()
        .try_into()
        .expect("piece_cid should have been validated on proposal");

    // TODO: tokio::spawn_blocking
    let (padded_reader, piece_info) = prepare_piece(piece_path, piece_commitment)?;

    state.db.save_sector(&sector)?;
    // TODO(@th7nder,30/10/2024): simplification, as we're always scheduling a precommit just after adding a piece and creating a new sector.
    // Ideally sector won't be finalized after one piece has been added and the precommit will depend on the start_block?
    state.pipeline_sender.send(PipelineMessage::PreCommit(PreCommitMessage {
        sector_id: sector.id(),
    }))?;

    Ok(())
}

#[tracing::instrument(skip_all, fields(sector_id))]
async fn precommit(
    state: Arc<PipelineState>,
    sector_id: SectorNumber,
    token: CancellationToken,
) -> Result<(), PipelineError> {
    tracing::debug!("Starting pre-commit task for sector {}", sector_id);

    let Some(sector) = state.db.get_sector(sector_id)? else {
        tracing::error!("tried to precommit non-existing sector");
        return Err(PipelineError::NotExistentSector);
    };

    // TODO(@th7nder,30/10/2024):
    // - get all of the piece infos which were saved in the sector
    // - pad it finally
    // - start sealing

    // Questions to be answered:
    // * what happens if some of it fails? SP will be slashed, and there is no error reporting?
    // * where do we save the state of a sector/deals, how do we keep track of it?
    let sealing_handle: JoinHandle<Result<PreCommitOutput, _>> = {
        let state = state.clone();
        let prover_id = derive_prover_id(state.xt_keypair.account_id());
        let cache_dir = state.sealing_cache_dir.clone();
        let unsealed_dir = state.sealed_sectors_dir.clone();
        let sealed_dir = state.sealed_sectors_dir.clone();
        tokio::task::spawn_blocking(move || {
            create_replica(
                sector_id,
                prover_id,
                unsealed_dir,
                sealed_dir,
                cache_dir,
                state.server_info.seal_proof,
            )
        })
    };

    let sealing_output = tokio::select! {
        res = sealing_handle => {
            res??
        },
        _ = token.cancelled() => {
            tracing::warn!("Cancelled sealing process...");
            return Ok(())
        }
    };
    tracing::info!("Created sector's replica: {:?}", sealing_output);

    let result = state
        .xt_client
        .pre_commit_sectors(
            &state.xt_keypair,
            vec![SectorPreCommitInfo {
                deal_ids: sector.deals.into_iter().map(|(id, _)| id).collect(),
                expiration: 1000,
                sector_number: sector_id,
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
                // TODO(@th7nder,30/10/2024): xxx
                seal_randomness_height: 0,
            }],
        )
        .await?;

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

fn create_replica(
    sector_id: SectorNumber,
    prover_id: [u8; 32],
    unsealed_sector_path: Arc<PathBuf>,
    sealed_sector_path: Arc<PathBuf>,
    cache_dir: Arc<PathBuf>,
    seal_proof: RegisteredSealProof,
) -> Result<PreCommitOutput, polka_storage_proofs::porep::PoRepError> {
    // let unsealed_sector_path = unsealed_dir.join(sector_id.to_string());
    // let sealer = Sealer::new(seal_proof);
    //
    // let piece_infos = {
    //     // The scope creates an implicit drop of the file handler
    //     // avoiding reading issues later on
    //     let sector_writer = std::fs::File::create(&unsealed_sector_path)?;
    //     sealer.create_sector(vec![prepared_piece], sector_writer)?
    // };

    // let sealed_sector_path = {
    //     let path = sealed_dir.join(sector_id.to_string());
    //     // We need to create the file ourselves, even though that's not documented
    //     std::fs::File::create(&path)?;
    //     path
    // }

    // let sealer = Sealer::new(seal_proof);
    // sealer.precommit_sector(
    //     *cache_dir,
    //     *unsealed_sector_path,
    //     *sealed_sector_path,
    //     prover_id,
    //     sector_id,
    //     TICKET,
    //     &piece_infos,
    // )

    todo!()
}
