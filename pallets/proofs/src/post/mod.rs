mod config;

use config::Config;
use frame_support::{pallet_prelude::*, sp_runtime::BoundedBTreeMap};
use polka_storage_proofs::get_partitions_for_window_post;
use primitives::{
    commitment::RawCommitment,
    proofs::{PublicReplicaInfo, RegisteredPoStProof, Ticket, MAX_SECTORS_PER_PROOF},
    sector::SectorNumber,
    NODE_SIZE,
};
use sha2::{Digest, Sha256};

use crate::{
    crypto::groth16::{verify_proof, Bls12, Fr, Proof, VerificationError, VerifyingKey},
    fr32, Vec,
};

const LOG_TARGET: &'static str = "runtime::proofs::post";

pub struct ProofScheme {
    config: Config,
}

impl ProofScheme {
    pub fn setup(post_type: RegisteredPoStProof) -> Self {
        Self {
            config: Config::new(post_type),
        }
    }

    /// Verifies PoSt for all of the replicas.
    ///
    /// Replicas are bounded to the largest possible amount, 2349 for 32GiB.
    /// For other proof size the length of the input map is checked.
    ///
    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/api/window_post.rs#L181>
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/storage-proofs-core/src/compound_proof.rs#L148>
    pub fn verify(
        &self,
        randomness: Ticket,
        replicas: BoundedBTreeMap<SectorNumber, PublicReplicaInfo, ConstU32<MAX_SECTORS_PER_PROOF>>,
        vk: VerifyingKey<Bls12>,
        proof: Proof<Bls12>,
    ) -> Result<(), ProofError> {
        let randomness = fr32::bytes_into_fr(&randomness)
            .map_err(|_| ProofError::Conversion)?
            .into();

        let required_partitions = get_partitions_for_window_post(
            replicas.len(),
            self.config.challenged_sectors_per_partition,
        )
        .unwrap_or(1);

        if required_partitions != 1 {
            // We don't support more than 1 partition in this method right now.
            return Err(ProofError::InvalidNumberOfProofs);
        }

        // NOTE:
        //  * This is checked after the required partitions on purpose!
        //  * Once we support verification of multiple partitions this check should be done for every partition
        let replica_count = replicas.len();
        ensure!(
            replica_count <= self.config.challenged_sectors_per_partition,
            {
                log::error!(
                    target: LOG_TARGET,
                    "Got more replicas than expected. Expected max replicas = {}, submitted replicas = {replica_count}",
                    self.config.challenged_sectors_per_partition
                );
                ProofError::InvalidNumberOfReplicas
            }
        );
        let pub_sectors: Vec<_> = replicas
            .iter()
            .map(|(sector_id, replica)| {
                Ok(PublicSector {
                    id: *sector_id,
                    comm_r: fr32::bytes_into_fr(&replica.comm_r)
                        .map_err(|_| ProofError::Conversion)?,
                })
            })
            .collect::<Result<_, ProofError>>()?;

        let public_inputs = PublicInputs {
            randomness,
            sectors: pub_sectors,
        };

        let inputs = self.generate_public_inputs(public_inputs, None)?;
        verify_proof(vk, &proof, inputs.as_slice())?;
        Ok(())
    }

    /// References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/storage-proofs-post/src/fallback/compound.rs#L42>
    fn generate_public_inputs(
        &self,
        public_inputs: PublicInputs,
        partition_k: Option<usize>,
    ) -> Result<Vec<Fr>, ProofError> {
        let num_sectors_per_chunk = self.config.challenged_sectors_per_partition;
        let partition_index = partition_k.unwrap_or(0);

        let sectors = public_inputs
            .sectors
            .chunks(num_sectors_per_chunk)
            .nth(partition_index)
            .ok_or(ProofError::InvalidNumberOfSectors)?;

        let mut inputs = Vec::new();
        for sector in sectors {
            inputs.push(sector.comm_r);

            let mut challenge_hasher = Sha256::new();
            challenge_hasher.update(AsRef::<[u8]>::as_ref(&public_inputs.randomness));
            challenge_hasher.update(&u64::from(sector.id).to_le_bytes()[..]);

            for n in 0..self.config.challenges_per_sector {
                // let sector_index =
                //     partition_index * self.config.challenged_sectors_per_partition + i;
                let challenge_index = n as u64;
                let challenged_leaf =
                    self.generate_leaf_challenge_inner(challenge_hasher.clone(), challenge_index);

                inputs.push(Fr::from(challenged_leaf));
            }
        }

        let num_inputs_per_sector = inputs.len() / sectors.len();
        // duplicate last one if too little sectors available
        while inputs.len() / num_inputs_per_sector < num_sectors_per_chunk {
            let s = inputs[inputs.len() - num_inputs_per_sector..].to_vec();
            inputs.extend_from_slice(&s);
        }
        assert_eq!(inputs.len(), num_inputs_per_sector * num_sectors_per_chunk);

        Ok(inputs)
    }

    pub fn generate_leaf_challenge_inner(
        &self,
        mut hasher: Sha256,
        leaf_challenge_index: u64,
    ) -> u64 {
        hasher.update(&leaf_challenge_index.to_le_bytes()[..]);
        let hash = hasher.finalize();

        let leaf_challenge =
            u64::from_le_bytes(hash[..8].try_into().expect("hashed value to be 32 bytes"));

        leaf_challenge % (self.config.sector_size / NODE_SIZE as u64)
    }
}

#[derive(core::fmt::Debug)]
pub enum ProofError {
    InvalidNumberOfSectors,
    InvalidNumberOfProofs,
    /// Returned when the given replicas exceeds the maximum amount set by the SP.
    InvalidNumberOfReplicas,
    /// Returned when the given proof was invalid in a verification.
    InvalidProof,
    /// Returned when the given verifying key was invalid.
    InvalidVerifyingKey,
    /// Returned in case of failed conversion, i.e. in `bytes_into_fr()`.
    Conversion,
}

impl From<VerificationError> for ProofError {
    fn from(value: VerificationError) -> ProofError {
        match value {
            VerificationError::InvalidProof => ProofError::InvalidProof,
            VerificationError::InvalidVerifyingKey => ProofError::InvalidVerifyingKey,
        }
    }
}

struct PublicInputs {
    randomness: RawCommitment,
    sectors: Vec<PublicSector>,
}

struct PublicSector {
    id: SectorNumber,
    comm_r: Fr,
}
