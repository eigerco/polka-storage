//! This submodule separates all definitions enabled by feature `substrate`.
#![cfg(feature = "substrate")]

use super::*;

impl<E> ::codec::Decode for VerifyingKey<E>
where
    E: Engine<G1Affine = G1Affine, G2Affine = G2Affine>,
{
    fn decode<I: ::codec::Input>(input: &mut I) -> Result<Self, ::codec::Error> {
        // TODO(@neutrinoks,#395,20/09/2024): check again needed buffer size.
        // We don't know how many `ic` values will be passed.
        // 2784 == 20 * 96 + 864 | see notes above.
        let mut buffer = [0u8; 2784];
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
        let mut buffer = ByteBuffer::new();
        self.encode_to(&mut buffer);
        f(buffer.as_slice())
    }
}

impl ::codec::Output for ByteBuffer {
    fn write(&mut self, bytes: &[u8]) {
        for b in bytes.iter() {
            self.0.push(*b);
        }
    }
}

impl ::codec::Input for ByteBuffer {
    fn remaining_len(&mut self) -> Result<Option<usize>, ::codec::Error> {
        Ok(Some(self.bytes_to_read()))
    }

    fn read(&mut self, into: &mut [u8]) -> Result<(), ::codec::Error> {
        let max = self.bytes_to_read();
        let n = core::cmp::min(into.len(), max);
        self.1 = copy_to_buffer(&mut into[..n], self.1, &self.0[..]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use codec::{Decode, Encode};

    use super::*;

    impl From<Vec<u8>> for ByteBuffer {
        fn from(vec: Vec<u8>) -> ByteBuffer {
            ByteBuffer(vec, 0)
        }
    }

    /// This is a smoke test of the `codec::Encode` and `codec::Decode` implementation.
    #[test]
    fn verifying_key_encode_decode() {
        let vkey = VerifyingKey::<Bls12>::random();
        let vkey_bytes = vkey.encode();
        let mut output = ByteBuffer::from(vkey_bytes);
        assert_eq!(
            vkey,
            VerifyingKey::decode(&mut output).expect("VerifyingKey::decode failed")
        );
    }
}
