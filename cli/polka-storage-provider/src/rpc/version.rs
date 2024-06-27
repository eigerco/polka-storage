use private::Sealed;

/// Sealed trait to prevent external implementations.
mod private {
    pub trait Sealed {}
}

/// RPC API version.
pub trait ApiVersion: Sealed {
    /// Returns the version string.
    fn version() -> &'static str;
}

/// RPC API version v0.
pub struct V0;

impl Sealed for V0 {}

impl ApiVersion for V0 {
    fn version() -> &'static str {
        "rpc/v0"
    }
}
