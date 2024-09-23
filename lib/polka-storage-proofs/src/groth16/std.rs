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

impl std::io::Read for ByteBuffer {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = std::cmp::min(self.0.len(), buf.len());
        buf.copy_from_slice(&self.0[self.1..self.1 + n]);
        self.1 += n;
        Ok(n)
    }
}

impl std::io::Write for ByteBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.extend_from_slice(buf);
        assert!(buf.len() > 0, "{}", buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

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

/// Method transforms a `blstrs::Scalar` into a `bls12_381::Scalar`.
// TODO: (395,@neutrinoks,20/10/2024): Remove allow dead_code.
#[allow(dead_code)]
fn scalar(scalar: &blstrs::Scalar) -> Result<Scalar, FromBytesError> {
    Scalar::from_bytes(&scalar.to_bytes_le())
        .into_option()
        .ok_or(FromBytesError::ScalarConversion)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn random_bellperson_verifying_key() -> bp_g16::VerifyingKey<blstrs::Bls12> {
        bp_g16::VerifyingKey::<blstrs::Bls12> {
            alpha_g1: blstrs::G1Affine::generator(),
            beta_g1: blstrs::G1Affine::generator(),
            beta_g2: blstrs::G2Affine::generator(),
            gamma_g2: blstrs::G2Affine::generator(),
            delta_g1: blstrs::G1Affine::generator(),
            delta_g2: blstrs::G2Affine::generator(),
            ic: vec![
                blstrs::G1Affine::generator(),
                blstrs::G1Affine::generator(),
                blstrs::G1Affine::generator(),
            ],
        }
    }

    /// This test is about testing the deserialisation of `VerifyingKey` with bytes that have been
    /// serialised from `bellperson::VerifyingKey`.
    #[test]
    fn verifying_key_from_bytes_from_bellperson() {
        // Generate a verifying key with bellperson crate.
        let bp_vkey = random_bellperson_verifying_key();
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
        // Generate a verifying key with bellperson crate.
        let bp_vkey = random_bellperson_verifying_key();
        // Serialise it by using its `Read` implementation.
        let bytes = VERIFYINGKEY_MIN_BYTES + bp_vkey.ic.len() * G1AFFINE_BYTES;
        let mut bytes = ByteBuffer::new_with_size(bytes);
        bp_vkey.write(bytes.0.as_mut_slice()).unwrap();
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
        let mut bytes = ByteBuffer::new_with_size(vkey.serialised_bytes());
        vkey.into_bytes(bytes.as_mut_slice()).unwrap();
        // Deserialise bytes to bellperson's VerifyingKey by using its `Write` implementation.
        let bp_vkey_result = bp_g16::VerifyingKey::<blstrs::Bls12>::read(bytes).unwrap();
        // Compare initial struct with this one.
        assert_eq!(bp_vkey, bp_vkey_result);
    }
}
