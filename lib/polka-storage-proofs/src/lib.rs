//! This crate implements the data type definitions needed for the Groth16 proof generation and
//! verification. Therefore, all types need to be `std` and `no-std` compatible.
#![cfg_attr(not(feature = "std"), no_std)]

mod groth16;
pub mod post;
pub mod types;

pub use groth16::*;

#[cfg(feature = "std")]
pub mod porep;

/// Reference:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/api/post_util.rs#L217>
pub fn get_partitions_for_window_post(
    total_sector_count: usize,
    sector_count: usize,
) -> Option<usize> {
    let partitions = total_sector_count.div_ceil(sector_count);
    (partitions > 1).then_some(partitions)
}
