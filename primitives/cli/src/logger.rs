/// Statically defined logging channels names (targets).
#[derive(Debug, Copy, Clone)]
pub enum Channel {
    // TODO(@serhii, no-ref, 2024-05-28): Add and extend with error variants required in the `polka-storage` or `polka-storage-provider` crates.
}

impl AsRef<str> for Channel {
    fn as_ref(&self) -> &str {
        // TODO(@serhii, no-ref, 2024-05-28): Add and extend with error variants required in the `polka-storage` or `polka-storage-provider` crates.
        todo!()
    }
}
