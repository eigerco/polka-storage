use thiserror::Error;

/// CLI components error handling implementor.
#[derive(Debug, Error)]
pub enum Error {
    // TODO(@serhii, no-ref, 2024-05-28): Add and extend with error variants required in the `polka-storage` or `polka-storage-provider` crates.
}
