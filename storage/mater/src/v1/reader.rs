use std::io::Cursor;

use integer_encoding::VarIntAsyncReader;
use ipld_core::{cid::Cid, codec::Codec};
use serde_ipld_dagcbor::codec::DagCborCodec;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{v1::Header, v2::PRAGMA, Error};

pub(crate) async fn read_header<R>(mut reader: R) -> Result<Header, Error>
where
    R: AsyncRead + Unpin,
{
    let header_length: usize = reader.read_varint_async().await?;
    let mut header_buffer = vec![0; header_length];
    reader.read_exact(&mut header_buffer).await?;

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

pub(crate) async fn read_block<R>(mut reader: R) -> Result<(Cid, Vec<u8>), Error>
where
    R: AsyncRead + Unpin,
{
    let full_block_length: usize = reader.read_varint_async().await?;
    let mut full_block_buffer = vec![0; full_block_length];
    reader.read_exact(&mut full_block_buffer).await?;

    // We're cheating to get Seek
    let mut full_block_cursor = Cursor::new(full_block_buffer);
    let cid = Cid::read_bytes(&mut full_block_cursor)?;

    let data_start_position = full_block_cursor.position() as usize;
    let mut full_block_buffer = full_block_cursor.into_inner();

    // NOTE(@jmg-duarte,19/05/2024): could we avoid getting a new vector here and just drop the beginning?
    Ok((cid, full_block_buffer.split_off(data_start_position)))
}

/// Low-level CARv1 reader.
pub struct Reader<R> {
    reader: R,
}

impl<R> Reader<R> {
    /// Constructs a new [`Reader`].
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R> Reader<R>
where
    R: AsyncRead + Unpin,
{
    /// Read a [`Header`].
    ///
    /// As defined in the [specification constraints](https://ipld.io/specs/transport/car/carv1/#constraints),
    /// this function will return an error if:
    /// * The read header does not have version 1.
    /// * The read header does not have roots.
    ///
    /// For more information, check the [header specification](https://ipld.io/specs/transport/car/carv1/#header).
    pub async fn read_header(&mut self) -> Result<Header, Error> {
        read_header(&mut self.reader).await
    }

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
    pub async fn read_block(&mut self) -> Result<(Cid, Vec<u8>), Error> {
        read_block(&mut self.reader).await
    }
}

#[cfg(test)]
mod tests {
    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::{fs::File, io::BufReader};

    use crate::{multihash::generate_multihash, v1::reader::Reader, Error};

    const RAW_CODEC: u64 = 0x55;

    #[tokio::test]
    async fn header_reader() {
        let contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256>(&contents);
        let contents_cid = Cid::new_v1(RAW_CODEC, contents_multihash);

        let file = File::open("tests/fixtures/car_v1/lorem_header.car")
            .await
            .unwrap();
        let reader = BufReader::new(file);
        let mut reader = Reader::new(reader);
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
        let contents_multihash = generate_multihash::<Sha256>(&contents);
        let contents_cid = Cid::new_v1(RAW_CODEC, contents_multihash);

        let file = File::open("tests/fixtures/car_v1/lorem.car").await.unwrap();
        let reader = BufReader::new(file);
        let mut reader = Reader::new(reader);
        let header = reader.read_header().await.unwrap();

        assert_eq!(header.version, 1);
        assert_eq!(header.roots.len(), 1);
        assert_eq!(header.roots[0], contents_cid);

        let (cid, block) = reader.read_block().await.unwrap();
        assert_eq!(cid, contents_cid);
        assert_eq!(block, contents);
    }

    #[tokio::test]
    async fn v2_header() {
        let file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let mut reader = Reader::new(file);
        let header = reader.read_header().await;
        println!("{:?}", header);
        assert!(matches!(
            header,
            Err(Error::VersionMismatchError {
                expected: 1,
                received: 2
            })
        ));
    }

    // TODO(@jmg-duarte,19/05/2024): add more tests
}
