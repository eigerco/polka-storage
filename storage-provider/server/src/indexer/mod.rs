use std::{path::Path, sync::Arc};

use async_stream::try_stream;
use futures::{pin_mut, Stream, StreamExt};
use local_index_directory::{IndexRecord, OffsetSize, Service};
use mater::{BlockMetadata, CarV2Reader};
use polka_storage_provider_common::sector::ProvenSector;
use tokio::{fs::File, io::AsyncSeekExt, sync::mpsc::UnboundedReceiver, task::spawn_blocking};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{debug, error, info, instrument};

use crate::ServerError;

pub mod local_index_directory;

#[derive(Debug, Clone)]
pub enum IndexMessage {
    /// Start indexing the sector
    IndexSector(ProvenSector),
}

#[derive(Clone)]
pub struct IndexerState<D> {
    pub lid: Arc<D>,
}

pub async fn start_indexer<D>(
    state: IndexerState<D>,
    mut indexer_rx: UnboundedReceiver<IndexMessage>,
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

fn on_command<D>(command: IndexMessage, db: Arc<D>, tracker: &TaskTracker)
where
    D: Service + Send + Sync + 'static,
{
    debug!("Command received: {command:?}");

    match command {
        IndexMessage::IndexSector(sector) => {
            tracker.spawn(on_index_sector(db, sector));
        }
    }
}

#[instrument(skip_all, fields(sector = %sector.sector_number))]
async fn on_index_sector<D>(db: Arc<D>, sector: ProvenSector) -> Result<(), ServerError>
where
    D: Service + Send + Sync + 'static,
{
    for (commitment, location) in sector.pieces_locations {
        let piece_cid = commitment.cid();
        info!(%piece_cid, "indexing piece");

        let records = piece_indexes(&location).await?;
        // Move adding the index to the blocking pool. The RocksDB API is sync.
        match spawn_blocking({
            let db = Arc::clone(&db);
            move || db.add_index(piece_cid, records, true)
        })
        .await
        {
            Ok(Ok(_)) => {
                info!(%piece_cid, "indexing completed");
            }
            Ok(Err(err)) => {
                error!(%piece_cid, ?err, "piece indexing failed with an error");
            }
            Err(err) => {
                error!(%piece_cid, ?err, "piece indexing panicked");
            }
        };
    }

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
