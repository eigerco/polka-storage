use bytes::BytesMut;
use digest::Digest;
use futures::StreamExt;
use ipld_core::cid::Cid;
use sha2::Sha256;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite};
use tracing::trace;

use super::Config;
use crate::{
    multicodec::SHA_256_CODE,
    unixfs::{stream_balanced_tree, stream_balanced_tree_unixfs},
    CarV1Header, CarV2Header, CarV2Writer, Error, Index, IndexEntry, MultihashIndexSorted,
    SingleWidthIndex,
};

/// Converts a source stream into a CARv2 file and writes it to an output stream.
//
// The expanded trait bounds are required because:
// - `Send + 'static`: The async stream operations require the ability to move the source/output
//   between threads and ensure they live long enough for the entire async operation
// - `AsyncSeek`: Required for the output to write the final header after processing all blocks
// - `Unpin`: Required because we need to move the source/output around during async operations
async fn balanced_import<Src, Out>(
    mut source: Src,
    mut output: Out,
    chunk_size: usize,
    tree_width: usize,
) -> Result<Cid, Error>
where
    Src: AsyncRead + Unpin,
    Out: AsyncWrite + AsyncSeek + Unpin,
{
    // This custom stream gathers incoming buffers into a single byte chunk of `chunk_size`
    // `tokio_util::io::ReaderStream` does a very similar thing, however, it does not attempt
    // to fill it's buffer before returning, voiding the whole promise of properly sized chunks
    // There is an alternative implementation (untested & uses unsafe) in the following GitHub Gist:
    // https://gist.github.com/jmg-duarte/f606410a5e0314d7b5cee959a240b2d8
    let chunker = async_stream::try_stream! {
        let mut buf = BytesMut::with_capacity(chunk_size);

        loop {
            // BytesMut::reserve *may* allocate more memory than requested to avoid further
            // allocations, while that's very helpful, it's also unpredictable.
            if buf.capacity() < chunk_size {
                // BytesMut::reserve *may* allocate more memory than requested to avoid further
                // allocations, while that's very helpful, it's also unpredictable.
                // If and when necessary, we can replace this with the following line:
                // std::mem::replace(buf, BytesMut::with_capacity(chunk_size)):

                // Reserve only the difference as the split may leave nothing, or something
                buf.reserve(chunk_size - buf.capacity());
            }

            // If the read length is 0, we *assume* we reached EOF
            // tokio's docs state that this does not mean we exhausted the reader,
            // as it may be able to return more bytes later, *however*,
            // this means there is no right way of knowing when the reader is fully exhausted!
            // If we need to support a case like that, we just need to track how many times
            // the reader returned 0 and break at a certain point
            let read_bytes = source.read_buf(&mut buf).await?;
            trace!(bytes_read = read_bytes, buffer_size = buf.len(), "Buffer read status");
            // EOF but there's still content to yield -> yield it
            while buf.len() >= chunk_size {
                // The buffer may have a larger capacity than chunk_size due to reserve
                // this also means that our read may have read more bytes than we expected,
                // thats why we check if the length if bigger than the chunk_size and if so
                // we split the buffer to the chunk_size, then freeze and return
                let chunk = buf.split_to(chunk_size);
                yield chunk.freeze();
            } // otherwise, the buffer is not full, so we don't do a thing

            if read_bytes == 0 && !buf.is_empty() {
                let chunk = buf.split();
                yield chunk.freeze();
                break;
            } else if read_bytes == 0 {
                break;
            }
        }
    };

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
        let entry = IndexEntry::new(
            node_cid.hash().digest().to_vec(),
            (position - car_v1_start) as u64,
        );
        entries.push(entry);
        position += writer.write_block(&node_cid, &node_bytes).await?;

        if nodes.as_mut().peek().await.is_none() {
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

    writer.get_inner_mut().rewind().await?;
    let header = CarV2Header::new(
        false,
        car_v1_start.try_into().unwrap(),
        (index_offset - car_v1_start).try_into().unwrap(),
        index_offset.try_into().unwrap(),
    );
    writer.write_header(&header).await?;

    let header_v1 = CarV1Header::new(vec![root.unwrap()]);
    writer.write_v1_header(&header_v1).await?;

    writer.finish().await?;

    Ok(root.unwrap())
}

async fn balanced_import_unixfs<Src, Out>(
    mut source: Src,
    mut output: Out,
    chunk_size: usize,
    tree_width: usize,
) -> Result<Cid, Error>
where
    Src: AsyncRead + Unpin + Send + 'static,
    Out: AsyncWrite + AsyncSeek + Unpin + Send + 'static,
{
    let chunker = async_stream::try_stream! {
        let mut buf = BytesMut::with_capacity(chunk_size);
        loop {
            let read_bytes = source.read_buf(&mut buf).await?;
            while buf.len() >= chunk_size {
                let chunk = buf.split_to(chunk_size);
                yield chunk.freeze();
            }

            if read_bytes == 0 && !buf.is_empty() {
                let chunk = buf.split();
                yield chunk.freeze();
                break;
            } else if read_bytes == 0 {
                break;
            }
        }
    };

    let nodes = stream_balanced_tree_unixfs(chunker, tree_width).peekable();
    tokio::pin!(nodes);

    let mut writer = CarV2Writer::new(&mut output);
    let mut position = 0;

    position += writer.write_header(&CarV2Header::default()).await?;
    let car_v1_start = position;
    position += writer.write_v1_header(&CarV1Header::default()).await?;

    let mut root = None;
    let mut entries = vec![];

    while let Some(node) = nodes.next().await {
        let (node_cid, node_bytes) = node?;
        let entry = IndexEntry::new(
            node_cid.hash().digest().to_vec(),
            (position - car_v1_start) as u64,
        );
        entries.push(entry);
        position += writer.write_block(&node_cid, &node_bytes).await?;

        root = Some(node_cid);
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

    // Flush even if the caller doesn't - we did our best
    writer.finish().await?;

    Ok(root)
}

/// Convert a `source` stream into a CARv2 file and write it to an `output` stream.
pub async fn create_filestore<Src, Out>(
    source: Src,
    output: Out,
    config: Config,
) -> Result<Cid, Error>
where
    Src: AsyncRead + Unpin + Send + 'static,
    Out: AsyncWrite + AsyncSeek + Unpin + Send + 'static,
{
    match config {
        Config::Balanced {
            chunk_size,
            tree_width,
            raw_mode,
        } => {
            if raw_mode {
                balanced_import(source, output, chunk_size, tree_width).await
            } else {
                balanced_import_unixfs(source, output, chunk_size, tree_width).await
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::path::Path;

    use ipld_core::codec::Codec;
    use quick_protobuf::MessageRead;
    use tempfile::tempdir;
    use tokio::fs::File;

    use super::*;
    use crate::{
        test_utils::assert_buffer_eq, DEFAULT_CHUNK_SIZE, DEFAULT_TREE_WIDTH,
    };

    async fn test_filestore_roundtrip<P1, P2>(original: P1, expected: P2)
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().join("temp.car");

        let source_file = File::open(original).await.unwrap();
        let output_file = File::create(&temp_path).await.unwrap();
        let config = Config::balanced_raw(DEFAULT_CHUNK_SIZE, DEFAULT_TREE_WIDTH);
        create_filestore(source_file, output_file, config)
            .await
            .unwrap();

        let expected = tokio::fs::read(expected.as_ref()).await.unwrap();
        let result = tokio::fs::read(temp_path).await.unwrap();

        assert_buffer_eq!(&expected, &result);
    }

    #[tokio::test]
    async fn test_lorem_roundtrip() {
        test_filestore_roundtrip(
            "tests/fixtures/original/lorem.txt",
            "tests/fixtures/car_v2/lorem.car",
        )
        .await
    }

    #[tokio::test]
    async fn test_spaceglenda_roundtrip() {
        test_filestore_roundtrip(
            "tests/fixtures/original/spaceglenda.jpg",
            "tests/fixtures/car_v2/spaceglenda.car",
        )
        .await
    }

    #[tokio::test]
    async fn test_filestore_unixfs_dag_structure() {
        use rand::{thread_rng, Rng};

        const TEST_CHUNK_SIZE: usize = 64 * 1024; // 64 KiB
        const TEST_TREE_WIDTH: usize = 2;

        let temp_dir = tempdir().unwrap();
        let input_path = temp_dir.path().join("input.bin");
        let temp_path = temp_dir.path().join("temp.car");

        // Create test file with random data to ensure unique chunks.
        let mut rng = thread_rng();
        let test_data = (0..512 * 1024).map(|_| rng.gen::<u8>()).collect::<Vec<_>>();

        trace!("Creating test file of size: {} bytes", test_data.len());
        tokio::fs::write(&input_path, &test_data).await.unwrap();

        let source_file = File::open(&input_path).await.unwrap();
        let output_file = File::create(&temp_path).await.unwrap();

        let config = Config::balanced_unixfs(TEST_CHUNK_SIZE, TEST_TREE_WIDTH);

        let root_cid = create_filestore(source_file, output_file, config)
            .await
            .unwrap();
        trace!("Root CID: {}", root_cid);

        // Read back and verify structure.
        let file = File::open(&temp_path).await.unwrap();
        let mut reader = crate::CarV2Reader::new(file);

        reader.read_pragma().await.unwrap();
        reader.read_header().await.unwrap();
        reader.read_v1_header().await.unwrap();

        // Track all unique blocks and statistics.
        let mut unique_blocks = std::collections::HashSet::new();
        let mut leaf_blocks = std::collections::HashSet::new();
        let mut parent_blocks = std::collections::HashSet::new();
        let mut level_sizes = Vec::new();
        let mut current_level_nodes = std::collections::HashSet::new();
        let mut current_level = 0;

        while let Ok((cid, data)) = reader.read_block().await {
            unique_blocks.insert(cid);

            let pb_node: ipld_dagpb::PbNode = ipld_dagpb::DagPbCodec::decode(&data[..]).unwrap();
            let mut proto_reader =
                quick_protobuf::BytesReader::from_bytes(&pb_node.data.clone().unwrap());
            let bytes = &pb_node.data.unwrap();
            let unixfs_data = crate::unixfs::Data::from_reader(&mut proto_reader, bytes).unwrap();

            if pb_node.links.is_empty() {
                leaf_blocks.insert(cid);
                trace!("Found leaf node: {} (size: {})", cid, data.len());
                trace!(
                    "  Data size: {}",
                    unixfs_data.Data.as_ref().map_or(0, |d| d.len())
                );
                trace!("  Blocksizes: {:?}", unixfs_data.blocksizes);

                // New level if this is first leaf.
                if current_level_nodes.is_empty() {
                    level_sizes.push(0);
                    current_level = level_sizes.len() - 1;
                }
            } else {
                parent_blocks.insert(cid);

                trace!(
                    "Found parent node: {} with {} links (size: {})",
                    cid,
                    pb_node.links.len(),
                    data.len()
                );
                trace!("  Total filesize: {:?}", unixfs_data.filesize);
                trace!("  Blocksizes: {:?}", unixfs_data.blocksizes);

                // Track level changes.
                if !current_level_nodes.is_empty()
                    && current_level_nodes
                        .iter()
                        .any(|n| pb_node.links.iter().any(|l| l.cid == *n))
                {
                    level_sizes.push(0);
                    current_level = level_sizes.len() - 1;
                    current_level_nodes.clear();
                }
            }

            level_sizes[current_level] += 1;
            current_level_nodes.insert(cid);
        }

        // Verify structure.
        assert!(!leaf_blocks.is_empty(), "No leaf nodes found");
        assert!(!parent_blocks.is_empty(), "No parent nodes found");
        assert_eq!(
            unique_blocks.len(),
            leaf_blocks.len() + parent_blocks.len(),
            "Block count mismatch"
        );
    }

}
