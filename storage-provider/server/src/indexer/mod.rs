use std::{fmt::Debug, path::Path, sync::Arc};

use local_index_directory::{IndexRecord, OffsetSize, Service};
use mater::CarV2Reader;
use polka_storage_provider_common::sector::ProvenSector;
use primitives::commitment::{CommP, Commitment};
use tokio::{fs::File, io::BufReader, sync::mpsc::UnboundedReceiver, task::spawn_blocking};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{debug, error, info, instrument};

use crate::ServerError;

pub mod local_index_directory;

#[derive(Debug, Clone)]
pub enum IndexerMessage {
    /// Start indexing the sector
    IndexSector(ProvenSector),
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
        IndexerMessage::IndexSector(sector) => {
            sector
                .pieces_locations
                .into_iter()
                .for_each(|(commitment, piece_path)| {
                    let db = Arc::clone(&db);
                    tracker.spawn(index_piece(db, commitment, piece_path));
                });
        }
    }
}

#[instrument(skip_all, fields(piece_cid = %commitment.cid()))]
async fn index_piece<D, P>(db: Arc<D>, commitment: Commitment<CommP>, piece_path: P)
where
    D: Service + Send + Sync + 'static,
    P: AsRef<Path> + Debug,
{
    let records = match piece_indexes(&piece_path).await {
        Ok(records) => records,
        Err(err) => {
            error!(?piece_path, ?err, "piece indexing failed with an error");
            return;
        }
    };

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
}

/// Prepares indexes of a raw piece.
async fn piece_indexes<P>(location: P) -> Result<Vec<IndexRecord>, ServerError>
where
    P: AsRef<Path>,
{
    let file = File::open(location).await?;
    let reader = BufReader::new(file);
    let mut reader = CarV2Reader::new(reader);

    reader.read_pragma().await?;
    let header = reader.read_header().await?;
    let _v1_header = reader.read_v1_header().await?;
    let data_end = header.data_offset + header.data_size;

    let mut records = vec![];
    loop {
        let metadata = reader.read_block_metadata().await?;
        let position = metadata.data_offset_source + metadata.data_size;

        records.push(IndexRecord {
            cid: metadata.cid,
            offset_size: OffsetSize {
                offset: metadata.data_offset_source,
                size: metadata.data_size,
            },
        });

        // This is the last block
        if position >= data_end {
            break;
        }
    }

    Ok(records)
}

#[cfg(test)]
pub mod tests {
    use std::{
        fmt::Debug,
        path::{Path, PathBuf},
        sync::Arc,
    };

    use mater::CarV2Reader;
    use primitives::commitment::{CommP, Commitment};
    use tempfile::tempdir;
    use tokio::{fs::File, io::BufReader};

    use crate::indexer::{
        index_piece,
        local_index_directory::{
            rdb::{RocksDBLid, RocksDBStateStoreConfig},
            IndexRecord, OffsetSize, Service,
        },
    };

    pub(crate) async fn index_piece_util<D, P>(
        db: Arc<D>,
        commitment: Commitment<CommP>,
        piece_path: P,
    ) where
        D: Service + Send + Sync + 'static,
        P: AsRef<Path> + Debug,
    {
        index_piece(db, commitment, piece_path).await
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
        index_piece(Arc::clone(&db), dummy_commitment, piece_path.clone()).await;

        // Index records should be returned
        let index = db.get_index(dummy_commitment.cid()).unwrap();
        assert_eq!(index.len(), 5);

        // Piece exists
        assert!(db.get_piece_metadata(dummy_commitment.cid()).is_ok());

        // Check indexed blocks
        let file = File::open(piece_path).await.unwrap();
        let reader = BufReader::new(file);
        let mut reader = CarV2Reader::new(reader);

        reader.read_pragma().await.unwrap();
        let header = reader.read_header().await.unwrap();
        let _v1_header = reader.read_v1_header().await.unwrap();
        let data_end = header.data_offset + header.data_size;

        let mut records = vec![];
        loop {
            let metadata = reader.read_block_metadata().await.unwrap();
            let position = metadata.data_offset_source + metadata.data_size;

            // Check if piece exists for the block
            let indexed_pieces = db
                .pieces_containing_multihash(*metadata.cid.hash())
                .unwrap();
            assert_eq!(indexed_pieces, vec![dummy_commitment.cid()]);

            // Check the indexed offset size for the block
            let indexed_offset = db
                .get_offset_size(dummy_commitment.cid(), *metadata.cid.hash())
                .unwrap();
            assert_eq!(indexed_offset.offset, metadata.data_offset_source);
            assert_eq!(indexed_offset.size, metadata.data_size);

            records.push(IndexRecord {
                cid: metadata.cid,
                offset_size: OffsetSize {
                    offset: metadata.data_offset_source,
                    size: metadata.data_size,
                },
            });

            // This is the last block
            if position >= data_end {
                break;
            }
        }
    }
}
