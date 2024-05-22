use std::io::{Cursor, Seek};

use byteorder::ReadBytesExt;
use integer_encoding::{VarIntAsyncReader, VarIntReader};
use ipld_core::{
    cid::{multihash::Multihash, Cid},
    codec::Codec,
};
use serde_ipld_dagcbor::codec::DagCborCodec;
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::{v1::Header, v2::PRAGMA, Error};

// `bytes::Buf` might be more useful here
// https://docs.rs/bytes/1.6.0/bytes/buf/trait.Buf.html
fn read_cid_v1<B>(cursor: &mut Cursor<B>) -> Result<Cid, Error>
where
    B: AsRef<[u8]>,
{
    let cid_version: u64 = cursor.read_varint()?;
    debug_assert_eq!(cid_version, 1);
    let codec_code: u64 = cursor.read_varint()?;

    let hash_function_code: u64 = cursor.read_varint()?;
    let hash_digest_size: usize = cursor.read_varint()?;

    let cursor_position = cursor.position() as usize;
    let hash_digest_slice =
        &cursor.get_ref().as_ref()[cursor_position..(cursor_position + hash_digest_size)];

    // At this point, the cursor holds an allocated buffer
    // Reading into a new buffer would require the new buffer to be allocated
    // Taking the slice directly from the inner buffer and setting the position avoids that allocation
    let multihash = Multihash::wrap(hash_function_code, hash_digest_slice)?;
    cursor.set_position((cursor_position + hash_digest_size) as u64);

    Ok(Cid::new_v1(codec_code, multihash))
}

fn read_cid<B>(cursor: &mut Cursor<B>) -> Result<Cid, Error>
where
    B: AsRef<[u8]>,
{
    // Attempt to read it as a CIDv0 first
    let cid_v0_hash_code = cursor.read_u8()? as u64;
    let cid_v0_hash_length = cursor.read_u8()? as usize;

    if cid_v0_hash_code == 0x12 && cid_v0_hash_length == 0x20 {
        let cursor_position = cursor.position() as usize;
        let multihash = Multihash::wrap(
            cid_v0_hash_code,
            &cursor.get_ref().as_ref()[cursor_position..(cursor_position + cid_v0_hash_length)],
        )?;
        return Ok(Cid::new_v0(multihash)?);
    }

    // Couldn't read a CIDv0, means it is V1
    // rewind to regain the two bytes we read
    cursor.rewind()?;

    read_cid_v1(cursor)
}

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
        return Err(Error::CarV2Error);
    }

    let header: Header = DagCborCodec::decode_from_slice(&header_buffer)?;
    debug_assert!(header.version == 1, "header version is not 1");
    debug_assert!(!header.roots.is_empty(), "header does not have roots");
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
    let cid = read_cid(&mut full_block_cursor)?;

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
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R> Reader<R>
where
    R: AsyncRead + Unpin,
{
    /// Read an [`Header`].
    pub async fn read_header(&mut self) -> Result<Header, Error> {
        read_header(&mut self.reader).await
    }

    /// Reads a block.
    ///
    /// A block is composed of a CID (either version 0 or 1) and data, it is prefixed with the data length.
    /// Below you can see a diagram:
    /// ```text
    /// ┌──────────────────────┬─────┬────────────────────────┐
    /// │ Data length (varint) │ CID │ Data block (raw bytes) │
    /// └──────────────────────┴─────┴────────────────────────┘
    /// ```
    /// The data block is returned AS IS, callers should use the codec field of the [`Cid`] to parse it.
    pub async fn read_block(&mut self) -> Result<(Cid, Vec<u8>), Error> {
        read_block(&mut self.reader).await
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use ipld_core::cid::{Cid, Version};
    use sha2::Sha256;
    use tokio::{fs::File, io::BufReader};

    use crate::{
        multihash::generate_multihash,
        v1::reader::{read_cid, Reader},
        Error,
    };

    const RAW_CODEC: u64 = 0x55;

    #[test]
    fn read_cid_v0_roundtrip() {
        let contents = std::fs::read("tests/fixtures/original/lorem.txt").unwrap();
        let contents_multihash = generate_multihash::<Sha256>(&contents);
        let contents_cid = Cid::new_v0(contents_multihash).unwrap();
        let encoded_cid = contents_cid.to_bytes();
        let mut cursor = Cursor::new(encoded_cid);

        let cid = read_cid(&mut cursor).unwrap();
        assert_eq!(cid.version(), Version::V0);
        assert_eq!(cid.hash(), &contents_multihash);
    }

    #[test]
    fn read_cid_v1_roundtrip() {
        let contents = std::fs::read("tests/fixtures/original/lorem.txt").unwrap();
        let contents_multihash = generate_multihash::<Sha256>(&contents);
        let contents_cid = Cid::new_v1(RAW_CODEC, contents_multihash);
        let encoded_cid = contents_cid.to_bytes();
        let mut cursor = Cursor::new(encoded_cid);

        let cid = super::read_cid_v1(&mut cursor).unwrap();
        assert_eq!(cid.version(), Version::V1);
        assert_eq!(cid.hash(), &contents_multihash);
    }

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
        assert!(matches!(header, Err(Error::CarV2Error)));
    }

    // TODO(@jmg-duarte,19/05/2024): add more tests
}
