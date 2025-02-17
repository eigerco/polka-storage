#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(all(feature = "runtime-benchmarks", test))]
pub(crate) mod mock;

#[cfg(feature = "runtime-benchmarks")]
pub use benchmarking::pallet::*;
