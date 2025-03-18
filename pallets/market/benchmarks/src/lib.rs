#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(any(feature = "runtime-benchmarks", test))]
pub(crate) mod mock;

#[cfg(any(feature = "runtime-benchmarks", test))]
mod pallet;

#[cfg(any(feature = "runtime-benchmarks", test))]
pub use pallet::*;

#[cfg(test)]
mod test;
