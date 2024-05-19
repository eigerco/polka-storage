use std::io::{Cursor, Seek};

use byteorder::ReadBytesExt;
use ipld_core::{
    cid::{multihash::Multihash, Cid},
    codec::Codec,
};

use integer_encoding::{VarIntAsyncReader, VarIntAsyncWriter, VarIntReader};
use serde::{Deserialize, Serialize};
use serde_ipld_dagcbor::codec::DagCborCodec;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

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
}

/// Low-level CARv1 header.
#[derive(Debug, Serialize, Deserialize)]
pub struct Header {
    version: u8,
    roots: Vec<Cid>,
}

impl Header {
    /// Construct a new CARv1 header.
    ///
    /// The version will always be 1.
    pub fn new(roots: Vec<Cid>) -> Self {
        Self { version: 1, roots }
    }
}

/// Write a CARv1 header to the provider writer.
pub(crate) async fn write_header<W>(writer: &mut W, header: &Header) -> Result<(), Error>
where
    W: AsyncWrite + Unpin,
{
    let encoded_header = DagCborCodec::encode_to_vec(header)?;
    writer.write_varint_async(encoded_header.len()).await?;
    writer.write_all(&encoded_header).await?;
    Ok(())
}

/// Write a [`Cid`] and block to the given writer.
///
/// This is a low-level function to be used in the implementation of CAR writers.
/// The function takes a `cid_buffer` to avoid allocating a new buffer every time it is called.
pub(crate) async fn write_block<W, Block>(
    writer: &mut W,
    cid: &Cid,
    block: Block,
    mut cid_buffer: &mut Vec<u8>,
) -> Result<(), Error>
where
    W: AsyncWrite + Unpin,
    Block: AsRef<[u8]>,
{
    let written = cid.write_bytes(&mut cid_buffer)?;
    debug_assert!(written == cid.encoded_len(), "failed to write the full Cid");
    let data = block.as_ref();
    let len = written + data.len();

    writer.write_varint_async(len).await?;
    // NOTE(@jmg-duarte,17/05/2024): this .as_mut() could be "avoided" if O: AsRef<[u8]>
    // though we're achieving the same result regardless...
    writer.write_all(&cid_buffer[..written]).await?;
    writer.write_all(&data).await?;
    Ok(())
}

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

/// Low-level CARv1 reader.
pub struct Reader<R> {
    reader: R,
}

impl<R> Reader<R> {
    fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R> Reader<R>
where
    R: AsyncRead + Unpin,
{
    /// Read an [`Header`].
    async fn read_header(&mut self) -> Result<Header, Error> {
        let header_length: usize = self.reader.read_varint_async().await?;
        let mut header_buffer = Vec::with_capacity(header_length);
        self.reader.read_buf(&mut header_buffer).await?;
        let header: Header = DagCborCodec::decode_from_slice(&header_buffer)?;
        debug_assert!(header.version == 1, "header version is not 1");
        debug_assert!(!header.roots.is_empty(), "header does not have roots");
        Ok(header)
    }

    /// Reads a data block, composed of a [`Cid`] and the remaining data.
    ///
    /// The remaining data is returned AS IS, callers should use the coded
    /// described in the returned [`Cid`] to parse it.
    ///
    /// A block is composed of a CID (either version 0 or 1) and data.
    /// ```text
    ///       | CID                                                               | Data block |
    /// CIDv0 | hash function code (byte 0x12) | digest size (byte 0x20) | digest | ********** |
    /// CIDv1 | hash function code (varint)    | digest size (varint)    | digest | ********** |
    /// ```
    async fn read_block(&mut self) -> Result<(Cid, Vec<u8>), Error> {
        let full_block_length: usize = self.reader.read_varint_async().await?;
        let mut full_block_buffer = Vec::with_capacity(full_block_length);
        // Read the full block to save on later allocations
        self.reader.read_buf(&mut full_block_buffer).await?;

        let mut full_block_cursor = Cursor::new(full_block_buffer);
        let cid = read_cid(&mut full_block_cursor)?;

        let data_start_position = full_block_cursor.position() as usize;
        let mut full_block_buffer = full_block_cursor.into_inner();

        Ok((cid, full_block_buffer.split_off(data_start_position)))
    }
}

/// Low-level CARv1 writer.
pub struct Writer<W> {
    writer: W,
    was_header_written: bool,
    /// Avoids allocating a new buffer each time we write a [`Cid`].
    cid_buffer: Vec<u8>,
}

impl<W> Writer<W> {
    /// Construct a new CARv1 writer.
    ///
    /// Takes a writer into which the data will be written.
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            was_header_written: false,
            cid_buffer: Vec::new(),
        }
    }
}

