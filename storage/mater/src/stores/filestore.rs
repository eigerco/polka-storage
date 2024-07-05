use futures::stream::StreamExt;
use ipld_core::cid::Cid;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncSeek, AsyncSeekExt, AsyncWrite};
use tokio_util::io::ReaderStream;

use super::Config;
use crate::{
    multicodec::SHA_256_CODE, unixfs::stream_balanced_tree, CarV1Header, CarV2Header, CarV2Writer,
    Error, Index, IndexEntry, MultihashIndexSorted, SingleWidthIndex,
};

async fn balanced_import<Src, Out>(
    source: Src,
    mut output: Out,
    chunk_size: usize,
    tree_width: usize,
) -> Result<Cid, Error>
where
    Src: AsyncRead + Unpin,
    Out: AsyncWrite + AsyncSeek + Unpin,
{
    let chunker = ReaderStream::with_capacity(source, chunk_size);
    let nodes = stream_balanced_tree(chunker, tree_width).peekable();
    tokio::pin!(nodes);

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

        if nodes.as_mut().peek().await.is_none() {
            root = Some(node_cid);
        }
    }

    let Some(root) = root else {
        return Err(Error::EmptyRootsError);
    };

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
    let header_v1 = CarV1Header::new(vec![root]);
    writer.write_v1_header(&header_v1).await?;

    Ok(root)
}

/// Convert a `source` stream into a CARv2 file and write it to an `output` stream.
pub async fn create_filestore<Src, Out>(
    source: Src,
    output: Out,
    config: Config,
) -> Result<Cid, Error>
where
    Src: AsyncRead + Unpin,
    Out: AsyncWrite + AsyncSeek + Unpin,
{
    match config {
        Config::Balanced {
            chunk_size,
            tree_width,
        } => balanced_import(source, output, chunk_size, tree_width).await,
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use tempfile::tempdir;
    use tokio::fs::File;

    use crate::{
        stores::{filestore::create_filestore, Config},
        test_utils::assert_buffer_eq,
    };

    async fn test_filestore_roundtrip<P1, P2>(original: P1, expected: P2)
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().join("lorem.car");

        let source_file = File::open(original).await.unwrap();
        let output_file = File::create(&temp_path).await.unwrap();
        create_filestore(source_file, output_file, Config::default())
            .await
            .unwrap();

        let expected = tokio::fs::read(expected.as_ref()).await.unwrap();
        let result = tokio::fs::read(temp_path).await.unwrap();

        assert_buffer_eq!(&expected, &result);
    }

    #[tokio::test]
    async fn test_filestore_lorem() {
        test_filestore_roundtrip(
            "tests/fixtures/original/lorem.txt",
            "tests/fixtures/car_v2/lorem.car",
        )
        .await
    }

    #[tokio::test]
    async fn test_filestore_spaceglenda() {
        test_filestore_roundtrip(
            "tests/fixtures/original/spaceglenda.jpg",
            "tests/fixtures/car_v2/spaceglenda.car",
        )
        .await
    }
}
