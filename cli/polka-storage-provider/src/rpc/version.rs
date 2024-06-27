use sealed::sealed;

/// RPC API version.
#[sealed]
pub trait ApiVersion {
    /// Returns the version string.
    fn version() -> &'static str;
}

/// RPC API version v0.
pub struct V0;

#[sealed]
impl ApiVersion for V0 {
    fn version() -> &'static str {
        "rpc/v0"
    }
}
