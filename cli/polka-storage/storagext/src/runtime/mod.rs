//! This module covers the Runtime API extracted from SCALE-encoded runtime and extra goodies
//! to interface with the runtime.
//!
//! This module wasn't designed to be exposed to the final user of the crate.

pub(crate) mod bounded_vec;

#[subxt::subxt(
    runtime_metadata_path = "../../artifacts/metadata.scale",
    substitute_type(
        path = "sp_runtime::MultiSignature",
        with = "::subxt::utils::Static<::frame_support::sp_runtime::MultiSignature>"
    ),
    derive_for_all_types = "Clone"
)]
mod polka_storage_runtime {}

// Using self keeps the import separate from the others
pub use self::polka_storage_runtime::*;
