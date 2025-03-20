use std::{collections::BTreeMap, path::PathBuf};

use bellperson::groth16;
use blstrs::Bls12;
use filecoin_proofs::{
    as_safe_commitment, parameters::window_post_setup_params, MerkleTreeTrait, PoStConfig,
    PoStType, PrivateReplicaInfo,
};
use primitives::{proofs::RegisteredPoStProof, sector::SectorNumber};
use rand::rngs::OsRng;
use storage_proofs_core::compound_proof::{self, CompoundProof};
use storage_proofs_post::fallback::{
    self, FallbackPoSt, FallbackPoStCompound, PrivateSector, PublicSector,
};

use crate::{
    get_partitions_for_window_post,
    types::{Commitment, ProverId, Ticket},
};

pub type PoStParameters = groth16::MappedParameters<Bls12>;

/// Automatically sets the generic parameters for any function that is generic over a SectorShape (sector size).
///
/// If a function has multiple generic parameters, the first one needs to be SectorShape.
///
/// # Reasoning
///
/// Underlying `rust-fil-proofs` functions used for proving are generic.
/// Those generics are dependant on the sector size.
/// This macro avoids the boilerplate of writing a `match` statement every time we need to use `rust-fil-proofs`.
///
/// # Examples
///
/// ```
/// # #[macro_use] extern crate polka_storage_proofs;
/// fn foo<SectorShape: filecoin_proofs::MerkleTreeTrait + 'static>() {}
///
/// match_post_proof!(
///     primitives::proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
///     foo::<_>()
/// );
/// ```
///
/// ```
/// # #[macro_use] extern crate polka_storage_proofs;
/// fn bar<SectorShape: filecoin_proofs::MerkleTreeTrait + 'static, R: AsRef<std::path::Path>>() {}
///
/// match_post_proof!(
///     primitives::proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
///     bar::<_, std::path::PathBuf>()
/// );
/// ```
#[macro_export]
macro_rules! match_post_proof {
    ($seal_proof:expr, $func:ident::<_>($($args:expr),*)) => {
        match_post_proof!($seal_proof, $func::<_,>($($args),*))
    };

    ($seal_proof:expr, $func:ident::<_, $($generic:ty),*>($($args:expr),*)) => {
        match $seal_proof {
            ::primitives::proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1 => $func::<::filecoin_proofs::SectorShape2KiB, $($generic),*>($($args),*),
            ::primitives::proofs::RegisteredPoStProof::StackedDRGWindow8MiBV1 => $func::<::filecoin_proofs::SectorShape8MiB, $($generic),*>($($args),*),
            ::primitives::proofs::RegisteredPoStProof::StackedDRGWindow512MiBV1 => $func::<::filecoin_proofs::SectorShape512MiB, $($generic),*>($($args),*),
            ::primitives::proofs::RegisteredPoStProof::StackedDRGWindow1GiBV1 => $func::<::filecoin_proofs::SectorShape1GiB, $($generic),*>($($args),*),
        }
    };
}

fn generate_params<S: MerkleTreeTrait + 'static>(
    post_config: PoStConfig,
) -> Result<groth16::Parameters<Bls12>, PoStError> {
    let public_params = filecoin_proofs::parameters::window_post_public_params::<S>(&post_config)?;
    let circuit =
        storage_proofs_post::fallback::FallbackPoStCompound::<S>::blank_circuit(&public_params);

    Ok(groth16::generate_random_parameters::<Bls12, _, _>(
        circuit, &mut OsRng,
    )?)
}

/// Generates parameters for proving and verifying PoSt.
/// It should be called once and then reused across provers and the verifier.
/// Verifying Key is only needed for verification (no_std), rest of the params are required for proving (std).
pub fn generate_random_groth16_parameters(
    seal_proof: RegisteredPoStProof,
) -> Result<groth16::Parameters<Bls12>, PoStError> {
    let post_config = seal_to_config(seal_proof);

    match_post_proof!(seal_proof, generate_params::<_>(post_config))
}

