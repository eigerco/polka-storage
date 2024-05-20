mod reader;
mod writer;

use ipld_core::cid::Cid;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use crate::car::v1::reader::Reader;
pub use crate::car::v1::writer::Writer;

pub(crate) use crate::car::v1::reader::{read_block, read_header};
pub(crate) use crate::car::v1::writer::{write_block, write_header};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    CodecError(#[from] serde_ipld_dagcbor::error::CodecError),
    #[error(transparent)]
    IoError(#[from] tokio::io::Error),
    #[error(transparent)]
    CidError(#[from] ipld_core::cid::Error),
    #[error(transparent)]
    MultihashError(#[from] ipld_core::cid::multihash::Error),
    #[error("trying to read V2")]
    CarV2Error,
}

/// Low-level CARv1 header.
#[derive(Debug, Serialize, Deserialize)]
pub struct Header {
    pub version: u8,
    pub roots: Vec<Cid>,
}

impl Header {
    /// Construct a new CARv1 header.
    ///
    /// The version will always be 1.
    pub fn new(roots: Vec<Cid>) -> Self {
        Self { version: 1, roots }
    }
}
