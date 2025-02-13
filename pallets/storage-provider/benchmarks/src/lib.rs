#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub mod inner;

#[cfg(all(feature = "runtime-benchmarks", test))]
pub(crate) mod mock;

#[cfg(any(feature = "runtime-benchmarks", test))]
mod utils;

#[cfg(any(feature = "runtime-benchmarks", test))]
mod pallet;

#[cfg(feature = "runtime-benchmarks")]
pub use pallet::*;
