#![cfg(feature = "std")]
//! This module should work only as a facade separating our codebase from `rust-fil-proofs`.
//! Only things that have direct `TryFrom`/`From` in the codebase, should leak from this thing.

pub mod sealer;

use bellperson::groth16;
use blstrs::Bls12;
use filecoin_proofs::{DefaultBinaryTree, DefaultPieceHasher};
use primitives_proofs::RegisteredSealProof;
use rand::rngs::OsRng;
use storage_proofs_core::{compound_proof::CompoundProof, proof::ProofScheme};
use storage_proofs_porep::stacked::StackedDrg;

use crate::types::Commitment;

/// Generates parameters for proving and verifying PoRep.
/// It should be called once and then reused across provers and the verifier.
/// Verifying Key is only needed for verification (no_std), rest of the params are required for proving (std).
pub fn generate_random_groth16_parameters(
    seal_proof: RegisteredSealProof,
) -> Result<groth16::Parameters<Bls12>, PoRepError> {
    let porep_config = seal_to_config(seal_proof);
    let setup_params = filecoin_proofs::parameters::setup_params(&porep_config)?;
    let public_params = StackedDrg::<DefaultBinaryTree, DefaultPieceHasher>::setup(&setup_params)?;

    let circuit = storage_proofs_porep::stacked::StackedCompound::<
        DefaultBinaryTree,
        DefaultPieceHasher,
    >::blank_circuit(&public_params);

    Ok(groth16::generate_random_parameters::<Bls12, _, _>(
        circuit, &mut OsRng,
    )?)
}

/// Loads Groth16 parameters from the specified path.
/// Parameters needed to be serialized with [`groth16::Paramters::<Bls12>::write_bytes`].
pub fn load_groth16_parameters(
    path: std::path::PathBuf,
) -> Result<groth16::MappedParameters<Bls12>, PoRepError> {
    groth16::Parameters::<Bls12>::build_mapped_parameters(path.clone(), false)
        .map_err(|e| PoRepError::FailedToLoadGrothParameters(path, e))
}

#[derive(Debug, thiserror::Error)]
pub enum PoRepError {
    #[error("proof-level failure: {0}")]
    StorageProofsCoreError(#[from] storage_proofs_core::error::Error),
    #[error("key generation failure: {0}")]
    KeyGeneratorError(#[from] bellpepper_core::SynthesisError),
    #[error("given cid at index {0} {1:?} differs from the generated one: {2:?}")]
    InvalidPieceCid(usize, Commitment, Commitment),
    #[error("tried to create a sector without pieces in it")]
    EmptySector,
    #[error("failed to load groth16 parameters from path: {0}, because {1}")]
    FailedToLoadGrothParameters(std::path::PathBuf, std::io::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

fn seal_to_config(seal_proof: RegisteredSealProof) -> filecoin_proofs::PoRepConfig {
    match seal_proof {
        RegisteredSealProof::StackedDRG2KiBV1P1 => {
            // https://github.com/filecoin-project/rust-filecoin-proofs-api/blob/b44e7cecf2a120aa266b6886628e869ba67252af/src/registry.rs#L308
            let sector_size = 1 << 11;
            let porep_id = [0u8; 32];
            let api_version = storage_proofs_core::api_version::ApiVersion::V1_2_0;

            filecoin_proofs::PoRepConfig::new_groth16(sector_size, porep_id, api_version)
        }
    }
}
