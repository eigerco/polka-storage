use std::{collections::BTreeSet, sync::Arc};

use polka_storage_proofs::{
    porep::sealer::{BlstrsProof, SubstrateProof},
    post::{self, PoStParameters, ReplicaInfo},
};
use primitives::{
    proofs::{derive_prover_id, RegisteredPoStProof},
    randomness::{draw_randomness, DomainSeparationTag},
    sector::SectorNumber,
};
use storagext::{
    runtime::runtime_types::primitives::pallets::DeadlineInfo, types::storage_provider::{PartitionState, PoStProof, SubmitWindowedPoStParams}, RandomnessClientExt, StorageProviderClientExt, SystemClientExt
};
use subxt::{ext::codec::Encode, tx::Signer};
use tokio::task::{JoinError, JoinHandle};

use crate::sector::ProvenSector;

#[derive(Debug, thiserror::Error)]
pub enum DeadlineError {
    #[error("precommit scheduled too early, randomness not available")]
    RandomnessNotAvailable,
    #[error("current deadline or storage provider not found")]
    DeadlineNotFound,
    #[error("deadline of given index does not have a state")]
    DeadlineStateNotFound,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error(transparent)]
    Subxt(#[from] subxt::Error),
    #[error(transparent)]
    PoSt(#[from] post::PoStError),
    #[error("sector {0} does not exist")]
    SectorNotFound(SectorNumber),
}

pub struct Deadline {
    post_proof: RegisteredPoStProof,
    deadline_index: u64,
}

impl Deadline {
    pub fn new(deadline_index: u64, post_proof: RegisteredPoStProof) -> Self {
        Self {
            deadline_index,
            post_proof,
        }
    }

    pub async fn get_info(&self, xt_client: Arc<storagext::Client>, xt_keypair: &storagext::multipair::MultiPairSigner) -> Result<DeadlineInfo<u64>, DeadlineError> {
        tracing::info!("Getting deadline info for {} deadline", self.deadline_index);

        xt_client
            .deadline_info(&xt_keypair.account_id().into(), self.deadline_index)
            .await?
            .ok_or(DeadlineError::DeadlineNotFound)
    }

    pub async fn submit_windowed_post<SectorStorage>(
        &self,
        xt_client: Arc<storagext::Client>,
        xt_keypair: &storagext::multipair::MultiPairSigner,
        post_params: Arc<PoStParameters>,
        sector_storage: SectorStorage,
    ) -> Result<(), DeadlineError>
    where
        SectorStorage: Fn(SectorNumber) -> Option<ProvenSector>,
    {
        let deadline = self.get_info(xt_client.clone(), xt_keypair).await?;

        tracing::debug!("Deadline Info: {:?}", deadline);
        tracing::info!(
            "Wait for challenge_block {}, start: {}, for deadline challenge",
            deadline.challenge_block,
            deadline.start
        );

        xt_client
            .wait_for_height(deadline.challenge_block, true)
            .await?;
        tracing::info!(
            "Waiting finished (block: {}), let's go",
            deadline.challenge_block
        );

        let Some(deadline_state) = xt_client
            .deadline_state(&xt_keypair.account_id().into(), self.deadline_index)
            .await?
        else {
            tracing::error!("Something went catastrophic, there is no current deadline state");
            return Err(DeadlineError::DeadlineStateNotFound);
        };

        if deadline_state.partitions.len() == 0 {
            tracing::info!("There are not partitions in this deadline yet. Nothing to prove here.");
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
            return Ok(());
        }

        let mut replicas = Vec::new();
        for sector_number in all_sectors {
            let sector = sector_storage(sector_number)
                .ok_or(DeadlineError::SectorNotFound(sector_number))?;

            replicas.push(ReplicaInfo {
                sector_id: sector_number,
                comm_r: sector.comm_r.raw(),
                cache_path: sector.cache_path.clone(),
                replica_path: sector.sealed_path.clone(),
            });
        }

        let prover_id = derive_prover_id(xt_keypair.account_id());
        let Some(digest) = xt_client.get_randomness(deadline.challenge_block).await? else {
            tracing::error!("Randomness for the block not available.");
            return Err(DeadlineError::RandomnessNotAvailable);
        };
        let entropy = xt_keypair.account_id().encode();
        let randomness = draw_randomness(
            &digest,
            DomainSeparationTag::WindowedPoStChallengeSeed,
            deadline.challenge_block,
            &entropy,
        );

        tracing::info!("Proving PoSt partitions... {:?}", partitions);
        let handle: JoinHandle<Result<Vec<BlstrsProof>, _>> = {
            let post_params = post_params.clone();
            let post_proof = self.post_proof.clone();

            tokio::task::spawn_blocking(move || {
                post::generate_window_post(
                    post_proof,
                    &post_params,
                    randomness,
                    prover_id,
                    replicas,
                )
            })
        };
        let proofs = handle.await??;
        tracing::info!("Generated PoSt proof for partitions: {:?}", partitions);

        let proofs = proofs
            .into_iter()
            .map(|p| PoStProof {
                post_proof: self.post_proof,
                proof_bytes: codec::Encode::encode(
                    &TryInto::<SubstrateProof>::try_into(p.clone()).expect(
                        "converstion between rust-fil-proofs and polka-storage-proofs to work",
                    ),
                ),
            })
            .collect::<Vec<_>>();

        tracing::info!("Wait for block {} for open deadline", deadline.start,);
        xt_client.wait_for_height(deadline.start, true).await?;

        let result = xt_client
            .submit_windowed_post(
                xt_keypair,
                SubmitWindowedPoStParams {
                    deadline: self.deadline_index,
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

        Ok(())
    }
}
