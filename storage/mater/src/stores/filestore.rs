use std::path::PathBuf;

use sha2::{Digest, Sha256};
use tokio::{fs::File, io::AsyncSeekExt};
use tokio_stream::StreamExt;
use tokio_util::io::ReaderStream;

use crate::{
    multicodec::SHA_256_CODE, unixfs::stream_balanced_tree, CarV1Header, CarV2Header, CarV2Writer,
    Error, Index, IndexEntry, MultihashIndexSorted, SingleWidthIndex,
};

use super::Config;

/// A file-backed CAR store.
pub struct Filestore {
    source_path: PathBuf,
    output_path: PathBuf,

    config: Config,
}

impl Filestore {
    pub fn new<P>(source: P, output: P, config: Config) -> Result<Self, Error>
    where
        P: Into<PathBuf>,
    {
        Ok(Self {
            source_path: source.into(),
            output_path: output.into(),
            config,
        })
    }

    async fn balanced_import(&mut self, chunk_size: usize, tree_width: usize) -> Result<(), Error> {
        let mut source = File::open(&self.source_path).await?;
        let chunker = ReaderStream::with_capacity(&mut source, chunk_size);
        let nodes = stream_balanced_tree(chunker, tree_width);
        tokio::pin!(nodes);
        let mut nodes = nodes.peekable();

        let mut output = File::create(&self.output_path).await?;
        let mut writer = CarV2Writer::new(&mut output);
        let mut position = 0;

        let placeholder_header = CarV2Header::default();
        position += writer.write_header(&placeholder_header).await?;
        let car_v1_start = position;

        let placeholder_header_v1 = CarV1Header::default();
        position += writer.write_v1_header(&placeholder_header_v1).await?;

        let mut root = None;
        let mut entries = vec![];
        while let Some(node) = nodes.next().await {
            let (node_cid, node_bytes) = node?;
            let digest = node_cid.hash().digest().to_owned();
            let entry = IndexEntry::new(digest, (position - car_v1_start) as u64);
            entries.push(entry);
            position += writer.write_block(&node_cid, &node_bytes).await?;

            if nodes.peek().await.is_none() {
                root = Some(node_cid);
            }
        }

        let index_offset = position;
        let single_width_index =
            SingleWidthIndex::new(Sha256::output_size() as u32, entries.len() as u64, entries);
        let index = Index::MultihashIndexSorted(MultihashIndexSorted::from_single_width(
            SHA_256_CODE,
            single_width_index.into(),
        ));
        writer.write_index(&index).await?;

        // Go back to the beginning of the file
        writer.get_inner_mut().rewind().await?;
        let header = CarV2Header::new(
            false,
            (car_v1_start) as u64,
            (index_offset - car_v1_start) as u64,
            (index_offset) as u64,
        );
        writer.write_header(&header).await?;

        // If the length of the roots doesn't match the previous one, you WILL OVERWRITE parts of the file
        let header_v1 = CarV1Header::new(vec![root.expect("root should have been set")]);
        writer.write_v1_header(&header_v1).await?;

        Ok(())
    }

    pub async fn import(&mut self) -> Result<(), Error> {
        match self.config {
            Config::Balanced {
                chunk_size,
                tree_width,
            } => self.balanced_import(chunk_size, tree_width).await,
        }
    }
}
