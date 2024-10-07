//! This submodule separates all definitions enabled by feature `substrate`.
#![cfg(feature = "substrate")]

use super::*;

impl<E> Default for VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    fn default() -> Self {
        VerifyingKey::<E> {
            alpha_g1: G1Affine::default(),
            beta_g1: G1Affine::default(),
            beta_g2: G2Affine::default(),
            gamma_g2: G2Affine::default(),
            delta_g1: G1Affine::default(),
            delta_g2: G2Affine::default(),
            ic: alloc::vec![],
        }
    }
}

impl<E> ::codec::Decode for VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    fn decode<I: ::codec::Input>(input: &mut I) -> Result<Self, ::codec::Error> {
        let mut buffer = [0u8; VERIFYINGKEY_MAX_BYTES];
        let Some(n_bytes) = input.remaining_len()? else {
            return Err(::codec::Error::from("unable to get remaining_len"));
        };
        input.read(&mut buffer[..n_bytes])?;
        VerifyingKey::<E>::from_bytes(&buffer[..n_bytes])
            .map_err(|e| codec::Error::from(e.as_static_str()))
    }
}

impl<E> ::codec::Encode for VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    fn size_hint(&self) -> usize {
        self.serialised_bytes()
    }

    // TODO(@th7nder,#408, 28/09/2024): Encode/Decode, make it use `to_compressed` to reduce the bytes stored on-chain.
    fn encode_to<T: ::codec::Output + ?Sized>(&self, dest: &mut T) {
        dest.write(&self.alpha_g1.to_uncompressed()[..]);
        dest.write(&self.beta_g1.to_uncompressed()[..]);
        dest.write(&self.beta_g2.to_uncompressed()[..]);
        dest.write(&self.gamma_g2.to_uncompressed()[..]);
        dest.write(&self.delta_g1.to_uncompressed()[..]);
        dest.write(&self.delta_g2.to_uncompressed()[..]);
        dest.write(&(self.ic.len() as u32).to_be_bytes()[..]);
        for ic in &self.ic {
            dest.write(&ic.to_uncompressed()[..]);
        }
    }

    fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
        let mut buffer = Vec::<u8>::new();
        self.encode_to(&mut buffer);
        f(buffer.as_slice())
    }
}

impl<E: Engine> ::scale_info::TypeInfo for VerifyingKey<E> {
    type Identity = Self;

    fn type_info() -> ::scale_info::Type {
        ::scale_info::Type::builder()
            .path(::scale_info::Path::new("VerifyingKey", module_path!()))
            .composite(
                scale_info::build::Fields::named()
                    .field(|f| {
                        f.ty::<[u8; G1AFFINE_UNCOMPRESSED_BYTES]>()
                            .name("alpha_g1")
                            .type_name("G1Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G1AFFINE_UNCOMPRESSED_BYTES]>()
                            .name("beta_g1")
                            .type_name("G1Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G2AFFINE_UNCOMPRESSED_BYTES]>()
                            .name("beta_g2")
                            .type_name("G2Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G2AFFINE_UNCOMPRESSED_BYTES]>()
                            .name("gamma_g2")
                            .type_name("G2Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G1AFFINE_UNCOMPRESSED_BYTES]>()
                            .name("delta_g1")
                            .type_name("G1Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G2AFFINE_UNCOMPRESSED_BYTES]>()
                            .name("delta_g2")
                            .type_name("G2Affine")
                    })
                    .field(|f| {
                        f.ty::<Vec<[u8; G1AFFINE_UNCOMPRESSED_BYTES]>>()
                            .name("ic")
                            .type_name("Vec<G1Affine>")
                    }),
            )
    }
}

impl<E: Engine> Default for Proof<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    fn default() -> Self {
        Proof::<E> {
            a: G1Affine::default(),
            b: G2Affine::default(),
            c: G1Affine::default(),
        }
    }
}

impl<E> ::codec::Decode for Proof<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    fn decode<I: ::codec::Input>(input: &mut I) -> Result<Self, ::codec::Error> {
        let mut buffer = [0u8; PROOF_BYTES];
        input.read(&mut buffer[..])?;
        Proof::<E>::from_bytes(&buffer[..]).map_err(|e| codec::Error::from(e.as_static_str()))
    }
}

impl<E> ::codec::Encode for Proof<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    fn size_hint(&self) -> usize {
        PROOF_BYTES
    }

    fn encode_to<T: ::codec::Output + ?Sized>(&self, dest: &mut T) {
        dest.write(&self.a.to_compressed()[..]);
        dest.write(&self.b.to_compressed()[..]);
        dest.write(&self.c.to_compressed()[..]);
    }

    fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
        let mut buffer = Vec::new();
        self.encode_to(&mut buffer);
        f(buffer.as_slice())
    }
}

impl<E: Engine> ::scale_info::TypeInfo for Proof<E> {
    type Identity = Self;

    fn type_info() -> ::scale_info::Type {
        ::scale_info::Type::builder()
            .path(::scale_info::Path::new("VerifyingKey", module_path!()))
            .composite(
                scale_info::build::Fields::named()
                    .field(|f| {
                        f.ty::<[u8; G1AFFINE_UNCOMPRESSED_BYTES]>()
                            .name("a")
                            .type_name("G1Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G2AFFINE_UNCOMPRESSED_BYTES]>()
                            .name("b")
                            .type_name("G2Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G1AFFINE_UNCOMPRESSED_BYTES]>()
                            .name("c")
                            .type_name("G1Affine")
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use codec::{Decode, Encode};
    use rand::SeedableRng;

    use super::*;
    use crate::groth16::tests::TEST_SEED;

    /// This is a smoke test of the `codec::Encode` and `codec::Decode` implementation.
    #[test]
    fn verifyingkey_encode_decode() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let vkey = VerifyingKey::<Bls12>::random(&mut rng);
        let vkey_bytes = vkey.encode();
        let output = Vec::from(vkey_bytes);
        assert_eq!(
            vkey,
            VerifyingKey::decode(&mut output.as_slice()).expect("VerifyingKey::decode failed")
        );
    }

    /// This is a smoke test of the `codec::Encode` and `codec::Decode` implementation.
    #[test]
    fn proof_encode_decode() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let proof = Proof::<Bls12>::random(&mut rng);
        let proof_bytes = proof.encode();
        let output = Vec::from(proof_bytes);
        assert_eq!(
            proof,
            Proof::decode(&mut output.as_slice()).expect("Proof::decode failed")
        );
    }

    /// Tests the serialisation compatibility between scale codec and own implementation.
    #[test]
    fn scale_and_regular_serialisation() {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let proof = Proof::<Bls12>::random(&mut rng);
        let bytes_scale = proof.encode();
        let mut bytes_regular = vec![0u8; Proof::<Bls12>::serialised_bytes()];
        proof.into_bytes(&mut bytes_regular.as_mut_slice()).unwrap();
        assert_eq!(bytes_regular.as_slice(), bytes_scale.as_slice());
    }
}
