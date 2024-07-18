///! Runtime API extracted from SCALE-encoded runtime.

#[subxt::subxt(runtime_metadata_path = "../../artifacts/metadata.scale")]
mod polka_storage_runtime {}

pub use polka_storage_runtime::*;
