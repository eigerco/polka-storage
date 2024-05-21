mod reader;
mod writer;

use ipld_core::cid::Cid;
use serde::{Deserialize, Serialize};

pub use crate::car::v1::{reader::Reader, writer::Writer};
pub(crate) use crate::car::v1::{
    reader::{read_block, read_header},
    writer::{write_block, write_header},
};

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
