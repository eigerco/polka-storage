mod index;
mod reader;
mod writer;

use bitflags::bitflags;
pub use index::{Index, IndexEntry, MultiWidthIndex, MultihashIndexSorted, SingleWidthIndex};
pub use reader::Reader;
pub use writer::Writer;

/// The pragma for a CARv2. This is also a valid CARv1 header, with version 2 and no root CIDs.
pub const PRAGMA: [u8; 11] = [
    0x0a, // unit(10)
    0xa1, // map(1)
    0x67, // string(7)
    0x76, 0x65, 0x72, 0x73, 0x69, 0x6f, 0x6e, // "version"
    0x02, // uint(2)
];

bitflags! {
    /// Characteristics of the enclosed data.
    pub struct Characteristics: u128 {
        const FULLY_INDEXED = 1 << 127;
    }
}

impl Characteristics {
    /// Create a new [`Characteristics`].
    pub fn new(fully_indexed: bool) -> Self {
        if fully_indexed {
            Self::FULLY_INDEXED
        } else {
            Self::empty()
        }
    }

    /// Check whether the `fully-indexed` characteristic is set.
    #[inline]
    pub const fn is_fully_indexed(&self) -> bool {
        self.intersects(Self::FULLY_INDEXED)
    }
}

/// Low-level CARv2 header.
pub struct Header {
    /// Describes certain features of the enclosed data.
    characteristics: Characteristics,
    /// Byte-offset from the beginning of the CARv2 pragma to the first byte of the CARv1 data payload.
    data_offset: u64,
    /// Byte-length of the CARv1 data payload.
    data_size: u64,
    /// Byte-offset from the beginning of the CARv2 pragma to the first byte of the index payload.
    /// This value may be 0 to indicate the absence of index data.
    index_offset: u64,
}

impl Header {
    pub fn new(fully_indexed: bool, data_offset: u64, data_size: u64, index_offset: u64) -> Self {
        Self {
            characteristics: Characteristics::new(fully_indexed),
            data_offset,
            data_size,
            index_offset,
        }
    }
}

// TODO(@jmg-duarte,22/05/2024): add roundtrip tests