/// Loads Groth16 parameters from the specified path.
/// Parameters needed to be serialized with [`groth16::Paramters::<Bls12>::write_bytes`].
pub fn load_groth16_parameters(path: std::path::PathBuf) -> Result<PoStParameters, PoStError> {
    groth16::Parameters::<Bls12>::build_mapped_parameters(path.clone(), false)
        .map_err(|e| PoStError::FailedToLoadGrothParameters(path, e))
}

#[derive(Debug)]
pub struct ReplicaInfo {
    pub sector_id: SectorNumber,
    pub comm_r: Commitment,
    pub replica_path: PathBuf,
    pub cache_path: PathBuf,
}

/// Generates Windowed PoSt for a replica.
///
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/api/window_post.rs#L100>
pub fn generate_window_post<S: MerkleTreeTrait + 'static>(
    proof_type: RegisteredPoStProof,
    groth_params: &groth16::MappedParameters<Bls12>,
    randomness: Ticket,
    prover_id: ProverId,
    partition_replicas: Vec<ReplicaInfo>,
) -> Result<Vec<groth16::Proof<Bls12>>, PoStError> {
    let post_config = seal_to_config(proof_type);

    let randomness_safe = as_safe_commitment(&randomness, "randomness")?;
    let prover_id_safe = as_safe_commitment(&prover_id, "prover_id")?;

    let vanilla_params = window_post_setup_params(&post_config);
    // ASSUMPTION: there are no duplicates in `partition_replicas`, if there are there is a heavy bug upstream.
    let partitions =
        get_partitions_for_window_post(partition_replicas.len(), post_config.sector_count);

    let sector_count = vanilla_params.sector_count;
    let setup_params = compound_proof::SetupParams {
        vanilla_params,
        partitions,
        priority: post_config.priority,
    };

    let pub_params: compound_proof::PublicParams<'_, FallbackPoSt<'_, S>> =
        FallbackPoStCompound::setup(&setup_params)?;

    let mut replicas = BTreeMap::new();
    for replica in partition_replicas {
        replicas.insert(
            storage_proofs_core::sector::SectorId::from(u64::from(replica.sector_id)),
            PrivateReplicaInfo::<S>::new(replica.replica_path, replica.comm_r, replica.cache_path)?,
        );
    }

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

    let priv_inputs = fallback::PrivateInputs::<_> {
        sectors: &priv_sectors,
    };

    let proofs = FallbackPoStCompound::prove(&pub_params, &pub_inputs, &priv_inputs, groth_params)?;

    Ok(proofs)
}

/// References:
/// * <https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L644>
fn seal_to_config(seal_proof: RegisteredPoStProof) -> filecoin_proofs::PoStConfig {
    match seal_proof {
        RegisteredPoStProof::StackedDRGWindow2KiBV1P1
        | RegisteredPoStProof::StackedDRGWindow8MiBV1
        | RegisteredPoStProof::StackedDRGWindow512MiBV1 => filecoin_proofs::PoStConfig {
            sector_size: filecoin_proofs::SectorSize(seal_proof.sector_size().bytes()),
            challenge_count: filecoin_proofs::WINDOW_POST_CHALLENGE_COUNT,
            // https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/constants.rs#L104
            sector_count: 2,
            typ: PoStType::Window,
            priority: true,
            api_version: storage_proofs_core::api_version::ApiVersion::V1_2_0,
        },
        RegisteredPoStProof::StackedDRGWindow1GiBV1 => filecoin_proofs::PoStConfig {
            sector_size: filecoin_proofs::SectorSize(seal_proof.sector_size().bytes()),
            challenge_count: filecoin_proofs::WINDOW_POST_CHALLENGE_COUNT,
            // https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/constants.rs#L104
            sector_count: 25,
            typ: PoStType::Window,
            priority: true,
            api_version: storage_proofs_core::api_version::ApiVersion::V1_2_0,
        },
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
