//! This module implements utilities for [`BoundedVec`](runtime_types::bounded_collections::bounded_vec::BoundedVec),
//! such as conversion traits and others.

use cid::CidGeneric;

use super::runtime_types::bounded_collections::bounded_vec;

/// Trait to convert `T` into a bounded vector of bytes.
///
/// Due to Rust's orphan rule, we cannot implement `Into<BoundedVec<u8>> for T`
/// where we don't own `T`, which turns out to be most of the useful cases;
/// much like [`Cid`] or [`String`], which don't have official representations in Substrate.
pub(crate) trait IntoBoundedByteVec {
    /// Convert [`Self`] into a bounded vector of bytes.
    fn into_bounded_byte_vec(self) -> bounded_vec::BoundedVec<u8>;
}

impl IntoBoundedByteVec for CidGeneric<64> {
    fn into_bounded_byte_vec(self) -> bounded_vec::BoundedVec<u8> {
        bounded_vec::BoundedVec(self.to_bytes())
    }
}

impl IntoBoundedByteVec for String {
    fn into_bounded_byte_vec(self) -> bounded_vec::BoundedVec<u8> {
        bounded_vec::BoundedVec(self.into_bytes())
    }
}
