//! This module implements utilities for [`BoundedVec`](runtime_types::bounded_collections::bounded_vec::BoundedVec),
//! such as conversion traits and others.

use super::runtime_types::bounded_collections::bounded_vec;

pub trait IntoErasedBoundedVec<T> {
    fn into_erased_bounded_vec(self) -> bounded_vec::BoundedVec<T>;
}

impl<T> IntoErasedBoundedVec<T> for Vec<T> {
    fn into_erased_bounded_vec(self) -> bounded_vec::BoundedVec<T> {
        bounded_vec::BoundedVec(self)
    }
}
