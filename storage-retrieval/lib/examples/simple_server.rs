//! The example showcases how to setup a retrieval server with the simple
//! blockstore. Because the server is simple it is used for manual testing of
//! the retrieval client.

use std::sync::Arc;

use anyhow::Result;
use libp2p::Multiaddr;
use mater::FileBlockstore;
use polka_storage_retrieval::server::Server;

#[tokio::main]
async fn main() -> Result<()> {
    // Init tracing
    let _guard = init_tracing();

    // Example blockstore providing only a single file.
    let blockstore = Arc::new(
        FileBlockstore::from_existing("./mater/lib/tests/fixtures/car_v2/spaceglenda_wrapped.car")
            .await?,
    );

    // Setup & run the server
    let server = Server::new(blockstore)?;
    let listener: Multiaddr = format!("/ip4/127.0.0.1/tcp/8989").parse()?;
    server.run(vec![listener], std::future::pending()).await?;

    Ok(())
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
