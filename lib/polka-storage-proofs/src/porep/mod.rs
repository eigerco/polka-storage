#![cfg(feature = "std")]

use bellperson::groth16;
use blstrs::Bls12;
use filecoin_proofs::{DefaultBinaryTree, DefaultPieceHasher};
use primitives_proofs::RegisteredSealProof;
use rand::rngs::OsRng;
use storage_proofs_core::{compound_proof::CompoundProof, proof::ProofScheme};
use storage_proofs_porep::stacked::StackedDrg;

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

#[derive(Debug, thiserror::Error)]
pub enum PoRepError {
    #[error("proof-level failure: {0}")]
    StorageProofsCoreError(#[from] storage_proofs_core::error::Error),
    #[error("key generation failure: {0}")]
    KeyGeneratorError(#[from] bellpepper_core::SynthesisError),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}
