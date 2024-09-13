//! This module encapsulate all relevant functionality to send verifying-key and proofs to the
//! pallet, i.e. converting it to another format, serialisation etc.

use bellperson::groth16::{Proof, VerifyingKey};
pub use polka_storage_proofs::{
    G1Affine, Proof as PtProof, PublicInputs as PtPublicInputs, VerifyingKey as PtVerifyingKey,
};
// pub use polka_storage_proofs::PreparedVerifyingKey as PtPreparedVerifyingKey;

// TODO
pub fn into_pallet_proof(proof: Proof<blstrs::Bls12>) -> anyhow::Result<PtProof<bls12_381::Bls12>> {
    Ok(PtProof::<bls12_381::Bls12> {
        a: g1affine(&proof.a)?,
        b: g2affine(&proof.b)?,
        c: g1affine(&proof.c)?,
    })
}

// TODO
pub fn into_pallet_verifying_key(
    vkey: &VerifyingKey<blstrs::Bls12>,
) -> anyhow::Result<PtVerifyingKey<bls12_381::Bls12>> {
    let mut ic = Vec::<G1Affine>::new();
    for i in &vkey.ic {
        ic.push(g1affine(i)?);
    }
    Ok(PtVerifyingKey::<bls12_381::Bls12> {
        alpha_g1: g1affine(&vkey.alpha_g1)?,
        beta_g1: g1affine(&vkey.beta_g1)?,
        beta_g2: g2affine(&vkey.beta_g2)?,
        gamma_g2: g2affine(&vkey.gamma_g2)?,
        delta_g1: g1affine(&vkey.delta_g1)?,
        delta_g2: g2affine(&vkey.delta_g2)?,
        ic,
    })
}

// TODO
pub fn into_pallet_public_inputs(
    inputs: &[blstrs::Scalar],
) -> anyhow::Result<PtPublicInputs<bls12_381::Bls12>> {
    let mut vec = Vec::<bls12_381::Scalar>::new();
    for input in inputs {
        vec.push(scalar(input)?);
    }
    Ok(PtPublicInputs(vec))
}

fn g1affine(affine: &blstrs::G1Affine) -> anyhow::Result<bls12_381::G1Affine> {
    bls12_381::G1Affine::from_uncompressed(&affine.to_uncompressed())
        .into_option()
        .ok_or(anyhow::anyhow!("conversion error"))
}

fn g2affine(affine: &blstrs::G2Affine) -> anyhow::Result<bls12_381::G2Affine> {
    bls12_381::G2Affine::from_uncompressed(&affine.to_uncompressed())
        .into_option()
        .ok_or(anyhow::anyhow!("conversion error"))
}

fn scalar(scalar: &blstrs::Scalar) -> anyhow::Result<bls12_381::Scalar> {
    bls12_381::Scalar::from_bytes(&scalar.to_bytes_le())
        .into_option()
        .ok_or(anyhow::anyhow!("conversion error"))
}
