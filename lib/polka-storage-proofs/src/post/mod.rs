#![cfg(feature = "std")]

use bellperson::groth16;
use blstrs::Bls12;
use filecoin_proofs::{DefaultOctTree, PoStType};
use primitives_proofs::RegisteredPoStProof;
use rand::rngs::OsRng;
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
