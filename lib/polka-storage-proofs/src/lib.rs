//! This crate implements the data type definitions needed for the Groth16 proof generation and
//! verification. Therefore, all types need to be `std` and `no-std` compatible.
#![cfg_attr(not(feature = "std"), no_std)]

mod groth16;
pub mod porep;
pub mod types;
pub mod padding;

pub use groth16::*;
