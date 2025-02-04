use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use async_stream::try_stream;
use futures::{pin_mut, Stream, StreamExt};
use local_index_directory::{IndexRecord, OffsetSize, Service};
use mater::{BlockMetadata, CarV2Reader};
use primitives::commitment::{CommP, Commitment};
use tokio::{fs::File, io::AsyncSeekExt, sync::mpsc::UnboundedReceiver, task::spawn_blocking};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{debug, error, info, instrument};

use crate::ServerError;

pub mod local_index_directory;

#[derive(Debug, Clone)]
pub enum IndexerMessage {
    /// Start indexing the raw piece
    IndexPiece {
        /// Piece commitment
        commitment: Commitment<CommP>,
        /// Raw piece path
        piece_path: PathBuf,
    },
}

#[derive(Clone)]
pub struct IndexerState<D> {
    pub lid: Arc<D>,
}

pub async fn start_indexer<D>(
    state: IndexerState<D>,
    mut indexer_rx: UnboundedReceiver<IndexerMessage>,
    token: CancellationToken,
) -> Result<(), ServerError>
where
    D: Service + Send + Sync + 'static,
{
    let tracker = TaskTracker::new();

    loop {
        tokio::select! {
            _ = token.cancelled() => break,
            Some(command) = indexer_rx.recv() => {
                on_command(command, Arc::clone(&state.lid), &tracker);
            },
        }
    }

    // Wait for tasks that are still executing.
    tracker.close();
    info!("Waiting for IndexWorker to finish");
    tracker.wait().await;
    info!("IndexWorker has stopped");

    Ok(())
}

fn on_command<D>(command: IndexerMessage, db: Arc<D>, tracker: &TaskTracker)
where
    D: Service + Send + Sync + 'static,
{
    debug!("Command received: {command:?}");

    match command {
        IndexerMessage::IndexPiece {
            commitment,
            piece_path,
        } => {
            tracker.spawn(on_index_piece(db, commitment, piece_path));
        }
    }
}

#[instrument(skip_all, fields(piece_cid = %commitment.cid()))]
async fn on_index_piece<D>(
    db: Arc<D>,
    commitment: Commitment<CommP>,
    piece_path: PathBuf,
) -> Result<(), ServerError>
where
    D: Service + Send + Sync + 'static,
{
    info!("indexing piece");

    let records = piece_indexes(&piece_path).await?;
    // Move adding the index to the blocking pool. The RocksDB API is sync.
    match spawn_blocking({
        let db = Arc::clone(&db);
        move || db.add_index(commitment.cid(), records, true)
    })
    .await
    {
        Ok(Ok(_)) => {
            info!("indexing completed");
        }
        Ok(Err(err)) => {
            error!(?err, "piece indexing failed with an error");
        }
        Err(err) => {
            error!(?err, "piece indexing panicked");
        }
    };

    Ok(())
}

/// Prepares indexes of a raw piece.
async fn piece_indexes<P>(location: P) -> Result<Vec<IndexRecord>, ServerError>
where
    P: AsRef<Path>,
{
    let blocks = stream_blocks_metadata(&location).await?;
    pin_mut!(blocks);

    let mut records = vec![];
    while let Some(metadata) = blocks.next().await {
        let metadata = metadata?;

        records.push(IndexRecord {
            cid: metadata.cid,
            offset_size: OffsetSize {
                offset: metadata.data_offset_source,
                size: metadata.data_size,
            },
        });
    }

    Ok(records)
}

/// Stream blocks metadata from car file until completion.
async fn stream_blocks_metadata<P>(
    location: P,
) -> Result<impl Stream<Item = Result<BlockMetadata, mater::Error>>, ServerError>
where
    P: AsRef<Path>,
{
    let raw_piece = File::open(&location).await?;
    let mut reader = CarV2Reader::new(raw_piece);
    reader.read_pragma().await?;
    let header = reader.read_header().await?;
    let _v1_header = reader.read_v1_header().await?;

    Ok(try_stream! {
        loop {
            let metadata = reader.read_block_metadata().await?;
            let position = reader.get_inner_mut().stream_position().await?;
            let data_end = header.data_offset + header.data_size;

            yield metadata;

            // This is the last block
            if position >= data_end {
                break;
            }
        }
    })
}
