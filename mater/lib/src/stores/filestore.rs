use super::Config;
use crate::{
    multicodec::SHA_256_CODE, unixfs::stream_balanced_tree, CarV1Header, CarV2Header, CarV2Writer,
    Error, Index, IndexEntry, MultihashIndexSorted, SingleWidthIndex,
};
use bytes::BytesMut;
use futures::StreamExt;
use ipld_core::cid::Cid;
use std::collections::HashMap;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite};
use tracing::trace;

/// Converts a source stream into a CARv2 file and writes it to an output stream.
///
/// The expanded trait bounds are required because:
/// - `Send + 'static`: The async stream operations require the ability to move the source/output
///   between threads and ensure they live long enough for the entire async operation
/// - `AsyncSeek`: Required for the output to write the final header after processing all blocks
/// - `Unpin`: Required because we need to move the source/output around during async operations
async fn balanced_import<Src, Out>(
    mut source: Src,
    mut output: Out,
    chunk_size: usize,
    tree_width: usize,
    config: &Config,
) -> Result<Cid, Error>
where
    Src: AsyncRead + Unpin + Send + 'static,
    Out: AsyncWrite + AsyncSeek + Unpin + Send + 'static,
{
    // This custom stream gathers incoming buffers into a single byte chunk of `chunk_size`
    // `tokio_util::io::ReaderStream` does a very similar thing, however, it does not attempt
    // to fill it's buffer before returning, voiding the whole promise of properly sized chunks
    // There is an alternative implementation (untested & uses unsafe) in the following GitHub Gist:
    // https://gist.github.com/jmg-duarte/f606410a5e0314d7b5cee959a240b2d8
    let chunker = Box::pin(async_stream::try_stream! {
        let mut buf = BytesMut::with_capacity(chunk_size);
        loop {
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
    });

    // Create the balanced tree stream
    let nodes = stream_balanced_tree(chunker, tree_width, config).peekable();
    tokio::pin!(nodes);

    // Initialize the writer
    let mut writer = CarV2Writer::new(&mut output);
    let mut position = 0;

    // Write placeholder header to be updated later
    position += writer.write_header(&CarV2Header::default()).await?;

    // Write CARv1 header and track its start position for index offsets
    let car_v1_start = position;
    position += writer.write_v1_header(&CarV1Header::default()).await?;

    let mut root = None;
    let mut entries = vec![];
    let mut seen_blocks = HashMap::new();

    // Process all blocks from the balanced tree
    while let Some(node) = nodes.next().await {
        let (node_cid, node_bytes) = node?;

        // Handle deduplication
        if let Some(existing_offset) = seen_blocks.get(&node_cid) {
            entries.push(IndexEntry::new(
                node_cid.hash().digest().to_vec(),
                (*existing_offset - car_v1_start) as u64,
            ));
        } else {
            entries.push(IndexEntry::new(
                node_cid.hash().digest().to_vec(),
                (position - car_v1_start) as u64,
            ));
            position += writer.write_block(&node_cid, &node_bytes).await?;
            seen_blocks.insert(node_cid, position);
        }

        // Check if this is the root node
        if nodes.as_mut().peek().await.is_none() {
            root = Some(node_cid);
        }
    }

    // Create and write index
    let index = {
        let single_width_index = SingleWidthIndex::try_from(entries)?;
        Index::MultihashIndexSorted(MultihashIndexSorted::from_single_width(
            SHA_256_CODE,
            single_width_index.into(),
        ))
    };
    position += writer.write_index(&index).await?;

    // Update header with final values
    let header = {
        let data_size = position - car_v1_start;
        let data_offset = CarV2Header::SIZE;
        CarV2Header::new(
            false,
            data_offset,
            data_size.try_into().unwrap(),
            position.try_into().unwrap(),
        )
    };

    // Seek back and write final header
    writer
        .get_inner_mut()
        .seek(std::io::SeekFrom::Start(0))
        .await?;
    writer.write_header(&header).await?;

    // Finalize writer
    writer.finish().await?;

    // Return root CID
    Ok(root.expect("the stream yielded at least one element"))
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
            ..
        } => balanced_import(source, output, chunk_size, tree_width, &config).await,
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::assert_buffer_eq;
    use crate::unixfs::Data;
    use ipld_core::codec::Codec;
    use ipld_dagpb::{DagPbCodec, PbNode};
    use quick_protobuf::MessageRead;
    use std::{collections::HashSet, path::Path};
    use tempfile::tempdir;
    use tokio::fs::File;

    async fn test_filestore_roundtrip<P1, P2>(original: P1, expected: P2)
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let temp_dir = tempdir().unwrap();
        let temp_path = temp_dir.path().join("temp.car");

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

        let temp_dir = tempdir().unwrap();
        let input_path = temp_dir.path().join("input.bin");
        let temp_path = temp_dir.path().join("temp.car");

        // Create test file with random data to ensure unique chunks
        let mut rng = thread_rng();
        let test_data = (0..512 * 1024).map(|_| rng.gen::<u8>()).collect::<Vec<_>>();

        trace!("Creating test file of size: {} bytes", test_data.len());
        tokio::fs::write(&input_path, &test_data).await.unwrap();

        let source_file = File::open(&input_path).await.unwrap();
        let output_file = File::create(&temp_path).await.unwrap();

        let config = Config::balanced_unixfs(64 * 1024, 2);

        let root_cid = create_filestore(source_file, output_file, config)
            .await
            .unwrap();
        trace!("Root CID: {}", root_cid);

        // Read back and verify structure
        let file = File::open(&temp_path).await.unwrap();
        let mut reader = crate::CarV2Reader::new(file);

        reader.read_pragma().await.unwrap();
        reader.read_header().await.unwrap();
        reader.read_v1_header().await.unwrap();

        // Track all unique blocks and statistics
        let mut unique_blocks = HashSet::new();
        let mut leaf_blocks = HashSet::new();
        let mut parent_blocks = HashSet::new();
        let mut level_sizes = Vec::new();
        let mut current_level_nodes = HashSet::new();
        let mut current_level = 0;

        while let Ok((cid, data)) = reader.read_block().await {
            unique_blocks.insert(cid);

            let pb_node: PbNode = DagPbCodec::decode(&data[..]).unwrap();
            let reader =
                &mut quick_protobuf::BytesReader::from_bytes(&pb_node.data.clone().unwrap());
            let bytes = &pb_node.data.unwrap();
            let unixfs_data = Data::from_reader(reader, bytes).unwrap();

            if pb_node.links.is_empty() {
                leaf_blocks.insert(cid);
                trace!("Found leaf node: {} (size: {})", cid, data.len());
                trace!(
                    "  Data size: {}",
                    unixfs_data.Data.as_ref().map_or(0, |d| d.len())
                );
                trace!("  Blocksizes: {:?}", unixfs_data.blocksizes);

                // New level if this is first leaf
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

                for link in &pb_node.links {
                    trace!("  -> Link to: {} (size: {:?})", link.cid, link.size);
                }

                // Track level changes
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

        // Verify structure
        assert!(!leaf_blocks.is_empty(), "No leaf nodes found");
        assert!(!parent_blocks.is_empty(), "No parent nodes found");
        assert_eq!(
            unique_blocks.len(),
            leaf_blocks.len() + parent_blocks.len(),
            "Block count mismatch"
        );
    }
}
