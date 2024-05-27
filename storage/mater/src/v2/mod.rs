mod index;
mod reader;
mod writer;

use bitflags::bitflags;
pub use index::{Index, IndexEntry, IndexSorted, MultihashIndexSorted, SingleWidthIndex};
pub use reader::Reader;
pub use writer::Writer;

/// The pragma for a CARv2. This is also a valid CARv1 header, with version 2 and no root CIDs.
///
/// For more information, check the specification: <https://ipld.io/specs/transport/car/carv2/#pragma>
pub const PRAGMA: [u8; 11] = [
    0x0a, // unit(10)
    0xa1, // map(1)
    0x67, // string(7)
    0x76, 0x65, 0x72, 0x73, 0x69, 0x6f, 0x6e, // "version"
    0x02, // uint(2)
];

bitflags! {
    /// Characteristics of the enclosed data.
    #[derive(Debug, PartialEq, Eq)]
    pub struct Characteristics: u128 {
        const EMPTY = 0;
        const FULLY_INDEXED = 1 << 127;
    }
}

impl Characteristics {
    /// Create a new [`Characteristics`].
    pub fn new(fully_indexed: bool) -> Self {
        if fully_indexed {
            Self::FULLY_INDEXED
        } else {
            Self::EMPTY
        }
    }

    /// Check whether the `fully-indexed` characteristic is set.
    #[inline]
    pub const fn is_fully_indexed(&self) -> bool {
        self.intersects(Self::FULLY_INDEXED)
    }
}

impl Default for Characteristics {
    fn default() -> Self {
        Self::EMPTY
    }
}

/// Low-level CARv2 header.
#[derive(Debug, PartialEq, Eq)]
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
    /// Construct a new [`Header`].
    pub fn new(fully_indexed: bool, data_offset: u64, data_size: u64, index_offset: u64) -> Self {
        Self {
            characteristics: Characteristics::new(fully_indexed),
            data_offset,
            data_size,
            index_offset,
        }
    }

    /// The [`Header`] size in bytes (includes the pragma).
    ///
    /// As defined in the [specification](https://ipld.io/specs/transport/car/carv2/#header).
    pub const fn size() -> usize {
        PRAGMA.len() + 40
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, io::Cursor};

    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::io::{AsyncSeekExt, BufWriter};

    use crate::{
        multicodec::{generate_multihash, MultihashCode, RAW_CODE},
        v2::{
            index::{Index, IndexEntry, IndexSorted},
            test_utils::assert_buffer_eq,
            Header, Reader, Writer,
        },
    };

    #[tokio::test]
    async fn roundtrip_lorem() {
        let cursor = Cursor::new(vec![]);
        let buf_writer = BufWriter::new(cursor);
        let mut writer = Writer::new(buf_writer);

        let file_contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256, _>(&file_contents);
        let root_cid = Cid::new_v1(RAW_CODE, contents_multihash);

        let written_header = Header::new(false, 51, 7661, 7712);
        // To simplify testing, the values were extracted using `car inspect`
        writer.write_header(&written_header).await.unwrap();

        // We start writing the CARv1 here and keep the stream positions
        // so that we can properly index the blocks later
        let start_car_v1 = {
            let inner = writer.get_inner_mut();
            inner.stream_position().await.unwrap()
        };

        let written_header_v1 = crate::v1::Header::new(vec![root_cid]);
        writer.write_v1_header(&written_header_v1).await.unwrap();

        let start_car_v1_data = {
            let inner = writer.get_inner_mut();
            inner.stream_position().await.unwrap()
        };

        // There's only one block
        writer.write_block(&root_cid, &file_contents).await.unwrap();

        let written = {
            let inner = writer.get_inner_mut();
            inner.stream_position().await.unwrap()
        };
        assert_eq!(written, 7712);

        let mut mapping = BTreeMap::new();
        mapping.insert(
            Sha256::CODE,
            IndexSorted::from(IndexEntry::new(
                root_cid.hash().digest().to_vec(),
                // This detail is "hidden" in the spec even though it's SO IMPORTANT
                // See: https://ipld.io/specs/transport/car/carv2/#format-0x0400-indexsorted
                // > Individual index entries are the concatenation of the hash digest
                // > an an additional 64-bit unsigned little-endian integer indicating
                // > the offset of the block from the begining of the CARv1 data payload.
                start_car_v1_data - start_car_v1,
            )),
        );
        let written_index = Index::multihash(mapping);
        writer.write_index(&written_index).await.unwrap();

        let mut buffer = writer.finish().await.unwrap().into_inner();
        buffer.rewind().await.unwrap();
        let expected_header = tokio::fs::read("tests/fixtures/car_v2/lorem.car")
            .await
            .unwrap();

        assert_buffer_eq(&expected_header, buffer.get_ref());

        let mut reader = Reader::new(buffer);
        reader.read_pragma().await.unwrap();
        let read_header = reader.read_header().await.unwrap();
        assert_eq!(read_header, written_header);
        let read_header_v1 = reader.read_v1_header().await.unwrap();
        assert_eq!(read_header_v1, written_header_v1);
        let (read_cid, read_block) = reader.read_block().await.unwrap();
        assert_eq!(read_cid, root_cid);
        assert_eq!(read_block, file_contents);
        let read_index = reader.read_index().await.unwrap();
        assert_eq!(read_index, written_index);
    }
}

// NOTE(@jmg-duarte,23/05/2024): I'm looking for better alternatives to this
#[cfg(test)]
pub(crate) mod test_utils {
    /// Check if two given slices are equal.
    ///
    /// First checks if the two slices have the same size,
    /// then checks each byte-pair. If the slices differ,
    /// it will show an error message with the difference index
    /// along with a window showing surrounding elements
    /// (instead of spamming your terminal like `assert_eq!` does).
    pub fn assert_buffer_eq(lhs: &[u8], rhs: &[u8]) {
        // NOTE(@jmg-duarte,23/05/2024): Unsure if instead of a function, this should be a macro_rules!
        assert_eq!(lhs.len(), rhs.len());
        for (i, (l, r)) in lhs.iter().zip(rhs).enumerate() {
            let before = i.checked_sub(5).unwrap_or(0);
            let after = (i + 5).min(rhs.len());
            assert_eq!(
                l,
                r,
                "difference at index {}\n  left: {:02x?}\n right: {:02x?}",
                i,
                &lhs[before..=after],
                &rhs[before..=after],
            )
        }
    }
}
