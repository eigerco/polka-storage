use std::io::SeekFrom;

use ipld_core::{cid::Cid, codec::Codec};
use serde_ipld_dagcbor::codec::DagCborCodec;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt};

use super::BlockMetadata;
use crate::ipld::IpldExt;
use crate::{async_varint::read_varint, v1::Header, v2::PRAGMA, Error};

/// Low-level, reading functions for the CAR format.
pub trait CarReader {
    /// Read a [`Header`].
    ///
    /// As defined in the [specification constraints](https://ipld.io/specs/transport/car/carv1/#constraints),
    /// this function will return an error if:
    /// * The read header does not have version 1.
    /// * The read header does not have roots.
    ///
    /// For more information, check the [header specification](https://ipld.io/specs/transport/car/carv1/#header).
    fn read_header(&mut self) -> impl std::future::Future<Output = Result<Header, Error>>;

    /// Reads a [`Cid`] and a data block.
    ///
    /// A block is composed of a CID (either version 0 or 1) and data, it is prefixed with the data length.
    /// ```text
    /// ┌──────────────────────┬─────┬────────────────────────┐
    /// │ Data length (varint) │ CID │ Data block (raw bytes) │
    /// └──────────────────────┴─────┴────────────────────────┘
    /// ```
    /// *The data block is returned AS IS, callers should use the codec field of the [`Cid`] to parse it.*
    ///
    /// For more information, check the [block specification](https://ipld.io/specs/transport/car/carv1/#data).
    fn read_block(&mut self) -> impl std::future::Future<Output = Result<(Cid, Vec<u8>), Error>>;
}

impl<R> CarReader for R
where
    R: AsyncRead + Unpin,
{
    async fn read_header(&mut self) -> Result<Header, Error> {
        let header_length: usize = read_varint(self).await?.0;
        let mut header_buffer = vec![0; header_length];
        self.read_exact(&mut header_buffer).await?;

        // From the V2 specification:
        // > This 11 byte string remains fixed and may be matched using a
        // > simple byte comparison and does not require a varint or CBOR
        // > decode since it does not vary for the CARv2 format.
        // We're skipping the first byte because we already read the length
        if header_buffer.starts_with(&PRAGMA[1..]) {
            return Err(Error::VersionMismatchError {
                expected: 1,
                received: 2,
            });
        }

        let header: Header = DagCborCodec::decode_from_slice(&header_buffer)?;
        // NOTE(@jmg-duarte,23/05/2024): implementing a custom Deserialize for Header
        // would make this shorter and overall handling more reliable
        if header.version != 1 {
            return Err(Error::VersionMismatchError {
                expected: 1,
                received: header.version,
            });
        }
        if header.roots.is_empty() {
            return Err(Error::EmptyRootsError);
        }
        Ok(header)
    }

    async fn read_block(&mut self) -> Result<(Cid, Vec<u8>), Error> {
        let (full_block_length, _): (u64, _) = read_varint(self).await?;
        let (cid, cid_bytes_read) = self.read_cid().await?;

        let data_size = full_block_length as usize - cid_bytes_read;
        let mut data_buffer = vec![0; data_size];
        self.read_exact(&mut data_buffer).await?;

        Ok((cid, data_buffer))
    }
}

/// Extensions to the core functions in [`CarReader`].
pub trait CarReaderExt: CarReader {
    /// Skips the next block and only returns a [`BlockMetadata`]. This is
    /// useful in cases when we only need the block's metadata and don't care
    /// about the content.
    fn read_block_metadata(
        &mut self,
    ) -> impl std::future::Future<Output = Result<BlockMetadata, Error>>;
}

impl<R> CarReaderExt for R
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    async fn read_block_metadata(&mut self) -> Result<BlockMetadata, Error> {
        let block_offset = self.stream_position().await?;

        // Length of the block. This length contains the length of the cid and data.
        let (full_block_length, varint_bytes_read): (u64, _) = read_varint(self).await?;

        // Cid of the block
        let (cid, cid_bytes_read) = self.read_cid().await?;

        // Data section position and size
        let data_offset_source =
            block_offset + (varint_bytes_read as u64) + (cid_bytes_read as u64);
        let data_size = full_block_length - cid_bytes_read as u64;

        // Skip block data section
        self.seek(SeekFrom::Current(data_size as i64)).await?;

        Ok(BlockMetadata {
            block_offset,
            cid,
            data_offset_source,
            data_size,
        })
    }
}

#[cfg(test)]
mod tests {
    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::{fs::File, io::BufReader};

    use crate::{
        multicodec::{generate_multihash, RAW_CODE},
        v1::reader::{CarReader, CarReaderExt},
        Error,
    };

    #[tokio::test]
    async fn header_reader() {
        let contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256, _>(&contents);
        let contents_cid = Cid::new_v1(RAW_CODE, contents_multihash);

        let file = File::open("tests/fixtures/car_v1/lorem_header.car")
            .await
            .unwrap();
        let mut reader = BufReader::new(file);
        let header = reader.read_header().await.unwrap();

        assert_eq!(header.version, 1);
        assert_eq!(header.roots.len(), 1);
        assert_eq!(header.roots[0], contents_cid);
    }

    #[tokio::test]
    async fn full_reader() {
        let contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256, _>(&contents);
        let contents_cid = Cid::new_v1(RAW_CODE, contents_multihash);

        let file = File::open("tests/fixtures/car_v1/lorem.car").await.unwrap();
        let mut reader = BufReader::new(file);
        let header = reader.read_header().await.unwrap();

        assert_eq!(header.version, 1);
        assert_eq!(header.roots.len(), 1);
        assert_eq!(header.roots[0], contents_cid);

        let (cid, block) = reader.read_block().await.unwrap();
        assert_eq!(cid, contents_cid);
        assert_eq!(block, contents);
    }

    #[tokio::test]
    async fn block_metadata_reader() {
        let contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256, _>(&contents);
        let contents_cid = Cid::new_v1(RAW_CODE, contents_multihash);

        let file = File::open("tests/fixtures/car_v1/lorem.car").await.unwrap();
        let mut reader = BufReader::new(file);
        let header = reader.read_header().await.unwrap();

        assert_eq!(header.version, 1);
        assert_eq!(header.roots.len(), 1);
        assert_eq!(header.roots[0], contents_cid);

        let metadata = reader.read_block_metadata().await.unwrap();
        assert_eq!(metadata.cid, contents_cid);
        assert_eq!(metadata.data_offset_source, 97);
        assert_eq!(metadata.data_size as usize, contents.len());
    }

    #[tokio::test]
    async fn v2_header() {
        let mut file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let header = file.read_header().await;
        println!("{:?}", header);
        assert!(matches!(
            header,
            Err(Error::VersionMismatchError {
                expected: 1,
                received: 2
            })
        ));
    }
}
