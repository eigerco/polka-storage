use std::fmt::Display;

use jsonrpsee::types::{
    error::{INTERNAL_ERROR_CODE, INVALID_PARAMS_CODE},
    ErrorObjectOwned,
};
use serde_json::Value;

/// Error type for RPC server errors.
#[derive(Debug)]
pub struct ServerError {
    inner: ErrorObjectOwned,
}

impl std::error::Error for ServerError {}

impl Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error: {}", &self.inner)
    }
}

impl From<ServerError> for ErrorObjectOwned {
    fn from(err: ServerError) -> Self {
        err.inner
    }
}

impl ServerError {
    pub fn new(code: i32, message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self {
            inner: ErrorObjectOwned::owned(code, message.to_string(), data.into()),
        }
    }

    pub fn internal_error(message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self::new(INTERNAL_ERROR_CODE, message, data)
    }

    pub fn invalid_params(message: impl Display, data: impl Into<Option<Value>>) -> Self {
        Self::new(INVALID_PARAMS_CODE, message, data)
    }
}

impl From<subxt::Error> for ServerError {
    fn from(err: subxt::Error) -> Self {
        Self::internal_error(err, None)
    }
}
