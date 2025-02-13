use std::sync::Arc;

use itertools::Itertools;
use polka_storage_proofs::{
    match_post_proof,
    porep::sealer::{BlstrsProof, SubstrateProof},
    post::{self, generate_window_post, PoStParameters, ReplicaInfo},
};
use primitives::{
    proofs::{derive_prover_id, RegisteredPoStProof},
    randomness::{draw_randomness, DomainSeparationTag},
    sector::SectorNumber,
    PartitionNumber, MAX_PROOFS_PER_BLOCK,
};
use storagext::{
    runtime::runtime_types::primitives::pallets::DeadlineInfo,
    types::storage_provider::{PartitionState, PoStProof, SubmitWindowedPoStParams},
    RandomnessClientExt, StorageProviderClientExt, SystemClientExt,
};
use subxt::{
    ext::{codec::Encode, futures::future::join_all},
    tx::Signer,
};
use tokio::task::JoinError;

use crate::sector::ProvenSector;

#[derive(Debug, thiserror::Error)]
pub enum DeadlineError {
    #[error("precommit scheduled too early, randomness not available")]
    RandomnessNotAvailable,
    #[error("current deadline {0} or storage provider not found")]
    DeadlineNotFound(u64),
    #[error("deadline of index {0} does not have a state")]
    DeadlineStateNotFound(u64),
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

    pub async fn get_info(
        &self,
        xt_client: Arc<storagext::Client>,
        xt_keypair: &storagext::multipair::MultiPairSigner,
    ) -> Result<DeadlineInfo<u64>, DeadlineError> {
        tracing::info!("Getting deadline info for {} deadline", self.deadline_index);

        xt_client
            .deadline_info(&xt_keypair.account_id().into(), self.deadline_index)
            .await?
            .ok_or(DeadlineError::DeadlineNotFound(self.deadline_index))
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
            return Err(DeadlineError::DeadlineStateNotFound(self.deadline_index));
        };

        // This executes if there are no partitions in the deadline, or if those
        // partitions have no sectors to prove. In that case we just hang until
        // the next deadline.
        if deadline_state.partitions.is_empty()
            || deadline_state
                .partitions
                .iter()
                .all(|s| s.1.sectors.is_empty())
        {
            tracing::info!(
                "There is nothing to prove here. Waiting for deadline close: {}",
                deadline.close
            );
            // Wait until the current deadline closes, so we can exit an re-schedule,
            // NOTE(@jmg-duarte,05/02/2025): IMO placing this wait here saves on complexity for now
            // but ideally, we'd want to reschedule the task and have it wait BEFORE it exits here
            // I can't really justify *why* it's just my spidey sense tingling
            xt_client.wait_for_height(deadline.close, true).await?;
            return Ok(());
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

        // We can only push a limited number of proofs in a single extrinsic
        // call. So we are chunking the partitions and limiting the number of
        // proofs generated. 1 proof == 1 partition
        let chunked_partitions = deadline_state
            .partitions
            .into_iter()
            .chunks(MAX_PROOFS_PER_BLOCK as usize);

        // Prepare the proofs for required partitions
        let proving_futures = chunked_partitions
            .into_iter()
            .map(|partitions| {
                let partitions = partitions.collect();
                tracing::info!("Proving PoSt partitions... {:?}", partitions);

                let post_params = Arc::clone(&post_params);
                let xt_client = Arc::clone(&xt_client);
                let deadline = deadline.clone();
                self.generate_and_submit(
                    xt_client,
                    xt_keypair,
                    deadline,
                    partitions,
                    prover_id,
                    randomness,
                    post_params,
                    &sector_storage,
                )
            })
            .collect::<Vec<_>>();

        // Proving results
        let proving_results = join_all(proving_futures).await;
        for result in &proving_results {
            if let Err(err) = result {
                tracing::error!("Failed to submit post for deadline, {}", err);
            }
        }

        // Ok here doesn't mean that the post was successfully submitted for all
        // partitions. it only means that the process was completed.
        Ok(())
    }

    async fn generate_and_submit<SectorStorage>(
        &self,
        xt_client: Arc<storagext::Client>,
        xt_keypair: &storagext::multipair::MultiPairSigner,
        deadline: DeadlineInfo<u64>,
        partitions: Vec<(PartitionNumber, PartitionState)>,
        prover_id: [u8; 32],
        randomness: [u8; 32],
        post_params: Arc<PoStParameters>,
        sector_storage: SectorStorage,
    ) -> Result<(), DeadlineError>
    where
        SectorStorage: Fn(SectorNumber) -> Option<ProvenSector>,
    {
        // Generate proofs for partitions
        let (partitions, proofs) = self
            .generate_proofs_for_partitions(
                partitions,
                prover_id,
                randomness,
                post_params,
                sector_storage,
            )
            .await?;

        // Wait for the current deadline to open
        tracing::info!("Wait for block {} for open deadline", deadline.start);
        xt_client.wait_for_height(deadline.start, true).await?;

        // Submit proofs
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

    async fn generate_proofs_for_partitions<SectorStorage>(
        &self,
        partitions: Vec<(PartitionNumber, PartitionState)>,
        prover_id: [u8; 32],
        randomness: [u8; 32],
        post_params: Arc<PoStParameters>,
        sector_storage: SectorStorage,
    ) -> Result<(Vec<PartitionNumber>, Vec<PoStProof>), DeadlineError>
    where
        SectorStorage: Fn(SectorNumber) -> Option<ProvenSector>,
    {
        // Get replicas for sectors part of the partitions
        let replicas = partitions
            .iter()
            .flat_map(|(_id, state)| {
                state.sectors.iter().map(|sector_number| {
                    sector_storage(*sector_number)
                        .ok_or(DeadlineError::SectorNotFound(*sector_number))
                        .map(|sector| ReplicaInfo {
                            sector_id: sector.sector_number,
                            comm_r: sector.comm_r.raw(),
                            cache_path: sector.cache_path.clone(),
                            replica_path: sector.sealed_path.clone(),
                        })
                })
            })
            .collect::<Result<Vec<_>, DeadlineError>>()?;

        // Generate proofs
        let proofs: Vec<BlstrsProof> = {
            let post_params = post_params.clone();
            let post_proof = self.post_proof;

            tokio::task::spawn_blocking(move || {
                match_post_proof!(
                    post_proof,
                    generate_window_post::<_>(
                        post_proof,
                        &post_params,
                        randomness,
                        prover_id,
                        replicas
                    )
                )
            })
        }
        .await??;

        // Map proofs to our internal type
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

        let partition_numbers = partitions.iter().map(|(number, _)| *number).collect();

        Ok((partition_numbers, proofs))
    }
}
