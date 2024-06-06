use std::fmt::Display;

use jsonrpsee::types::{error::INTERNAL_ERROR_CODE, ErrorObjectOwned};
use serde_json::Value;

pub struct ServerError {
    inner: ErrorObjectOwned,
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
}

impl From<subxt::Error> for ServerError {
    fn from(err: subxt::Error) -> Self {
        Self::internal_error(err, None)
    }
}
