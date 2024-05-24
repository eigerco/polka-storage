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
    pub async fn write_header(&mut self, header: &Header) -> Result<(), Error> {
        self.writer.write(&PRAGMA).await?;

        let mut buffer = [0; 40];
        let mut handle = &mut buffer[..];
        WriteBytesExt::write_u128::<LittleEndian>(&mut handle, header.characteristics.bits())?;
        WriteBytesExt::write_u64::<LittleEndian>(&mut handle, header.data_offset)?;
        WriteBytesExt::write_u64::<LittleEndian>(&mut handle, header.data_size)?;
        WriteBytesExt::write_u64::<LittleEndian>(&mut handle, header.index_offset)?;

        self.writer.write_all(&buffer).await?;
        Ok(())
    }

    /// Write a [`crate::v1::Header`].
    pub async fn write_v1_header(&mut self, v1_header: &crate::v1::Header) -> Result<(), Error> {
        crate::v1::write_header(&mut self.writer, v1_header).await
    }

    /// Write a [`Cid`] and the respective data block.
    pub async fn write_block<Block>(&mut self, cid: &Cid, block: &Block) -> Result<(), Error>
    where
        Block: AsRef<[u8]>,
    {
        crate::v1::write_block(&mut self.writer, cid, block).await
    }

    /// Write an [`Index`].
    pub async fn write_index(&mut self, index: &Index) -> Result<(), Error> {
        crate::v2::index::write_index(&mut self.writer, index).await
    }

    /// Write padding.
    ///
    /// Padding consists of only `0x0` bytes.
    pub async fn write_padding(&mut self, length: usize) -> Result<(), Error> {
        for _ in 0..length {
            self.writer.write_u8(0).await?;
        }
        Ok(())
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
    use ipld_dagpb::{PbLink, PbNode};
    use sha2::Sha256;
    use tokio::{
        fs::File,
        io::{AsyncSeekExt, BufWriter},
    };
    use tokio_stream::StreamExt;
    use tokio_util::io::ReaderStream;

    use crate::{
        multicodec::{generate_multihash, MultihashCode, DAG_PB_CODE, RAW_CODE},
        v2::{
            index::{IndexEntry, IndexSorted, SingleWidthIndex},
            test_utils::assert_buffer_eq,
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

        assert_buffer_eq(&expected_header, buf_writer.get_ref().get_ref())
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
        let mut file_chunker = ReaderStream::with_capacity(file, 1024 * 256);
        let mut file_blocks = vec![];
        while let Some(chunk) = file_chunker.next().await {
            let chunk = chunk.unwrap();
            let multihash = generate_multihash::<Sha256, _>(&chunk);
            let cid = Cid::new_v1(RAW_CODE, multihash);
            file_blocks.push((cid, chunk));
        }

        let links = file_blocks
            .iter()
            .map(|(cid, block)| PbLink {
                cid: cid.clone(),
                // NOTE(@jmg-duarte,23/05/2024): actually how go-car does it... kinda weird if you ask me
                name: Some("".to_string()),
                size: Some(block.len() as u64),
            })
            .collect();
        let node = PbNode { links, data: None };
        let mut node_bytes = node.into_bytes();
        // This is very much cheating but the contents here are the UnixFS wrapper for the node
        // TODO(@jmg-duarte,22/05/2024): replace this when we implement unixfs
        std::io::Write::write_all(
            &mut node_bytes,
            &vec![
                0x0A, 0x12, 0x08, 0x02, 0x18, 0xCE, 0xF5, 0x27, 0x20, 0x80, 0x80, 0x10, 0x20, 0x80,
                0x80, 0x10, 0x20, 0xCE, 0xF5, 0x07,
            ],
        )
        .unwrap();
        let root_cid = {
            let multihash = generate_multihash::<Sha256, _>(&node_bytes);
            Cid::new_v1(DAG_PB_CODE, multihash)
        };

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
            .write_v1_header(&crate::v1::Header::new(vec![root_cid]))
            .await
            .unwrap();

        let mut offsets = vec![];
        for (cid, block) in &file_blocks {
            // write the blocks, saving their positions for the index
            offsets.push({
                let inner = writer.get_inner_mut();
                inner.stream_position().await.unwrap() - start_car_v1
            });
            writer.write_block(cid, block).await.unwrap();
        }
        // Write the DAG-PB link unifying everything
        offsets.push({
            let inner = writer.get_inner_mut();
            inner.stream_position().await.unwrap() - start_car_v1
        });
        writer.write_block(&root_cid, &node_bytes).await.unwrap();

        let inner = writer.get_inner_mut();
        let written = inner.stream_position().await.unwrap();
        assert_eq!(written, 654453);

        let mut mapping = BTreeMap::new();
        mapping.insert(
            Sha256::CODE,
            IndexSorted::from(
                SingleWidthIndex::try_from(
                    file_blocks
                        .iter()
                        .chain(std::iter::once(&(root_cid, node_bytes.into())))
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

        assert_buffer_eq(&expected_header, buf_writer.get_ref().get_ref());
    }
}
