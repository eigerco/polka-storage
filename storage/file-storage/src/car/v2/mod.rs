mod index;
mod reader;

pub use index::{Index, MultiWidthIndex, MultihashIndexSorted};

use bitflags::bitflags;
use byteorder::{LittleEndian, WriteBytesExt};
use ipld_core::cid::Cid;
use tokio::io::AsyncWrite;
use tokio::io::AsyncWriteExt;

use crate::car::{self, Error};

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

/// Low-level CARv2 writer.
// TODO(@jmg-duarte,17/05/2024): add padding support
pub struct Writer<W>
where
    W: AsyncWrite + Unpin,
{
    writer: W,
    has_written_header: bool,
    has_written_v1_header: bool,
    cid_buffer: Vec<u8>,
}

impl<W> Writer<W>
where
    W: AsyncWrite + Unpin,
{
    /// Construct a new CARv1 writer.
    ///
    /// Takes a write into which the data will be written.
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            has_written_header: false,
            has_written_v1_header: false,
            cid_buffer: Vec::new(),
        }
    }

    /// Write a CARv2 header.
    ///
    /// * If the header has already been written, this is a no-op.
    pub async fn write_header(&mut self, header: &Header) -> Result<(), Error> {
        if !self.has_written_header {
            self.writer.write(&PRAGMA).await?;

            let mut buffer = [0; 40];
            let mut handle = &mut buffer[..];
            WriteBytesExt::write_u128::<LittleEndian>(&mut handle, header.characteristics.bits())?;
            WriteBytesExt::write_u64::<LittleEndian>(&mut handle, header.data_offset)?;
            WriteBytesExt::write_u64::<LittleEndian>(&mut handle, header.data_size)?;
            WriteBytesExt::write_u64::<LittleEndian>(&mut handle, header.index_offset)?;

            self.writer.write_all(&buffer).await?;
            self.has_written_header = true;
        }
        Ok(())
    }

    /// Write a CARv1 header.
    ///
    /// * If the header has already been written, this is a no-op.
    pub async fn write_v1_header(&mut self, v1_header: &car::v1::Header) -> Result<(), Error> {
        debug_assert!(
            self.has_written_header,
            "CARv2 header has not been written!"
        );
        if !self.has_written_v1_header {
            car::v1::write_header(&mut self.writer, v1_header).await?;
            self.has_written_v1_header = true;
        }
        Ok(())
    }

    /// Write a [`Cid`] and the respective data block.
    pub async fn write_block<Block>(&mut self, cid: &Cid, block: &Block) -> Result<(), Error>
    where
        Block: AsRef<[u8]>,
    {
        debug_assert!(
            self.has_written_header && self.has_written_v1_header,
            "Both headers need to be written!"
        );
        car::v1::write_block(&mut self.writer, cid, block, &mut self.cid_buffer).await?;
        Ok(())
    }

    /// Flushes and returns the inner writer.
    pub async fn finish(mut self) -> Result<W, Error> {
        self.writer.flush().await?;
        Ok(self.writer)
    }
}

#[cfg(test)]
mod tests {
    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::io::BufWriter;

    use crate::{car, car::generate_multihash, car::v2::Header};

    use super::Writer;

    const RAW_CODEC: u64 = 0x55;

    impl Writer<BufWriter<Vec<u8>>> {
        fn test_writer() -> Self {
            let buffer = Vec::new();
            let buf_writer = BufWriter::new(buffer);
            Writer::new(buf_writer)
        }
    }

    #[tokio::test]
    async fn header() {
        // TODO(@jmg-duarte,18/05/2024): finish this
    }

    #[tokio::test]
    async fn full() {
        let mut writer = Writer::test_writer();

        let file_contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256>(&file_contents);
        let root_cid = Cid::new_v1(RAW_CODEC, contents_multihash);

        // Values were reversed out of a the car file
        writer
            .write_header(&Header::new(false, 51, 7661, 7712))
            .await
            .unwrap();

        writer
            .write_v1_header(&car::v1::Header::new(vec![root_cid]))
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
