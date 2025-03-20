//! This submodule separates all definitions enabled by feature `substrate`.
#![cfg(feature = "substrate")]

use super::*;

impl Default for VerifyingKey<Bls12> {
    fn default() -> Self {
        VerifyingKey::<Bls12> {
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

impl ::codec::Decode for VerifyingKey<Bls12> {
    fn decode<I: ::codec::Input>(input: &mut I) -> Result<Self, ::codec::Error> {
        // We can't allocate 1.4MiB required for PoSt on stack.
        let mut buffer = alloc::vec::Vec::with_capacity(POST_VERIFYINGKEY_MAX_BYTES);
        let Some(n_bytes) = input.remaining_len()? else {
            return Err(::codec::Error::from("unable to get remaining_len"));
        };
        if n_bytes > POST_VERIFYINGKEY_MAX_BYTES {
            return Err(::codec::Error::from(
                "provided verifying key is too big for the current limit of bytes",
            ));
        }
        buffer.resize(n_bytes, 0);
        input.read(&mut buffer[..n_bytes])?;
        VerifyingKey::<Bls12>::from_bytes(&buffer[..n_bytes])
            .map_err(|e| codec::Error::from(e.as_static_str()))
    }
}

impl ::codec::EncodeLike for VerifyingKey<Bls12> {}

impl ::codec::Encode for VerifyingKey<Bls12> {
    fn size_hint(&self) -> usize {
        self.serialised_bytes()
    }

    fn encode_to<T: ::codec::Output + ?Sized>(&self, dest: &mut T) {
        dest.write(&self.alpha_g1.to_compressed()[..]);
        dest.write(&self.beta_g1.to_compressed()[..]);
        dest.write(&self.beta_g2.to_compressed()[..]);
        dest.write(&self.gamma_g2.to_compressed()[..]);
        dest.write(&self.delta_g1.to_compressed()[..]);
        dest.write(&self.delta_g2.to_compressed()[..]);
        dest.write(&(self.ic.len() as u32).to_be_bytes()[..]);
        for ic in &self.ic {
            dest.write(&ic.to_compressed()[..]);
        }
    }

    fn using_encoded<R, F: FnOnce(&[u8]) -> R>(&self, f: F) -> R {
        let mut buffer = Vec::<u8>::new();
        self.encode_to(&mut buffer);
        f(buffer.as_slice())
    }
}

impl codec::MaxEncodedLen for VerifyingKey<Bls12> {
    fn max_encoded_len() -> usize {
        MAX_PRODUCTION_POST_VK_IC_LEN
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
                        f.ty::<[u8; G1AFFINE_COMPRESSED_BYTES]>()
                            .name("alpha_g1")
                            .type_name("G1Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G1AFFINE_COMPRESSED_BYTES]>()
                            .name("beta_g1")
                            .type_name("G1Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G2AFFINE_COMPRESSED_BYTES]>()
                            .name("beta_g2")
                            .type_name("G2Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G2AFFINE_COMPRESSED_BYTES]>()
                            .name("gamma_g2")
                            .type_name("G2Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G1AFFINE_COMPRESSED_BYTES]>()
                            .name("delta_g1")
                            .type_name("G1Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G2AFFINE_COMPRESSED_BYTES]>()
                            .name("delta_g2")
                            .type_name("G2Affine")
                    })
                    .field(|f| {
                        f.ty::<Vec<[u8; G1AFFINE_COMPRESSED_BYTES]>>()
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
                        f.ty::<[u8; G1AFFINE_COMPRESSED_BYTES]>()
                            .name("a")
                            .type_name("G1Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G2AFFINE_COMPRESSED_BYTES]>()
                            .name("b")
                            .type_name("G2Affine")
                    })
                    .field(|f| {
                        f.ty::<[u8; G1AFFINE_COMPRESSED_BYTES]>()
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
        proof.into_bytes(bytes_regular.as_mut_slice()).unwrap();
        assert_eq!(bytes_regular.as_slice(), bytes_scale.as_slice());
    }

    #[test]
    fn decodes_production_1gib_porep_verifying_key() {
        let vk_bytes = include_bytes!("../../../../examples/1GiB.porep.vk.scale");
        // decode expects &mut mutability
        let vk_bytes = vk_bytes.to_vec();
        let key = VerifyingKey::<Bls12>::decode(&mut vk_bytes.as_slice());

        assert!(key.is_ok(), "failed to parse 1GiB PoRep verifying key");
    }

    #[test]
    fn decodes_production_1gib_post_verifying_key() {
        let vk_bytes = include_bytes!("../../../../examples/1GiB.post.vk.scale");
        // decode expects &mut mutability
        let vk_bytes = vk_bytes.to_vec();
        let key = VerifyingKey::<Bls12>::decode(&mut vk_bytes.as_slice());

        assert!(key.is_ok(), "failed to parse 1GiB PoSt verifying key");
    }

    #[test]
    fn decodes_production_8mib_porep_verifying_key() {
        let vk_bytes = include_bytes!("../../../../examples/8MiB.porep.vk.scale");
        // decode expects &mut mutability
        let vk_bytes = vk_bytes.to_vec();
        let key = VerifyingKey::<Bls12>::decode(&mut vk_bytes.as_slice());

        assert!(key.is_ok(), "failed to parse 8MiB PoRep verifying key");
    }

    #[test]
    fn decodes_production_8mib_post_verifying_key() {
        let vk_bytes = include_bytes!("../../../../examples/8MiB.post.vk.scale");
        // decode expects &mut mutability
        let vk_bytes = vk_bytes.to_vec();
        let key = VerifyingKey::<Bls12>::decode(&mut vk_bytes.as_slice());

        assert!(key.is_ok(), "failed to parse 8MiB PoSt verifying key");
    }
}
