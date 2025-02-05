use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use futures::{pin_mut, StreamExt};
use local_index_directory::{IndexRecord, OffsetSize, Service};
use mater::stream_blocks_metadata;
use primitives::commitment::{CommP, Commitment};
use tokio::{fs::File, io::BufReader, sync::mpsc::UnboundedReceiver, task::spawn_blocking};
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
async fn on_index_piece<D, P>(
    db: Arc<D>,
    commitment: Commitment<CommP>,
    piece_path: P,
) -> Result<(), ServerError>
where
    D: Service + Send + Sync + 'static,
    P: AsRef<Path>,
{
    info!("indexing piece");

    let records = piece_indexes(piece_path).await?;
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
    let file = File::open(location).await?;
    let reader = BufReader::new(file);
    let blocks = stream_blocks_metadata(reader).await?;
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

#[cfg(test)]
pub mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::Arc,
    };

    use futures::{pin_mut, StreamExt};
    use mater::stream_blocks_metadata;
    use primitives::commitment::{CommP, Commitment};
    use tempfile::tempdir;
    use tokio::{fs::File, io::BufReader};

    use crate::{
        indexer::{
            local_index_directory::{
                rdb::{RocksDBLid, RocksDBStateStoreConfig},
                Service,
            },
            on_index_piece,
        },
        ServerError,
    };

    pub(crate) async fn index_piece<D, P>(
        db: Arc<D>,
        commitment: Commitment<CommP>,
        piece_path: P,
    ) -> Result<(), ServerError>
    where
        D: Service + Send + Sync + 'static,
        P: AsRef<Path>,
    {
        on_index_piece(db, commitment, piece_path).await
    }

    #[tokio::test]
    async fn test_on_index_piece() {
        // Index database
        let indexer_dir = tempdir().unwrap();
        let db = Arc::new(
            RocksDBLid::new(RocksDBStateStoreConfig {
                path: indexer_dir.path().into(),
            })
            .unwrap(),
        );

        // Index the piece
        let dummy_commitment = Commitment::<CommP>::from([0; 32]);
        let piece_path = PathBuf::from("tests/fixtures/spaceglenda_wrapped_v2.car");
        on_index_piece(Arc::clone(&db), dummy_commitment, piece_path.clone())
            .await
            .unwrap();

        // Index records should be returned
        let index = db.get_index(dummy_commitment.cid()).unwrap();
        assert_eq!(index.len(), 5);

        // Piece exists
        assert!(db.get_piece_metadata(dummy_commitment.cid()).is_ok());

        // Check indexed blocks
        let file = File::open(piece_path).await.unwrap();
        let reader = BufReader::new(file);
        let blocks = stream_blocks_metadata(reader).await.unwrap();
        pin_mut!(blocks);

        while let Some(Ok(data)) = blocks.next().await {
            // Check if piece exists for the block
            let indexed_pieces = db.pieces_containing_multihash(*data.cid.hash()).unwrap();
            assert_eq!(indexed_pieces, vec![dummy_commitment.cid()]);

            // Check the indexed offset size for the block
            let indexed_offset = db
                .get_offset_size(dummy_commitment.cid(), *data.cid.hash())
                .unwrap();
            assert_eq!(indexed_offset.offset, data.data_offset_source);
            assert_eq!(indexed_offset.size, data.data_size);
        }
    }
}
