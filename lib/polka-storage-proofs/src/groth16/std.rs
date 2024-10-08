//! This submodule separates all definitions enabled by feature `std`.
#![cfg(feature = "std")]

extern crate std;

use bellperson::groth16 as bp_g16;
use pairing::Engine;

use super::*;

impl<E> TryFrom<bp_g16::VerifyingKey<blstrs::Bls12>> for VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    type Error = FromBytesError;

    fn try_from(vkey: bp_g16::VerifyingKey<blstrs::Bls12>) -> Result<Self, Self::Error> {
        let mut ic = Vec::<G1Affine>::new();
        for i in &vkey.ic {
            ic.push(g1affine(i)?);
        }
        Ok(VerifyingKey::<E> {
            alpha_g1: g1affine(&vkey.alpha_g1)?,
            beta_g1: g1affine(&vkey.beta_g1)?,
            beta_g2: g2affine(&vkey.beta_g2)?,
            gamma_g2: g2affine(&vkey.gamma_g2)?,
            delta_g1: g1affine(&vkey.delta_g1)?,
            delta_g2: g2affine(&vkey.delta_g2)?,
            ic,
        })
    }
}

impl<E> TryFrom<bp_g16::Proof<blstrs::Bls12>> for Proof<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    type Error = FromBytesError;

    fn try_from(vkey: bp_g16::Proof<blstrs::Bls12>) -> Result<Self, Self::Error> {
        Ok(Proof::<E> {
            a: g1affine(&vkey.a)?,
            b: g2affine(&vkey.b)?,
            c: g1affine(&vkey.c)?,
        })
    }
}

impl std::fmt::Display for FromBytesError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{}", self.as_static_str())
    }
}

impl std::error::Error for FromBytesError {}

/// Method transforms a `blstrs::G1Affine` into a `bls12_381::G1Affine`.
fn g1affine(affine: &blstrs::G1Affine) -> Result<G1Affine, FromBytesError> {
    G1Affine::from_uncompressed(&affine.to_uncompressed())
        .into_option()
        .ok_or(FromBytesError::G1AffineConversion)
}

