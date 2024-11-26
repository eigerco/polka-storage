mod reader;
mod writer;

use ipld_core::cid::{multihash::Multihash, Cid};
use serde::{Deserialize, Serialize};

use crate::multicodec::{RAW_CODE, SHA_256_CODE};
pub use crate::v1::{reader::Reader, writer::Writer};
pub(crate) use crate::v1::{
    reader::{read_block, read_header},
    writer::{write_block, write_header},
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

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::io::BufWriter;

    use crate::{
        multicodec::{generate_multihash, RAW_CODE},
        v1::{Header, Reader, Writer},
    };

    impl Writer<BufWriter<Vec<u8>>> {
        pub fn test_writer() -> Self {
            let buffer = Vec::new();
            let buf_writer = BufWriter::new(buffer);
            Writer::new(buf_writer)
        }
    }

    #[tokio::test]
    async fn roundtrip_lorem() {
        let file_contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256, _>(&file_contents);
        let root_cid = Cid::new_v1(RAW_CODE, contents_multihash);

        let written_header = Header::new(vec![root_cid]);
        let mut writer = crate::v1::Writer::test_writer();
        writer.write_header(&written_header).await.unwrap();

        // There's only one block
        writer.write_block(&root_cid, &file_contents).await.unwrap();
        let buf_writer = writer.finish().await.unwrap();
        let expected_header = tokio::fs::read("tests/fixtures/car_v1/lorem.car")
            .await
            .unwrap();
        assert_eq!(&expected_header, buf_writer.get_ref());

        let buffer = buf_writer.into_inner();
        let mut reader = Reader::new(Cursor::new(buffer));
        let read_header = reader.read_header().await.unwrap();
        assert_eq!(read_header, written_header);

        let (read_cid, read_block) = reader.read_block().await.unwrap();
        assert_eq!(read_cid, root_cid);
        assert_eq!(read_block, file_contents);
    }
}
