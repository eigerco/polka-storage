use std::{
    collections::{BTreeMap, HashMap},
    io::Cursor,
};

use futures::stream::StreamExt;
use ipld_core::cid::Cid;
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt, AsyncWrite, AsyncWriteExt};

use crate::{
    unixfs::stream_balanced_tree,
    v1::{self, CarWriter},
    v2::{self, CarWriter as _},
    BlockMetadata, Config, Error, Index, IndexEntry, IndexSorted, SingleWidthIndex,
};

/// CAR file writer.
///
/// Disambiguation: this writer is not a [`File`](tokio::fs::File) writer,
/// but rather a file writer in the CAR file format sense.
pub struct FileWriter<W> {
    writer: W,
    index: HashMap<Cid, BlockMetadata>,
    roots: Vec<Cid>,
    config: Config,
}

impl<W> FileWriter<W> {
    /// Creates a new [`FileWriter`] with the given `writer`.
    fn create(writer: W) -> Self {
        Self {
            writer,
            index: HashMap::new(),
            roots: Vec::with_capacity(1),
            config: Default::default(),
        }
    }

    /// Creates a new [`v1::Header`].
    fn header_v1(&self) -> v1::Header {
        // Lack of partial moves make this clone "required"
        v1::Header::new(self.roots.clone())
    }

    /// Creates a new [`v2::Header`].
    fn header_v2(&self, v1_header: &v1::Header) -> v2::Header {
        let total_block_encoded_len = self
            .index
            .values()
            .map(BlockMetadata::encoded_len)
            .sum::<u64>();

        let v1_payload_len = v1_header.encoded_len() as u64 + total_block_encoded_len;

        v2::Header::new(
            false,
            v2::Header::SIZE,
            v1_payload_len,
            v2::Header::SIZE + v1_payload_len,
        )
    }
}

impl<W> FileWriter<W>
where
    W: AsyncWrite + Unpin,
{
    /// Creates a new [`FileWriter`] from the passed `writer`.
    pub async fn new(writer: W) -> Result<Self, Error> {
        let mut self_ = Self::create(writer);
        self_.writer.write_v2_header(&Default::default()).await?;
        self_.writer.write_header(&Default::default()).await?;
        Ok(self_)
    }
}

impl FileWriter<Cursor<Vec<u8>>> {
    /// Creates a new [`FileWriter`] backed by an in-memory buffer.
    pub async fn in_memory() -> Result<Self, Error> {
        Self::new(Cursor::new(vec![])).await
    }
}

