#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
mod accounts;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;

#[cfg(feature = "runtime-benchmarks")]
mod deal_proposals;

#[cfg(all(feature = "runtime-benchmarks", test))]
pub(crate) mod mock;

#[cfg(feature = "runtime-benchmarks")]
mod pallet;

#[cfg(feature = "runtime-benchmarks")]
pub use pallet::*;