impl<W> Writer<W>
where
    W: AsyncWrite + Unpin,
{
    /// Write a CARv1 header.
    ///
    /// * If the header has already been written, this is a no-op.
    pub async fn write_header(&mut self, header: &Header) -> Result<(), Error> {
        if !self.was_header_written {
            write_header(&mut self.writer, header).await?;
            self.was_header_written = true;
        }
        Ok(())
    }

    /// Write a [`Cid`] and the respective data block.
    pub async fn write_block<D>(&mut self, cid: &Cid, data: &D) -> Result<(), Error>
    where
        D: AsRef<[u8]>,
    {
        write_block(&mut self.writer, cid, data, &mut self.cid_buffer).await
    }

    /// Flushes and returns the inner writer.
    pub async fn finish(mut self) -> Result<W, Error> {
        self.writer.flush().await?;
        Ok(self.writer)
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, path::Path};

    use ipld_core::cid::{multihash::Multihash, Cid, Version};
    use sha2::Sha256;
    use tokio::{
        fs::File,
        io::{BufReader, BufWriter},
    };

    use crate::{
        car_v1::{Header, Reader},
        generate_multihash,
    };

    use super::Writer;

    const RAW_CODEC: u64 = 0x55;

    impl Writer<BufWriter<Vec<u8>>> {
        fn test_writer() -> Self {
            let buffer = Vec::new();
            let buf_writer = BufWriter::new(buffer);
            Writer::new(buf_writer)
        }
    }

    async fn file_multihash<P>(path: P) -> Multihash<64>
    where
        P: AsRef<Path>,
    {
        let file_contents = tokio::fs::read(path).await.unwrap();
        generate_multihash::<Sha256>(&file_contents)
    }

    #[test]
    fn read_cid_v0_roundtrip() {
        let contents = std::fs::read("tests/fixtures/text/lorem.txt").unwrap();
        let contents_multihash = generate_multihash::<Sha256>(&contents);
        let contents_cid = Cid::new_v0(contents_multihash).unwrap();
        let encoded_cid = contents_cid.to_bytes();
        let mut cursor = Cursor::new(encoded_cid);

        let cid = super::read_cid(&mut cursor).unwrap();
        assert_eq!(cid.version(), Version::V0);
        assert_eq!(cid.hash(), &contents_multihash);
    }

    #[test]
    fn read_cid_v1_roundtrip() {
        let contents = std::fs::read("tests/fixtures/text/lorem.txt").unwrap();
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
        let contents_multihash = file_multihash("tests/fixtures/text/lorem.txt").await;
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
        let contents = tokio::fs::read("tests/fixtures/text/lorem.txt")
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
    async fn header_writer() {
        let contents_multihash = file_multihash("tests/fixtures/text/lorem.txt").await;
        let root_cid = Cid::new_v1(RAW_CODEC, contents_multihash);

        let mut writer = Writer::test_writer();
        writer
            .write_header(&Header::new(vec![root_cid]))
            .await
            .unwrap();
        let buf_writer = writer.finish().await.unwrap();

        let expected_header = tokio::fs::read("tests/fixtures/car_v1/lorem_header.car")
            .await
            .unwrap();

        assert_eq!(&expected_header, buf_writer.get_ref());
    }

    #[tokio::test]
    async fn full_writer() {
        let file_contents = tokio::fs::read("tests/fixtures/text/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256>(&file_contents);
        let root_cid = Cid::new_v1(RAW_CODEC, contents_multihash);

        let mut writer = Writer::test_writer();
        writer
            .write_header(&Header::new(vec![root_cid]))
            .await
            .unwrap();
        // There's only one block
        writer.write_block(&root_cid, &file_contents).await.unwrap();
        let buf_writer = writer.finish().await.unwrap();

        let expected_header = tokio::fs::read("tests/fixtures/car_v1/lorem.car")
            .await
            .unwrap();
        assert_eq!(&expected_header, buf_writer.get_ref());
    }
}
