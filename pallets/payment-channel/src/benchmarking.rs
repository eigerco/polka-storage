//! Benchmarking setup for pallet-payment-channel
#![cfg(feature = "runtime-benchmarks")]
use super::*;

#[allow(unused)]
use crate::Pallet as Template;
use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
    use super::*;

    // TODO(Serhii, no-ref, 2024-06-22): Add benchmarks.
}
