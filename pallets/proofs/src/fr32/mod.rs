use bls12_381::Scalar as Fr;
use ff::PrimeField;

/// Converts a slice of 32 bytes (little-endian, non-Montgomery form) into an `Fr::Repr` by
/// zeroing the most signficant two bits of `le_bytes`.
///
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/fr32/src/convert.rs#L35>
#[inline]
pub fn bytes_into_fr_repr_safe(le_bytes: &[u8; 32]) -> <Fr as PrimeField>::Repr {
    let mut repr = [0u8; 32];
    repr.copy_from_slice(le_bytes);
    repr[31] &= 0b0011_1111;
    repr
}

/// Takes a slice of bytes (little-endian, non-Montgomery form) and returns an Fr if byte slice is
/// exactly 32 bytes and does not overflow, otherwise it returns [`Error::BarFrBytes`].
///
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/fr32/src/convert.rs#L25>
pub fn bytes_into_fr(le_bytes: &[u8; 32]) -> Result<Fr, Error> {
    let mut repr = [0u8; 32];
    repr.copy_from_slice(le_bytes);
    Fr::from_repr_vartime(repr).ok_or_else(|| Error::BadFrBytes)
}

#[derive(core::fmt::Debug)]
pub enum Error {
    /// Bytes could not be converted to Fr
    BadFrBytes,
}
