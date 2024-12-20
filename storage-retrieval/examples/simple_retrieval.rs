use std::{str::FromStr, sync::Arc, time::Duration};

use anyhow::Result;
use blockstore::{
    block::{Block, CidError},
    Blockstore, InMemoryBlockstore,
};
use cid::Cid;
use libp2p::Multiaddr;
use multihash_codetable::{Code, MultihashDigest};
use storage_retrieval::{client::Client, server::Server};
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<()> {
    // Init tracing
    let _guard = init_tracing();

    // Setup indexer
    // let indexer_path = temp_dir();
    // let indexer = Arc::new(RocksDBLid::new(RocksDBStateStoreConfig {
    //     path: indexer_path,
    // })?);

    // TODO: Blocks should not be hold in memory. Implement blockstore that can
    // source blocks directly from sectors on disk with the help of an index.
    let blockstore = Arc::new(InMemoryBlockstore::<64>::new());
    blockstore.put(StringBlock("12345".to_string())).await?;

    // Setup server
    let server = Server::new(blockstore)?;
    let listener: Multiaddr = format!("/ip4/127.0.0.1/tcp/8989").parse()?;

    tokio::spawn({
        let listener = listener.clone();
        async move {
            let _ = server.run(vec![listener]).await;
        }
    });

    // TODO: Implement blockstore that persist blocks directly to disk as car file.
    let blockstore = Arc::new(InMemoryBlockstore::<64>::new());
    let client = Client::new(blockstore, vec![listener])?;

    // Payload cid of the car file we want to fetch
    // let payload_cid =
    //     Cid::from_str("bafkreiechz74drg7tg5zswmxf4g2dnwhemlwdv7e3l5ypehdqdwaoyz3dy").unwrap();
    let payload_cid =
        Cid::from_str("bafkreiczsrdrvoybcevpzqmblh3my5fu6ui3tgag3jm3hsxvvhaxhswpyu").unwrap();
    client
        .download(payload_cid, sleep(Duration::from_secs(10)))
        .await?;

    Ok(())
}

struct StringBlock(pub String);

impl Block<64> for StringBlock {
    fn cid(&self) -> Result<Cid, CidError> {
        const RAW_CODEC: u64 = 0x55;
        let hash = Code::Sha2_256.digest(self.0.as_ref());
        Ok(Cid::new_v1(RAW_CODEC, hash))
    }

    fn data(&self) -> &[u8] {
        self.0.as_ref()
    }
}

fn init_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());

    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .event_format(
            tracing_subscriber::fmt::format()
                .with_file(true)
                .with_line_number(true),
        )
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .init();

    guard
}
