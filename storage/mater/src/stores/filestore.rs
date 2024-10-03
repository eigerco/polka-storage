use bytes::BytesMut;
use futures::stream::StreamExt;
use ipld_core::cid::Cid;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite};

use super::Config;
use crate::{
    multicodec::SHA_256_CODE, unixfs::stream_balanced_tree, CarV1Header, CarV2Header, CarV2Writer,
    Error, Index, IndexEntry, MultihashIndexSorted, SingleWidthIndex,
};

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
            if buf.capacity() < chunk_size {
                // BytesMut::reserve *may* allocate more memory than requested to avoid further
                // allocations, while that's very helpful, it's also unpredictable.
                // If and when necessary, we can replace this with the following line:
                // std::mem::replace(buf, BytesMut::with_capacity(chunk_size)):

                // Reserve only the difference as the split may leave nothing, or something
                buf.reserve(chunk_size - buf.capacity());
            }

            let read = source.read_buf(&mut buf).await?;
            // If we're at capacity *or* we didn't read anything but the buffer isn't empty
            if (buf.len() == buf.capacity()) || (read == 0 && buf.len() > 0) {
                // We split the buffer, freeze it and yield it off
                let chunk = buf.split();
                yield chunk.freeze();
            } else if read == 0 && buf.len() == 0 {
                // If we didn't read a thing and the buffer is empty
                // we're good to go
                break
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
