#![cfg(feature = "std")]

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use bellperson::groth16;
use blstrs::Bls12;
use filecoin_proofs::{
    as_safe_commitment, parameters::window_post_setup_params, PoStType, PrivateReplicaInfo,
    SectorShapeBase,
};
use primitives_proofs::{RegisteredPoStProof, SectorNumber};
use rand::rngs::OsRng;
use storage_proofs_core::compound_proof::{self, CompoundProof};
use storage_proofs_post::fallback::{
    self, FallbackPoSt, FallbackPoStCompound, PrivateSector, PublicSector,
};

use crate::types::{Commitment, ProverId, Ticket};

/// Generates parameters for proving and verifying PoSt.
/// It should be called once and then reused across provers and the verifier.
/// Verifying Key is only needed for verification (no_std), rest of the params are required for proving (std).
pub fn generate_random_groth16_parameters(
    seal_proof: RegisteredPoStProof,
) -> Result<groth16::Parameters<Bls12>, PoStError> {
    let post_config = seal_to_config(seal_proof);

    let public_params =
        filecoin_proofs::parameters::window_post_public_params::<SectorShapeBase>(&post_config)?;

    let circuit =
        storage_proofs_post::fallback::FallbackPoStCompound::<SectorShapeBase>::blank_circuit(
            &public_params,
        );

    Ok(groth16::generate_random_parameters(circuit, &mut OsRng)?)
}

/// Loads Groth16 parameters from the specified path.
/// Parameters needed to be serialized with [`groth16::Paramters::<Bls12>::write_bytes`].
pub fn load_groth16_parameters(
    path: std::path::PathBuf,
) -> Result<groth16::MappedParameters<Bls12>, PoStError> {
    groth16::Parameters::<Bls12>::build_mapped_parameters(path.clone(), false)
        .map_err(|e| PoStError::FailedToLoadGrothParameters(path, e))
}

#[derive(Debug)]
pub struct ReplicaInfo {
    pub sector_id: SectorNumber,
    pub comm_r: Commitment,
    pub replica_path: PathBuf,
}

/// Generates Windowed PoSt for a replica.
/// Only supports 2KiB sectors.
///
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/api/window_post.rs#L100>
pub fn generate_window_post<CacheDirectory: AsRef<Path>>(
    proof_type: RegisteredPoStProof,
    groth_params: &groth16::MappedParameters<Bls12>,
    randomness: Ticket,
    prover_id: ProverId,
    partition_replicas: Vec<ReplicaInfo>,
    cache_dir: CacheDirectory,
) -> Result<Vec<groth16::Proof<Bls12>>, PoStError> {
    type Tree = SectorShapeBase;

    let post_config = seal_to_config(proof_type);
    let mut replicas = BTreeMap::new();
    for replica in partition_replicas {
        replicas.insert(
            replica.sector_id.into(),
            PrivateReplicaInfo::<Tree>::new(
                replica.replica_path,
                replica.comm_r,
                cache_dir.as_ref().to_path_buf(),
            )?,
        );
    }
    let randomness_safe = as_safe_commitment(&randomness, "randomness")?;
    let prover_id_safe = as_safe_commitment(&prover_id, "prover_id")?;

    let vanilla_params = window_post_setup_params(&post_config);
    let partitions = get_partitions_for_window_post(replicas.len(), &post_config);

    let sector_count = vanilla_params.sector_count;
    let setup_params = compound_proof::SetupParams {
        vanilla_params,
        partitions,
        priority: post_config.priority,
    };

    let pub_params: compound_proof::PublicParams<'_, FallbackPoSt<'_, Tree>> =
        FallbackPoStCompound::setup(&setup_params)?;

    let trees: Vec<_> = replicas
        .values()
        .map(|replica| replica.merkle_tree(post_config.sector_size))
        .collect::<Result<_, _>>()?;

    let mut pub_sectors = Vec::with_capacity(sector_count);
    let mut priv_sectors = Vec::with_capacity(sector_count);

    for ((sector_id, replica), tree) in replicas.iter().zip(trees.iter()) {
        let comm_r = replica.safe_comm_r()?;
        let comm_c = replica.safe_comm_c();
        let comm_r_last = replica.safe_comm_r_last();

        pub_sectors.push(PublicSector {
            id: *sector_id,
            comm_r,
        });
        priv_sectors.push(PrivateSector {
            tree,
            comm_c,
            comm_r_last,
        });
    }

    let pub_inputs = fallback::PublicInputs {
        randomness: randomness_safe,
        prover_id: prover_id_safe,
        sectors: pub_sectors,
        k: None,
    };

    let priv_inputs = fallback::PrivateInputs::<Tree> {
        sectors: &priv_sectors,
    };

    let proofs = FallbackPoStCompound::prove(&pub_params, &pub_inputs, &priv_inputs, groth_params)?;

    Ok(proofs)
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
    #[error("failed to load groth16 parameters from path: {0}, because {1}")]
    FailedToLoadGrothParameters(std::path::PathBuf, std::io::Error),
}

/// Reference:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/api/post_util.rs#L217>
fn get_partitions_for_window_post(
    total_sector_count: usize,
    post_config: &filecoin_proofs::PoStConfig,
) -> Option<usize> {
    let partitions = (total_sector_count as f32 / post_config.sector_count as f32).ceil() as usize;

    (partitions > 1).then_some(partitions)
}
