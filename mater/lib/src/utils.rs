use async_stream::try_stream;
use futures::Stream;
use tokio::io::{AsyncRead, AsyncSeek};

use crate::{BlockMetadata, CarV2Reader, Error};

/// Stream blocks metadata from CARv2 data source until completion.
pub async fn stream_blocks_metadata<R>(
    reader: R,
) -> Result<impl Stream<Item = Result<BlockMetadata, Error>>, Error>
where
    R: AsyncRead + AsyncSeek + Unpin,
{
    let mut reader = CarV2Reader::new(reader);

    reader.read_pragma().await?;
    let header = reader.read_header().await?;
    let _v1_header = reader.read_v1_header().await?;

    let data_end = header.data_offset + header.data_size;

    Ok(try_stream! {
        loop {
            let metadata = reader.read_block_metadata().await?;
            let position = metadata.data_offset_source + metadata.data_size;

            yield metadata;

            // This is the last block
            if position >= data_end {
                break;
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use futures::{pin_mut, StreamExt};
    use tokio::fs::File;

    use crate::stream_blocks_metadata;

    #[tokio::test]
    async fn test_stream_blocks_metadata_empty() {
        let file = File::open("tests/fixtures/car_v2/empty.car").await.unwrap();
        let blocks = stream_blocks_metadata(file).await.unwrap();
        pin_mut!(blocks);

        // Empty block
        let block = blocks.next().await.unwrap().unwrap();
        assert_eq!(block.data_size, 0);
        assert_eq!(block.data_offset_source, 147);
    }

    #[tokio::test]
    async fn test_stream_blocks_metadata_lorem() {
        let file = File::open("tests/fixtures/car_v2/lorem.car").await.unwrap();
        let blocks = stream_blocks_metadata(file).await.unwrap();
        pin_mut!(blocks);

        // First block
        let block = blocks.next().await.unwrap().unwrap();
        assert_eq!(block.data_size, 7564);
        assert_eq!(block.data_offset_source, 148);

        // Second block
        assert!(blocks.next().await.is_none());
    }

    #[tokio::test]
    async fn test_stream_blocks_metadata_spaceglenda() {
        let file = File::open("tests/fixtures/car_v2/spaceglenda.car")
            .await
            .unwrap();
        let blocks = stream_blocks_metadata(file).await.unwrap();
        pin_mut!(blocks);

        // First block
        let block = blocks.next().await.unwrap().unwrap();
        assert_eq!(block.data_size, 262144);
        assert_eq!(block.data_offset_source, 149);

        // Second block
        let block = blocks.next().await.unwrap().unwrap();
        assert_eq!(block.data_size, 262144);
        assert_eq!(block.data_offset_source, 262332);

        // Third block
        let block = blocks.next().await.unwrap().unwrap();
        assert_eq!(block.data_size, 129742);
        assert_eq!(block.data_offset_source, 524515);

        // Fourth block
        let block = blocks.next().await.unwrap().unwrap();
        assert_eq!(block.data_size, 158);
        assert_eq!(block.data_offset_source, 654295);

        // Fifth block
        assert!(blocks.next().await.is_none());
    }
}
