use std::{path::PathBuf, sync::Arc};

use cid::Cid;
use polka_storage_proofs::porep::sealer::{prepare_piece, PreCommitOutput, Sealer};
use polka_storage_provider_common::rpc::ServerInfo;
use primitives_proofs::RegisteredSealProof;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio_util::sync::CancellationToken;

// PLACEHOLDERS!!!!!
const SECTOR_ID: u64 = 77;
const PROVER_ID: [u8; 32] = [0u8; 32];
const TICKET: [u8; 32] = [12u8; 32];
// const SEED: [u8; 32] = [13u8; 32];
const SECTOR_EXPIRATION_MARGIN: u64 = 20;

#[derive(Debug)]
pub enum PipelineMessage {
    PreCommit,
}

/// Pipeline shared state.
pub struct PipelineState {
    pub server_info: ServerInfo,
    pub unsealed_piece_storage_dir: Arc<PathBuf>,
    pub sealed_piece_storage_dir: Arc<PathBuf>,
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
    loop {
        tokio::select! {
            msg = receiver.recv() => {
                tracing::info!("Received msg: {:?}", msg);
                todo!();
            },
            _ = token.cancelled() => {
                tracing::info!("Pipeline has been stopped...");
                return Ok(())
            },
        }
    }
}

fn create_replica(
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

    let unsealed_sector_path = unsealed_dir.join(piece_cid.to_string());
    let sealed_sector_path = {
        let path = sealed_dir.join(piece_cid.to_string());
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
        PROVER_ID,
        SECTOR_ID,
        TICKET,
        &piece_infos,
    )
}

/*

        let deal_start = deal.deal_proposal.start_block;
        let deal_duration = deal.deal_proposal.end_block - deal_start;


        let sealing_result = sealing_result.await.map_err(|err| RpcError::internal_error(err, None))??;
        tracing::info!("Created sector's replica: {:?}", sealing_result);

        // Questions to be answered:
        // * what happens if some of it fails? SP will be slashed, and there is no error reporting?
        // * where do we save the state of a sector/deals, how do we keep track of it?
        let sealing_result: JoinHandle<Result<PreCommitOutput, RpcError>> =
            tokio::task::spawn_blocking(move || {

            });

        let current_block = self.xt_client.height(false).await?;
        if current_block > deal_start {
            return Err(RpcError::internal_error(format!("Pre-commit took too much time... Cannot start pre-commit now, current_block: {}, deal_start: {}", current_block, deal_start), None));
        }


        let result = self
            .xt_client
            .pre_commit_sectors(
                &self.xt_keypair,
                vec![SectorPreCommitInfo {
                    deal_ids: bounded_vec![deal_id],
                    expiration: deal_start + deal_duration + SECTOR_EXPIRATION_MARGIN,
                    sector_number: SECTOR_ID,
                    seal_proof,
                    sealed_cid: primitives_commitment::Commitment::new(
                        sealing_result.comm_r,
                        primitives_commitment::CommitmentKind::Replica,
                    )
                    .cid(),
                    unsealed_cid: primitives_commitment::Commitment::new(
                        sealing_result.comm_d,
                        primitives_commitment::CommitmentKind::Data,
                    )
                    .cid(),
                }],
            )
            .await?;

        let precommited_sectors = result
            .events
            .find::<storagext::runtime::storage_provider::events::SectorsPreCommitted>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| RpcError::internal_error(err, None))?;

*/
