use std::{path::PathBuf, sync::Arc};

use cid::Cid;
use polka_storage_proofs::porep::{
    sealer::{prepare_piece, PreCommitOutput, Sealer},
    PoRepError,
};
use polka_storage_provider_common::rpc::ServerInfo;
use primitives_proofs::{derive_prover_id, RegisteredSealProof, SectorNumber};
use storagext::{
    types::{market::ClientDealProposal, storage_provider::SectorPreCommitInfo},
    StorageProviderClientExt, SystemClientExt,
};
use subxt::tx::Signer;
use tokio::{
    sync::mpsc::UnboundedReceiver,
    task::{JoinError, JoinHandle},
};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

// PLACEHOLDERS!!!!!
// TODO(@th7nder,29/10/2024): #474
const SECTOR_ID: u64 = 77;
// TODO(@th7nder,29/10/2024): get from pallet randomness
const TICKET: [u8; 32] = [12u8; 32];
// const SEED: [u8; 32] = [13u8; 32];
const SECTOR_EXPIRATION_MARGIN: u64 = 20;

#[derive(Debug)]
pub struct PreCommitMessage {
    pub deal: ClientDealProposal,
    pub published_deal_id: u64,
    pub piece_path: PathBuf,
    pub piece_cid: Cid,
}

#[derive(Debug)]
pub enum PipelineMessage {
    PreCommit(PreCommitMessage),
}

#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    #[error(transparent)]
    PoRepError(#[from] PoRepError),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error("Pre-commit took too much time... Cannot start pre-commit now, current_block: {0}, deal_start: {1}")]
    SealingTooSlow(u64, u64),
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
}
/// Pipeline shared state.
pub struct PipelineState {
    pub server_info: ServerInfo,
    pub unsealed_sectors_dir: Arc<PathBuf>,
    pub sealed_sectors_dir: Arc<PathBuf>,
    pub sealing_cache_dir: Arc<PathBuf>,

    pub xt_client: Arc<storagext::Client>,
    pub xt_keypair: storagext::multipair::MultiPairSigner,
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
        PipelineMessage::PreCommit(PreCommitMessage {
            deal,
            published_deal_id,
            piece_path,
            piece_cid,
        }) => {
            tracker.spawn(async move {
                tokio::select! {
                    res = precommit(state, deal, published_deal_id, SECTOR_ID, piece_path, piece_cid, token.clone()) => {
                        match res {
                            Ok(_) => tracing::info!("Precommit finished successfully."),
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

async fn precommit(
    state: Arc<PipelineState>,
    deal: ClientDealProposal,
    deal_id: u64,
    sector_id: SectorNumber,
    piece_path: PathBuf,
    piece_cid: Cid,
    token: CancellationToken,
) -> Result<(), PipelineError> {
    tracing::debug!(
        "Starting pre-commit task for deal {}, sector, {}",
        deal_id,
        sector_id
    );
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
                piece_path,
                state.server_info.seal_proof,
                piece_cid,
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

    let deal_start = deal.deal_proposal.start_block;
    let deal_duration = deal.deal_proposal.end_block - deal_start;
    let current_block = state.xt_client.height(false).await?;
    if current_block > deal_start {
        tracing::error!(
            "Cannot pre-commit sector, sealing was too slow: current_block: {}, deal_start: {}",
            current_block,
            deal_start
        );
        return Err(PipelineError::SealingTooSlow(current_block, deal_start));
    }

    let result = state
        .xt_client
        .pre_commit_sectors(
            &state.xt_keypair,
            vec![SectorPreCommitInfo {
                deal_ids: vec![deal_id],
                expiration: deal_start + deal_duration + SECTOR_EXPIRATION_MARGIN,
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
    unsealed_dir: Arc<PathBuf>,
    sealed_dir: Arc<PathBuf>,
    cache_dir: Arc<PathBuf>,
    piece_path: PathBuf,
    seal_proof: RegisteredSealProof,
    piece_cid: Cid,
) -> Result<PreCommitOutput, polka_storage_proofs::porep::PoRepError> {
    let piece_commitment: [u8; 32] = piece_cid
        .hash()
        .digest()
        .try_into()
        .expect("piece_cid should have been validated on proposal");

    let unsealed_sector_path = unsealed_dir.join(sector_id.to_string());
    let sealed_sector_path = {
        let path = sealed_dir.join(sector_id.to_string());
        // We need to create the file ourselves, even though that's not documented
        std::fs::File::create(&path)?;
        path
    };

    let sealer = Sealer::new(seal_proof);
    let prepared_piece = prepare_piece(piece_path, piece_commitment)?;
    let piece_infos = {
        // The scope creates an implicit drop of the file handler
        // avoiding reading issues later on
        let sector_writer = std::fs::File::create(&unsealed_sector_path)?;
        sealer.create_sector(vec![prepared_piece], sector_writer)?
    };

    sealer.precommit_sector(
        cache_dir.as_ref(),
        unsealed_sector_path,
        sealed_sector_path,
        prover_id,
        SECTOR_ID,
        TICKET,
        &piece_infos,
    )
}
