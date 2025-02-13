#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(any(feature = "runtime-benchmarks", test))]
mod accounts;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(any(feature = "runtime-benchmarks", test))]
mod deal_proposals;

#[cfg(all(feature = "runtime-benchmarks", test))]
pub(crate) mod mock;

#[cfg(any(feature = "runtime-benchmarks", test))]
mod pallet;

#[cfg(feature = "runtime-benchmarks")]
pub use pallet::*;