/// Method transforms a `blstrs::G2Affine` into a `bls12_381::G2Affine`.
fn g2affine(affine: &blstrs::G2Affine) -> Result<G2Affine, FromBytesError> {
    G2Affine::from_uncompressed(&affine.to_uncompressed())
        .into_option()
        .ok_or(FromBytesError::G2AffineConversion)
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;
    use crate::groth16::tests::TEST_SEED;

    fn blstrs_rand_g1affine(rng: &mut XorShiftRng) -> blstrs::G1Affine {
        (blstrs::G1Affine::generator() * blstrs::Scalar::random(rng)).into()
    }

    fn blstrs_rand_g2affine(rng: &mut XorShiftRng) -> blstrs::G2Affine {
        (blstrs::G2Affine::generator() * blstrs::Scalar::random(rng)).into()
    }

    fn random_bellperson_verifying_key(
        rng: &mut XorShiftRng,
    ) -> bp_g16::VerifyingKey<blstrs::Bls12> {
        bp_g16::VerifyingKey::<blstrs::Bls12> {
            alpha_g1: blstrs_rand_g1affine(rng),
            beta_g1: blstrs_rand_g1affine(rng),
            beta_g2: blstrs_rand_g2affine(rng),
            gamma_g2: blstrs_rand_g2affine(rng),
            delta_g1: blstrs_rand_g1affine(rng),
            delta_g2: blstrs_rand_g2affine(rng),
            ic: vec![
                blstrs_rand_g1affine(rng),
                blstrs_rand_g1affine(rng),
                blstrs_rand_g1affine(rng),
            ],
        }
    }

    fn random_bellperson_proof(rng: &mut XorShiftRng) -> bp_g16::Proof<blstrs::Bls12> {
        bp_g16::Proof::<blstrs::Bls12> {
            a: blstrs_rand_g1affine(rng),
            b: blstrs_rand_g2affine(rng),
            c: blstrs_rand_g1affine(rng),
        }
    }

    /// This test is about testing the deserialisation of `VerifyingKey` with bytes that have been
    /// serialised from `bellperson::VerifyingKey`.
    #[test]
    fn verifying_key_from_bytes_from_bellperson() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        // Generate a verifying key with bellperson crate.
        let bp_vkey = random_bellperson_verifying_key(&mut rng);
        // Smoke test about converting it directly to our implemmentation.
        let vkey =
            VerifyingKey::<Bls12>::try_from(bp_vkey.clone()).expect("expect VerifiyingKey::from");
        // Compare each single fields by their bytes to make sure conversion was correct.
        assert_eq!(
            bp_vkey.alpha_g1.to_uncompressed(),
            vkey.alpha_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.beta_g1.to_uncompressed(),
            vkey.beta_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.beta_g2.to_uncompressed(),
            vkey.beta_g2.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.gamma_g2.to_uncompressed(),
            vkey.gamma_g2.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.delta_g1.to_uncompressed(),
            vkey.delta_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.delta_g2.to_uncompressed(),
            vkey.delta_g2.to_uncompressed()
        );
        assert_eq!(bp_vkey.ic.len(), vkey.ic.len());
        for i in 0..bp_vkey.ic.len() {
            assert_eq!(
                bp_vkey.ic[i].to_uncompressed(),
                vkey.ic[i].to_uncompressed()
            );
        }
    }

    #[test]
    fn verifyingkey_serialise_and_deserialise_direct_bellperson() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        // Generate a verifying key with bellperson crate.
        let bp_vkey = random_bellperson_verifying_key(&mut rng);
        // Serialise it by using its `Read` implementation.
        let bytes = VERIFYINGKEY_MIN_BYTES + bp_vkey.ic.len() * G1AFFINE_UNCOMPRESSED_BYTES;
        let mut bytes = vec![0u8; bytes];
        bp_vkey.write(bytes.as_mut_slice()).unwrap();
        // Try to deserialise it by using `VerifyingKey::from_bytes()`.
        let vkey = VerifyingKey::<Bls12>::from_bytes(bytes.as_slice()).unwrap();
        // Compare their values.
        assert_eq!(
            bp_vkey.alpha_g1.to_uncompressed(),
            vkey.alpha_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.beta_g1.to_uncompressed(),
            vkey.beta_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.beta_g2.to_uncompressed(),
            vkey.beta_g2.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.gamma_g2.to_uncompressed(),
            vkey.gamma_g2.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.delta_g1.to_uncompressed(),
            vkey.delta_g1.to_uncompressed()
        );
        assert_eq!(
            bp_vkey.delta_g2.to_uncompressed(),
            vkey.delta_g2.to_uncompressed()
        );
        assert_eq!(bp_vkey.ic.len(), vkey.ic.len());
        for i in 0..bp_vkey.ic.len() {
            assert_eq!(
                bp_vkey.ic[i].to_uncompressed(),
                vkey.ic[i].to_uncompressed()
            );
        }
        // Serialise our implementation as well by using `VerifyingKey::into_bytes()'.
        let mut bytes = vec![0u8; vkey.serialised_bytes()];
        vkey.into_bytes(bytes.as_mut_slice()).unwrap();
        // Deserialise bytes to bellperson's VerifyingKey by using its `Write` implementation.
        let bp_vkey_result = bp_g16::VerifyingKey::<blstrs::Bls12>::read(bytes.as_slice()).unwrap();
        // Compare initial struct with this one.
        assert_eq!(bp_vkey, bp_vkey_result);
    }

    /// This test is about testing the deserialisation of `Proof` with bytes that have been
    /// serialised from `bellperson::Proof`.
    #[test]
    fn proof_from_bytes_from_bellperson() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        // Generate a proof with bellperson crate.
        let bp_proof = random_bellperson_proof(&mut rng);
        // Smoke test about converting it directly to our implemmentation.
        let proof = Proof::<Bls12>::try_from(bp_proof.clone()).expect("expect Proof::from");
        // Compare each single fields by their bytes to make sure conversion was correct.
        assert_eq!(bp_proof.a.to_compressed(), proof.a.to_compressed());
        assert_eq!(bp_proof.b.to_compressed(), proof.b.to_compressed());
        assert_eq!(bp_proof.c.to_compressed(), proof.c.to_compressed());
    }

    #[test]
    fn proof_serialise_and_deserialise_direct_bellperson() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        // Generate a verifying key with bellperson crate.
        let bp_proof = random_bellperson_proof(&mut rng);
        // Serialise it by using its `Read` implementation.
        let mut bytes = vec![0u8; PROOF_BYTES];
        bp_proof.write(bytes.as_mut_slice()).unwrap();
        // Try to deserialise it by using `Proof::from_bytes()`.
        let proof = Proof::<Bls12>::from_bytes(bytes.as_slice()).unwrap();
        // Compare their values.
        assert_eq!(bp_proof.a.to_compressed(), proof.a.to_compressed());
        assert_eq!(bp_proof.b.to_compressed(), proof.b.to_compressed());
        assert_eq!(bp_proof.c.to_compressed(), proof.c.to_compressed());
        // Serialise our implementation as well by using `Proof::into_bytes()'.
        let mut bytes = vec![0u8; PROOF_BYTES];
        proof.into_bytes(bytes.as_mut_slice()).unwrap();
        // Deserialise bytes to bellperson's VerifyingKey by using its `Write` implementation.
        let bp_proof_result = bp_g16::Proof::<blstrs::Bls12>::read(bytes.as_slice()).unwrap();
        // Compare initial struct with this one.
        assert_eq!(bp_proof, bp_proof_result);
    }
}
