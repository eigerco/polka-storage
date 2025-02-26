//! The example showcases how to setup a retrieval server with the simple
//! blockstore. Because the server is simple it is used for manual testing of
//! the retrieval client.

use std::{any::type_name, env::args, sync::Arc, time::Duration};

use anyhow::{Context, Result};
use blockstore::Blockstore;
use libp2p::Multiaddr;
use mater::blockstore::ReadOnlyBlockstore;
use polka_storage_retrieval::server::Server;
use rand::prelude::*;
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncSeek},
};

const DEFAULT_PORT: u16 = 8989;

/// Adds random delays on the `get` calls.
struct ChaosReadOnlyStore<R>(ReadOnlyBlockstore<R>);

impl<R> Blockstore for ChaosReadOnlyStore<R>
where
    R: AsyncRead + AsyncSeek + Unpin + blockstore::cond_send::CondSync,
{
    fn get<const S: usize>(
        &self,
        cid: &cid::CidGeneric<S>,
    ) -> impl futures::Future<Output = blockstore::Result<Option<Vec<u8>>>>
           + blockstore::cond_send::CondSend {
        async {
            if rand::thread_rng().gen_bool(0.5) {
                let dur = Duration::from_millis(thread_rng().gen_range(250..=1000));
                tracing::info!("sleeping for {}", dur.as_millis());
                tokio::time::sleep(dur).await;
            }
            self.0.get(cid).await
        }
    }

    fn put_keyed<const S: usize>(
        &self,
        _: &cid::CidGeneric<S>,
        _: &[u8],
    ) -> impl futures::Future<Output = blockstore::Result<()>> + blockstore::cond_send::CondSend
    {
        async {
            Err(blockstore::Error::FatalDatabaseError(format!(
                "{} is read-only",
                type_name::<Self>()
            )))
        }
    }

    fn remove<const S: usize>(
        &self,
        _: &cid::CidGeneric<S>,
    ) -> impl futures::Future<Output = blockstore::Result<()>> + blockstore::cond_send::CondSend
    {
        async {
            Err(blockstore::Error::FatalDatabaseError(format!(
                "{} is read-only",
                type_name::<Self>()
            )))
        }
    }

    fn close(
        self,
    ) -> impl futures::Future<Output = blockstore::Result<()>> + blockstore::cond_send::CondSend
    {
        async { Ok(()) }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Init tracing
    let _guard = init_tracing();

    // Avoiding importing clap
    let args = args().collect::<Vec<_>>();
    let port = match args.get(1).map(|port| port.trim().parse::<u16>()) {
        Some(Ok(port)) => port,
        Some(Err(err)) => return Err(err).context("failed to parse server port"),
        None => DEFAULT_PORT,
    };

    let file = match args.get(2) {
        Some(path) => File::open(path).await?,
        // If there isn't a port, the argument will be at the first position
        None => match args.get(1) {
            Some(path) => File::open(path).await?,
            None => File::open("./mater/lib/tests/fixtures/car_v2/spaceglenda_wrapped.car").await?,
        },
    };

    // Example blockstore providing only a single file.
    let blockstore = Arc::new(ChaosReadOnlyStore(ReadOnlyBlockstore::new(file).await?));

    let roots = blockstore.write().await.roots().await?;
    tracing::info!("available roots: {:?}", roots);

    // Setup & run the server
    let server = Server::new(blockstore)?;
    let listener: Multiaddr = format!("/ip4/127.0.0.1/tcp/{}", port).parse()?;
    tracing::info!(multiaddress = %listener);

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