impl<W> FileWriter<W>
where
    W: AsyncWrite + AsyncSeek + Unpin,
{
    /// Convert `source` into a CAR file, writing it to `writer`.
    /// Returns the root [`Cid`].
    pub async fn import<S>(source: S, writer: W) -> Result<Cid, Error>
    where
        S: AsyncRead + Unpin,
    {
        let mut writer = Self::new(writer).await?;
        writer.write_from(source).await?;
        let root = *writer.roots.first().ok_or(Error::EmptyRootsError)?;
        writer.finish().await?;
        Ok(root)
    }

    /// Writes the contents from `source`, adding a new root to [`FileWriter`].
    pub async fn write_from<S>(&mut self, source: S) -> Result<(), Error>
    where
        S: AsyncRead + Unpin,
    {
        let mut current_position = self.writer.stream_position().await?;

        let nodes = match self.config {
            Config::Balanced {
                chunk_size,
                tree_width,
            } => {
                let chunker = crate::chunker::byte_stream_chunker(source, chunk_size);
                stream_balanced_tree(chunker, tree_width).peekable()
            }
        };
        tokio::pin!(nodes);

        let mut root = None;
        while let Some(node) = nodes.next().await {
            let (node_cid, node_bytes) = node?;
            let block_offset = current_position;

            if !self.index.contains_key(&node_cid) {
                let written = self.writer.write_block(&node_cid, &node_bytes).await? as u64;
                current_position += written;
                self.index.insert(
                    node_cid,
                    BlockMetadata {
                        block_offset,
                        cid: node_cid,
                        data_offset_source: block_offset + written,
                        data_size: node_bytes.len() as u64,
                    },
                );
            }

            if nodes.as_mut().peek().await.is_none() {
                root = Some(node_cid);
            }
        }

        match root {
            Some(root) => self.roots.push(root),
            None => return Err(Error::EmptyRootsError),
        }

        Ok(())
    }

    /// Writes the final header as well as the indexes, flushes the inner writer and returns it.
    pub async fn finish(mut self) -> Result<W, Error> {
        self.writer.rewind().await?;

        let v1_header = self.header_v1();
        let v2_header = self.header_v2(&v1_header);

        self.writer.write_v2_header(&v2_header).await?;
        self.writer.write_header(&v1_header).await?;

        self.writer
            .seek(std::io::SeekFrom::Start(v2_header.index_offset))
            .await?;

        // Abstracting away the index writing is not that simple because we have extra bookkeeping
        let mut multihash_index: BTreeMap<u64, BTreeMap<usize, Vec<IndexEntry>>> = BTreeMap::new();
        for (cid, metadata) in self.index {
            let entry = IndexEntry::new(
                metadata.cid.hash().digest().to_vec(),
                metadata.block_offset - v2_header.data_offset,
            );

            let cid_hash_code = cid.hash().code();
            let cid_hash_digest_len = cid.hash().digest().len();

            if let Some(single_width_index) = multihash_index.get_mut(&cid_hash_code) {
                if let Some(entries) = single_width_index.get_mut(&cid_hash_digest_len) {
                    entries.push(entry);
                } else {
                    single_width_index.insert(cid_hash_digest_len, vec![entry]);
                }
            } else {
                let mut single_width_index = BTreeMap::new();
                single_width_index.insert(cid_hash_digest_len, vec![entry]);
                multihash_index.insert(cid_hash_code, single_width_index);
            };
        }
        let index = Index::multihash(
            multihash_index
                .into_iter()
                .map(|(codec, index)| {
                    let index_sorted = index
                        .into_iter()
                        .map(|(width, entries)| {
                            SingleWidthIndex::new(width as u32, entries.len() as u64, entries)
                        })
                        .collect::<Vec<_>>();
                    (codec, IndexSorted(index_sorted))
                })
                .collect(),
        );
        self.writer.write_index(&index).await?;

        self.writer.flush().await?;
        Ok(self.writer)
    }

    /// Sets the roots before performing the finalization procedure — see [`FileWriter::finish`]
    /// for details.
    pub async fn finish_with_roots<I>(mut self, roots: I) -> Result<W, Error>
    where
        I: IntoIterator<Item = Cid>,
    {
        self.roots = roots.into_iter().collect();
        self.finish().await
    }
}

#[cfg(test)]
mod tests {
    use std::{io::Cursor, path::Path};

    use tokio::fs::File;

    use crate::{convert::writer::FileWriter, test_utils::assert_buffer_eq};

    async fn byte_eq<P1, P2>(original: P1, reference: P2)
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let file = File::open(original).await.unwrap();
        let reference = tokio::fs::read(reference).await.unwrap();

        let mut store = FileWriter::in_memory().await.unwrap();
        store.write_from(file).await.unwrap();
        let output = store.finish().await.unwrap().into_inner();
        assert_buffer_eq!(&output, &reference);
    }

    #[tokio::test]
    async fn new_byte_eq_lorem() {
        byte_eq(
            "tests/fixtures/original/lorem.txt",
            "tests/fixtures/car_v2/lorem.car",
        )
        .await;
    }

    #[tokio::test]
    async fn byte_eq_spaceglenda() {
        byte_eq(
            "tests/fixtures/original/spaceglenda.jpg",
            "tests/fixtures/car_v2/spaceglenda.car",
        )
        .await;
    }

    #[tokio::test]
    async fn dedup() {
        let input = Cursor::new(vec![0u8; 524288]);
        let mut store = FileWriter::in_memory().await.unwrap();
        store.write_from(input).await.unwrap();
        let output = store.finish().await.unwrap().into_inner();

        let reference = tokio::fs::read("tests/fixtures/car_v2/zero.car")
            .await
            .unwrap();
        assert_buffer_eq!(&output, &reference);
    }
}
