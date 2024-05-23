mod reader;
mod writer;

use ipld_core::cid::Cid;
use serde::{Deserialize, Serialize};

pub use crate::v1::{reader::Reader, writer::Writer};
pub(crate) use crate::v1::{
    reader::{read_block, read_header},
    writer::{write_block, write_header},
};

/// Low-level CARv1 header.
#[derive(Debug, Serialize, Deserialize)]
pub struct Header {
    /// CAR file version.
    ///
    /// It is always 1, as defined in the
    /// [specification](https://ipld.io/specs/transport/car/carv1/#constraints).
    version: u8,

    /// Root [`Cid`](`ipld_core::cid::Cid`)s for the contained data.
    pub roots: Vec<Cid>,
}

impl Header {
    /// Construct a new [`CarV1Header`](`crate::v1::Header`).
    ///
    /// The version will always be 1, as defined in the
    /// [specification](https://ipld.io/specs/transport/car/carv1/#constraints).
    pub fn new(roots: Vec<Cid>) -> Self {
        Self { version: 1, roots }
    }
}
