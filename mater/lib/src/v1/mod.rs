mod reader;
mod writer;

use integer_encoding::VarInt;
use ipld_core::cid::{multihash::Multihash, Cid};
use serde::{Deserialize, Serialize};

use crate::multicodec::{RAW_CODE, SHA_256_CODE};
pub use crate::v1::{
    reader::{CarReader, CarReaderExt},
    writer::CarWriter,
};

/// The SHA256 hash over a 32-byte array filled with zeroes.
const DEFAULT_HASH: [u8; 32] = [
    0x66, 0x68, 0x7a, 0xad, 0xf8, 0x62, 0xbd, 0x77, 0x6c, 0x8f, 0xc1, 0x8b, 0x8e, 0x9f, 0x8e, 0x20,
    0x08, 0x97, 0x14, 0x85, 0x6e, 0xe2, 0x33, 0xb3, 0x90, 0x2a, 0x59, 0x1d, 0x0d, 0x5f, 0x29, 0x25,
];

/// Low-level CARv1 header.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Header {
    /// CAR file version.
    ///
    /// It is always 1, as defined in the
    /// [specification](https://ipld.io/specs/transport/car/carv1/#constraints).
    version: u8,

    /// Root [`Cid`]s for the contained data.
    pub roots: Vec<Cid>,
}

impl Header {
    /// Construct a new [`Header`].
    ///
    /// The version will always be 1, as defined in the
    /// [specification](https://ipld.io/specs/transport/car/carv1/#constraints).
    pub fn new(roots: Vec<Cid>) -> Self {
        Self { version: 1, roots }
    }

    /// The static components of the CBOR encoded [`Header`].
    ///
    /// The following is a CBOR encoded [`Header`] with a single CID:
    /// ```text
    /// A2                                      # map(2)
    ///    65                                   # text(5)
    ///       726F6F7473                        # "roots"
    ///    81                                   # array(1)
    ///       D8 2A                             # tag(42)
    ///          58 25                          # bytes(37)
    ///             00015512206D623B17625E25CBDA46D17AC89C26B3DB63544701E2C0592626320DBEFD515B
    ///    67                                   # text(7)
    ///       76657273696F6E                    # "version"
    ///    01                                   # unsigned(1)
    /// ```
    ///
    /// When calculating the CBOR encoded length, the only thing that changes are the amount of
    /// elements inside the array, as such, the static overhead is everything *but*:
    /// ```text
    ///       D8 2A                             # tag(42)
    ///          58 25                          # bytes(37)
    ///             00015512206D623B17625E25CBDA46D17AC89C26B3DB63544701E2C0592626320DBEFD515B
    /// ```
    const fn cbor_static_overhead() -> usize {
        1 + // map
        1 + // text
        5 + // "roots"
        1 + // array
        1 + // text
        7 + // "version"
        1 // unsigned(1)
    }

    /// The length of the CBOR encoded [`Cid`].
    const fn cbor_cid_encoded_len() -> usize {
        2 + // tag(42)
        2 + // bytes
        37 // <cid>
    }

    /// Returns the encoded length of the header, including the VarInt size prefix.
    /// The size of the [`Header`] when encoded using [`DagCborCodec`](serde_ipld_dagcbor::codec::DagCborCodec).
    ///
    /// The formula is: `overhead + 41 * roots.len()`.
    /// It is based on reversing the CBOR encoding, see an example:
    /// ```text
    /// A2                                      # map(2)
    ///    65                                   # text(5)
    ///       726F6F7473                        # "roots"
    ///    81                                   # array(1)
    ///       D8 2A                             # tag(42)
    ///          58 25                          # bytes(37)
    ///             00015512206D623B17625E25CBDA46D17AC89C26B3DB63544701E2C0592626320DBEFD515B
    ///    67                                   # text(7)
    ///       76657273696F6E                    # "version"
    ///    01                                   # unsigned(1)
    /// ```
    /// In this case we're always doing a single root, so we just use the fixed size: 58
    ///
    /// Is this cheating? Yes. The alternative is to encode the CARv1 header twice.
    /// We can cache it, but for now, this should be better.
    pub fn encoded_len(&self) -> usize {
        let header_encoded_len =
            Self::cbor_static_overhead() + Self::cbor_cid_encoded_len() * self.roots.len();
        header_encoded_len.required_space() + header_encoded_len
    }
}

impl Default for Header {
    /// Creates a "placeholder" [`Header`].
    ///
    /// This is useful when converting a regular file
    /// to a CARv1 file, where you don't know the root beforehand.
    ///
    /// If you need more than one root, please use [`Self::new`] instead.
    // NOTE(@jmg-duarte,29/05/2024): why tf doesn't the previous intradoc link work??
    fn default() -> Self {
        // Multihash::default does not return a multihash with the usually expected length
        // thus, we wrap a default SHA256. We're required to do this because otherwise writing
        // placeholder headers will fail
        let default_multihash =
            Multihash::wrap(SHA_256_CODE, &DEFAULT_HASH).expect("default hash to be valid");
        let default_cid = Cid::new_v1(RAW_CODE, default_multihash);
        Self {
            version: 1,
            roots: vec![default_cid],
        }
    }
}

/// BlockMetadata contains metadata about a block's section in a CAR file/stream.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockMetadata {
    /// Offset to the start of the block in relation to the start of the original buffer.
    pub block_offset: u64,
    /// [`Cid`] of the block.
    pub cid: Cid,
    /// Offset to the start of the block's data in relation to the start of the original buffer.
    pub data_offset_source: u64,
    /// Size of the data section of the block
    pub data_size: u64,
}

impl BlockMetadata {
    /// The length of the encoded block, including the VarInt prefix and CID.
    pub fn encoded_len(&self) -> u64 {
        let len = self.cid.encoded_len() as u64 + self.data_size;
        len.required_space() as u64 + len
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::io::{AsyncWriteExt, BufWriter};

    use crate::{
        multicodec::{generate_multihash, RAW_CODE},
        v1::{writer::CarWriter, Header},
        CarV1Reader,
    };

    #[tokio::test]
    async fn roundtrip_lorem() {
        let file_contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256, _>(&file_contents);
        let root_cid = Cid::new_v1(RAW_CODE, contents_multihash);

        let written_header = Header::new(vec![root_cid]);
        let buffer = Vec::new();
        let mut writer = BufWriter::new(buffer);
        writer.write_header(&written_header).await.unwrap();

        // There's only one block
        writer.write_block(&root_cid, &file_contents).await.unwrap();
        writer.flush().await.unwrap();
        let expected_header = tokio::fs::read("tests/fixtures/car_v1/lorem.car")
            .await
            .unwrap();
        assert_eq!(&expected_header, writer.get_ref());

        let buffer = writer.into_inner();
        let mut reader = Cursor::new(buffer);
        let read_header = reader.read_v1_header().await.unwrap();
        assert_eq!(read_header, written_header);

        let (read_cid, read_block) = reader.read_block().await.unwrap();
        assert_eq!(read_cid, root_cid);
        assert_eq!(read_block, file_contents);
    }
}
