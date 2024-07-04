use byteorder::{LittleEndian, WriteBytesExt};
use ipld_core::cid::Cid;
use tokio::io::{AsyncWrite, AsyncWriteExt};

use super::{Header, PRAGMA};
use crate::{v2::index::Index, Error};

/// Low-level CARv2 writer.
pub struct Writer<W> {
    writer: W,
}

impl<W> Writer<W> {
    /// Construct a new [`Writer`].
    ///
    /// Takes a write into which the data will be written.
    pub fn new(writer: W) -> Self {
        Self { writer }
    }
}

impl<W> Writer<W>
where
    W: AsyncWrite + Unpin,
{
    /// Write a [`Header`].
    ///
    /// Returns the number of bytes written.
    pub async fn write_header(&mut self, header: &Header) -> Result<usize, Error> {
        self.writer.write_all(&PRAGMA).await?;

        let mut buffer = [0; 40];
        let mut handle = &mut buffer[..];
        WriteBytesExt::write_u128::<LittleEndian>(&mut handle, header.characteristics.bits())?;
        WriteBytesExt::write_u64::<LittleEndian>(&mut handle, header.data_offset)?;
        WriteBytesExt::write_u64::<LittleEndian>(&mut handle, header.data_size)?;
        WriteBytesExt::write_u64::<LittleEndian>(&mut handle, header.index_offset)?;

        self.writer.write_all(&buffer).await?;
        Ok(PRAGMA.len() + buffer.len())
    }

    /// Write a [`crate::v1::Header`].
    ///
    /// Returns the number of bytes written.
    pub async fn write_v1_header(&mut self, v1_header: &crate::v1::Header) -> Result<usize, Error> {
        crate::v1::write_header(&mut self.writer, v1_header).await
    }

    /// Write a [`Cid`] and the respective data block.
    ///
    /// Returns the number of bytes written.
    pub async fn write_block<Block>(&mut self, cid: &Cid, block: &Block) -> Result<usize, Error>
    where
        Block: AsRef<[u8]>,
    {
        crate::v1::write_block(&mut self.writer, cid, block).await
    }

    /// Write an [`Index`].
    ///
    /// Returns the number of bytes written.
    pub async fn write_index(&mut self, index: &Index) -> Result<usize, Error> {
        crate::v2::index::write_index(&mut self.writer, index).await
    }

    /// Write padding (`0x0` bytes).
    ///
    /// Returns the number of bytes written.
    pub async fn write_padding(&mut self, length: usize) -> Result<usize, Error> {
        for _ in 0..length {
            self.writer.write_u8(0).await?;
        }
        Ok(length)
    }

    /// Flushes and returns the inner writer.
    pub async fn finish(mut self) -> Result<W, Error> {
        self.writer.flush().await?;
        Ok(self.writer)
    }

    /// Get a mutable reference to the inner writer.
    pub fn get_inner_mut(&mut self) -> &mut W {
        &mut self.writer
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, io::Cursor};

    use ipld_core::cid::Cid;
    use sha2::Sha256;
    use tokio::{
        fs::File,
        io::{AsyncSeekExt, BufWriter},
    };
    use tokio_stream::StreamExt;
    use tokio_util::io::ReaderStream;

    use crate::{
        multicodec::{generate_multihash, MultihashCode, RAW_CODE},
        test_utils::assert_buffer_eq,
        unixfs::stream_balanced_tree,
        v2::{
            index::{IndexEntry, IndexSorted, SingleWidthIndex},
            Header, Writer,
        },
    };

    impl Writer<BufWriter<Vec<u8>>> {
        fn test_writer() -> Self {
            let buffer = Vec::new();
            let buf_writer = BufWriter::new(buffer);
            Writer::new(buf_writer)
        }
    }

    #[tokio::test]
    async fn header_lorem() {
        let file_contents = tokio::fs::read("tests/fixtures/car_v2/lorem.car")
            .await
            .unwrap();

        let mut writer = Writer::test_writer();
        // To simplify testing, the values were extracted using `car inspect`
        writer
            .write_header(&Header::new(false, 51, 7661, 7712))
            .await
            .unwrap();

        let inner = writer.finish().await.unwrap().into_inner();
        assert_eq!(inner.len(), 51);
        assert_eq!(inner, file_contents[..51]);
    }

    #[tokio::test]
    async fn header_spaceglenda() {
        let file_contents = tokio::fs::read("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();

        let mut writer = Writer::test_writer();
        // To simplify testing, the values were extracted using `car inspect`
        writer
            .write_header(&Header::new(false, 51, 654402, 654453))
            .await
            .unwrap();

        let inner = writer.finish().await.unwrap().into_inner();
        assert_eq!(inner.len(), 51);
        assert_eq!(inner, file_contents[..51]);
    }

    // Byte to byte comparison to the lorem.car file
    #[tokio::test]
    async fn full_lorem() {
        let cursor = Cursor::new(vec![]);
        let buf_writer = BufWriter::new(cursor);
        let mut writer = Writer::new(buf_writer);

        let file_contents = tokio::fs::read("tests/fixtures/original/lorem.txt")
            .await
            .unwrap();
        let contents_multihash = generate_multihash::<Sha256, _>(&file_contents);
        let root_cid = Cid::new_v1(RAW_CODE, contents_multihash);

        // To simplify testing, the values were extracted using `car inspect`
        writer
            .write_header(&Header::new(false, 51, 7661, 7712))
            .await
            .unwrap();

        // We start writing the CARv1 here and keep the stream positions
        // so that we can properly index the blocks later
        let start_car_v1 = {
            let inner = writer.get_inner_mut();
            inner.stream_position().await.unwrap()
        };

        writer
            .write_v1_header(&crate::v1::Header::new(vec![root_cid]))
            .await
            .unwrap();

        let start_car_v1_data = {
            let inner = writer.get_inner_mut();
            inner.stream_position().await.unwrap()
        };

        // There's only one block
        writer.write_block(&root_cid, &file_contents).await.unwrap();

        let inner = writer.get_inner_mut();
        let written = inner.stream_position().await.unwrap();
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
        let index = crate::v2::index::Index::multihash(mapping);
        writer.write_index(&index).await.unwrap();

        let mut buf_writer = writer.finish().await.unwrap();
        buf_writer.rewind().await.unwrap();

        let expected_header = tokio::fs::read("tests/fixtures/car_v2/lorem.car")
            .await
            .unwrap();

        assert_buffer_eq!(&expected_header, buf_writer.get_ref().get_ref())
    }

    // Byte to byte comparison to the spaceglenda.car file
    // This test also covers the nitty-gritty details of how to write a CARv2 file with indexes.
    #[tokio::test]
    async fn full_spaceglenda() {
        let cursor = Cursor::new(vec![]);
        let buf_writer = BufWriter::new(cursor);
        let mut writer = Writer::new(buf_writer);

        let file = File::open("tests/fixtures/original/spaceglenda.jpg")
            .await
            .unwrap();
        // https://github.com/ipfs/boxo/blob/f4fe8997dcbeb39b3a4842d8f08b34739bfd84a4/chunker/parse.go#L13
        let file_chunker = ReaderStream::with_capacity(file, 1024 * 256);
        let nodes = stream_balanced_tree(file_chunker, 11)
            .collect::<Result<Vec<_>, _>>()
            .await
            .unwrap();

        // To simplify testing, the values were extracted using `car inspect`
        writer
            .write_header(&Header::new(false, 51, 654402, 654453))
            .await
            .unwrap();

        // We start writing the CARv1 here and keep the stream positions
        // so that we can properly index the blocks later
        let start_car_v1 = {
            let inner = writer.get_inner_mut();
            inner.stream_position().await.unwrap()
        };

        writer
            .write_v1_header(&crate::v1::Header::new(vec![nodes.last().unwrap().0]))
            .await
            .unwrap();

        let mut offsets = vec![];
        for (cid, block) in &nodes {
            // write the blocks, saving their positions for the index
            offsets.push({
                let inner = writer.get_inner_mut();
                inner.stream_position().await.unwrap() - start_car_v1
            });
            writer.write_block(cid, block).await.unwrap();
        }

        let inner = writer.get_inner_mut();
        let written = inner.stream_position().await.unwrap();
        assert_eq!(written, 654453);

        let mut mapping = BTreeMap::new();
        mapping.insert(
            Sha256::CODE,
            IndexSorted::from(
                SingleWidthIndex::try_from(
                    nodes
                        .iter()
                        .zip(&offsets)
                        .map(|((cid, _), offset)| {
                            IndexEntry::new(cid.hash().digest().to_vec(), *offset)
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap(),
            ),
        );

        let index = crate::v2::index::Index::multihash(mapping);
        writer.write_index(&index).await.unwrap();

        let mut buf_writer = writer.finish().await.unwrap();
        buf_writer.rewind().await.unwrap();

        let expected_header = tokio::fs::read("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();

        assert_buffer_eq!(&expected_header, buf_writer.get_ref().get_ref());
    }
}
