use std::{fmt::Debug, path::Path, sync::Arc};

use cid::Cid;
use local_index_directory::{IndexRecord, OffsetSize, Service};
use mater::{CarV1Reader, CarV1ReaderExt, CarV2Reader};
use polka_storage_provider_common::sector::ProvenSector;
use primitives::commitment::{CommP, Commitment};
use tokio::{fs::File, io::BufReader, sync::mpsc::UnboundedReceiver, task::spawn_blocking};
use tokio_util::{sync::CancellationToken, task::TaskTracker};
use tracing::{error, info, instrument};

use crate::ServerError;

pub mod local_index_directory;

#[derive(Debug, Clone)]
pub enum IndexerMessage {
    /// Start indexing the sector
    IndexSector(ProvenSector),
}

#[derive(Clone)]
pub struct IndexerState<I> {
    pub lid: Arc<I>,
}

pub async fn start_indexer<I>(
    state: IndexerState<I>,
    mut indexer_rx: UnboundedReceiver<IndexerMessage>,
    token: CancellationToken,
) -> Result<(), ServerError>
where
    I: Service + Send + Sync + 'static,
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

fn on_command<I>(command: IndexerMessage, db: Arc<I>, tracker: &TaskTracker)
where
    I: Service + Send + Sync + 'static,
{
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
async fn index_piece<I, P>(db: Arc<I>, commitment: Commitment<CommP>, piece_path: P)
where
    I: Service + Send + Sync + 'static,
    P: AsRef<Path>,
{
    info!("indexing started");

    let (roots, records) = match piece_indexes(&piece_path).await {
        Ok(data) => data,
        Err(err) => {
            error!(piece_path = ?piece_path.as_ref(), ?err, "piece indexing failed with an error");
            return;
        }
    };

    // Move adding the index to the blocking pool. The RocksDB API is sync.
    match spawn_blocking({
        let db = Arc::clone(&db);
        move || db.add_index(commitment.cid(), roots, records, true)
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
            error!(?err, "failed to join task");
        }
    };
}

/// Returns the root CIDs and the index records mapping CIDs to their respective
/// offsets and sizes.
async fn piece_indexes<P>(location: P) -> Result<(Vec<Cid>, Vec<IndexRecord>), ServerError>
where
    P: AsRef<Path>,
{
    let file = File::open(location).await?;
    let mut reader = BufReader::new(file);

    reader.read_pragma().await?;
    let header = reader.read_v2_header().await?;
    let v1_header = reader.read_v1_header().await?;
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

    Ok((v1_header.roots, records))
}

#[cfg(test)]
pub mod tests {
    use std::{path::PathBuf, sync::Arc};

    use mater::{CarV1Reader, CarV1ReaderExt, CarV2Reader};
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
        let mut reader = BufReader::new(file);

        reader.read_pragma().await.unwrap();
        let header = reader.read_v2_header().await.unwrap();
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
