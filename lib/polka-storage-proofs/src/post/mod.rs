#![cfg(feature = "std")]

use crate::types::{Commitment, ProverId, Ticket};

use bellperson::groth16;
use blstrs::Bls12;
use filecoin_proofs::{DefaultOctTree, PoStType, PrivateReplicaInfo};
use primitives_proofs::{SectorNumber, RegisteredPoStProof};
use rand::rngs::OsRng;
use std::{path::{PathBuf, Path}, collections::BTreeMap};
use storage_proofs_core::compound_proof::CompoundProof;

/// Generates parameters for proving and verifying PoSt.
/// It should be called once and then reused across provers and the verifier.
/// Verifying Key is only needed for verification (no_std), rest of the params are required for proving (std).
pub fn generate_random_groth16_parameters(
    seal_proof: RegisteredPoStProof,
) -> Result<groth16::Parameters<Bls12>, PoStError> {
    let post_config = seal_to_config(seal_proof);

    let public_params =
        filecoin_proofs::parameters::window_post_public_params::<DefaultOctTree>(&post_config)?;

    let circuit =
        storage_proofs_post::fallback::FallbackPoStCompound::<DefaultOctTree>::blank_circuit(
            &public_params,
        );

    Ok(groth16::generate_random_parameters(circuit, &mut OsRng)?)
}

#[derive(Debug)]
pub struct ReplicaInfo {
    pub sector_id: SectorNumber,
    pub comm_r: Commitment,
    pub replica_path: PathBuf,
}

pub fn generate_window_post<CacheDirectory: AsRef<Path>>(
    proof_type: RegisteredPoStProof,
    randomness: Ticket,
    prover_id: ProverId,
    partition_replicas: Vec<ReplicaInfo>,
    cache_dir: CacheDirectory,
) -> Result<Vec<u8>, PoStError> {
    let config = seal_to_config(proof_type);
    let mut replicas = BTreeMap::new();
    for replica in partition_replicas {
        replicas.insert(replica.sector_id.into(), PrivateReplicaInfo::<DefaultOctTree>::new(
            replica.replica_path,
            replica.comm_r,
            cache_dir.as_ref().to_path_buf()
        )?);
    }

    Ok(filecoin_proofs::generate_window_post(&config, &randomness, &replicas, prover_id)?)
}

/// References:
/// * <https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L644>
fn seal_to_config(seal_proof: RegisteredPoStProof) -> filecoin_proofs::PoStConfig {
    match seal_proof {
        RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => {
            filecoin_proofs::PoStConfig {
                sector_size: filecoin_proofs::SectorSize(seal_proof.sector_size().bytes()),
                challenge_count: filecoin_proofs::WINDOW_POST_CHALLENGE_COUNT,
                // https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/constants.rs#L104
                sector_count: 2,
                typ: PoStType::Window,
                priority: true,
                api_version: storage_proofs_core::api_version::ApiVersion::V1_2_0,
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PoStError {
    #[error("key generation failure: {0}")]
    KeyGeneratorError(#[from] bellpepper_core::SynthesisError),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
