//! This module encapsulate all relevant functionality to send verifying-key and proofs to the
//! pallet, i.e. converting it to another format, serialisation etc.

use bellperson::groth16::{Proof, VerifyingKey};
pub use primitives_proofs::{
    G1Affine, PreparedVerifyingKey as PtPreparedVerifyingKey, Proof as PtProof,
    PublicInputs as PtPublicInputs, VerifyingKey as PtVerifyingKey,
};

// TODO
pub fn into_pallet_proof(
    proofs: &[Proof<blstrs::Bls12>],
) -> anyhow::Result<Vec<PtProof<bls12_381::Bls12>>> {
    let mut pt_proofs = Vec::<PtProof<bls12_381::Bls12>>::new();

    for proof in proofs.iter() {
        pt_proofs.push(PtProof::<bls12_381::Bls12> {
            a: g1affine(&proof.a)?,
            b: g2affine(&proof.b)?,
            c: g1affine(&proof.c)?,
        });
    }

    Ok(pt_proofs)
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

// // TODO
// const WINDOW_SIZE: usize = 8;

// // TODO
// pub fn pallet_prepared_verifying_key(
//     vk: &VerifyingKey<blstrs::Bls12>,
// ) -> anyhow::Result<PtPreparedVerifyingKey<bls12_381::Bls12>> {
//     let neg_gamma = -vk.gamma_g2;
//     let neg_delta = -vk.delta_g2;

//     let multiscalar = precompute_fixed_window(&vk.ic[..], WINDOW_SIZE)?;
//     let mut ic_projective = Vec::<bls12_381::G1Projective>::new();
//     let mut ic = Vec::<G1Affine>::new();
//     for i in &vk.ic {
//         let affine = g1affine(i)?;
//         ic_projective.push(affine.to_curve());
//         ic.push(affine);
//     }

//     Ok(PtPreparedVerifyingKey {
//         alpha_g1_beta_g2: bls12_381::Bls12::pairing(&g1affine(&vk.alpha_g1)?, &g2affine(&vk.beta_g2)?),
//         neg_gamma_g2: g2affine(&neg_gamma)?.into(),
//         neg_delta_g2: g2affine(&neg_delta)?.into(),
//         gamma_g2: g2affine(&vk.gamma_g2)?.into(),
//         delta_g2: g2affine(&vk.delta_g2)?.into(),
//         ic,
//         multiscalar,
//         alpha_g1: g1affine(&vk.alpha_g1)?.to_curve(),
//         beta_g2: g2affine(&vk.beta_g2)?.into(),
//         // ic_projective: vk.ic.par_iter().map(|i| g1affine(i).to_curve()).collect(),
//         ic_projective,
//     })
// }

// /// Precompute the tables for fixed bases.
// pub fn precompute_fixed_window(
//     points: &[blstrs::G1Affine],
//     window_size: usize,
// ) -> anyhow::Result<PtMultiscalarPrecompOwned<bls12_381::Bls12>> {
//     let table_entries = (1 << window_size) - 1;
//     let num_points = points.len();

//     let tables = Vec::<Vec<G1Affine>>::new();
//     for p in points {
//         // .into_par_iter()
//         // .map(|opoint| {
//             let point = g1affine(p)?;

//             let mut table = Vec::<G1Affine>::with_capacity(table_entries);
//             table.push(point.clone());

//             let mut cur_precomp_point = point.to_curve();

//             for _ in 1..table_entries {
//                 // cur_precomp_point.add_assign(point);
//                 cur_precomp_point += point;
//                 table.push(cur_precomp_point.to_affine());
//             }

//             // table
//         // })
//         // .collect();
//     }

//     Ok(PtMultiscalarPrecompOwned {
//         num_points: num_points as u64,
//         window_size: window_size as u64,
//         window_mask: (1 << window_size) - 1,
//         table_entries: table_entries as u64,
//         tables,
//     })
// }

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
