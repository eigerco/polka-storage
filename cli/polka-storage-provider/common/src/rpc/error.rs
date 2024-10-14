use std::fmt::Display;

use jsonrpsee::types::{
    error::{INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE},
    ErrorObjectOwned,
};
use serde_json::Value;

/// Error type for RPC errors (client and server).
#[derive(Debug)]
pub struct RpcError {
    inner: ErrorObjectOwned,
}

impl std::error::Error for RpcError {}

impl Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error: {}", &self.inner)
    }
}

impl From<RpcError> for ErrorObjectOwned {
    fn from(err: RpcError) -> Self {
        err.inner
    }
}

impl RpcError {
    pub fn new(code: i32, message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self {
            inner: ErrorObjectOwned::owned(code, message.to_string(), data.into()),
        }
    }

    /// Construct an error with [`jsonrpsee::types::error::INTERNAL_ERROR_CODE`].
    pub fn internal_error(message: impl Display, data: impl Into<Option<Value>>) -> Self {
        // NOTE(@jmg-duarte,01/10/2024): this error is actually wrong!
        // the right kind of error has an arbitrary code as defined by us,
        // this is made evident by `jsonrpsee::error::ErrorCode::ServerError`
        Self::new(INTERNAL_ERROR_CODE, message, data)
    }

    /// Construct an error with [`jsonrpsee::types::error::INVALID_PARAMS_CODE`].
    pub fn invalid_params(message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self::new(INVALID_PARAMS_CODE, message, data)
    }
}

impl From<subxt::Error> for RpcError {
    fn from(err: subxt::Error) -> Self {
        Self::internal_error(err, None)
    }
}
