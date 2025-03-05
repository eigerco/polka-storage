use std::{path::PathBuf, time::Duration};

use clap::{command, Parser};
use download::{DownloadClient, DownloadClientSettings};
use libp2p::{Multiaddr, PeerId};
use peer_resolver::find_multiaddr_storage_provider;
use storagext::{MarketClientExt, StorageProviderClientExt};
use tokio::time::timeout;
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{
    filter::FromEnvError, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

mod download;
mod peer_resolver;

#[derive(Parser, Debug)]
#[command()]
struct Cli {
    /// The output file to write to.
    #[arg(long)]
    output: PathBuf,

    /// Whether to overwrite existing files.
    #[arg(long, action)]
    overwrite: bool,

    /// Whether to extract the retrieved file.
    #[arg(long, action)]
    extract: bool,

    /// Cancel the download if not completed after the specified duration in
    /// seconds. If not set the download will never timeout.
    #[arg(long, value_parser = parse_duration)]
    timeout: Option<Duration>,

    /// Deal id
    #[arg(long)]
    deal_id: u64,

    /// Bootstrap node address
    #[arg(long)]
    bootstrap_address: Multiaddr,

    /// Bootstrap node peerid
    #[arg(long)]
    bootstrap_peer: PeerId,

    /// Parachain address
    #[arg(long)]
    parachain_address: String,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    setup_tracing()?;
    let args = Cli::parse();

    // Get deal info from the chain
    let parachain_client =
        storagext::Client::new(args.parachain_address, 0, Duration::ZERO).await?;
    let Some(deal_info) = parachain_client.retrieve_deal(args.deal_id).await? else {
        error!(deal_id = args.deal_id, "Deal not found on chain");
        return Ok(());
    };

    // Get storage provider from the chain
    let Some(storage_provider) = parachain_client
        .retrieve_storage_provider(&deal_info.provider.clone().into())
        .await?
    else {
        error!(
            deal_id = args.deal_id,
            "Storage provider not found on chain"
        );
        return Ok(());
    };
    let sp_peer_id = PeerId::from_bytes(&storage_provider.info.peer_id.0)?;

    // Peer id to multiaddress
    let multiaddrs =
        find_multiaddr_storage_provider(args.bootstrap_address, args.bootstrap_peer, sp_peer_id)
            .await?
            .into_iter()
            .map(|multiaddr| (sp_peer_id, multiaddr))
            .collect();

    let settings = DownloadClientSettings::new(args.output, args.extract, args.overwrite);
    let client = DownloadClient::new(multiaddrs, settings).await?;

    let download_result = match args.timeout {
        Some(duration) => timeout(duration, client.download(&deal_info.piece_cid)).await,
        None => Ok(client.download(&deal_info.piece_cid).await),
    };

    match download_result {
        Ok(Ok(_)) => info!("download successfully finished"),
        Ok(Err(err)) => error!(?err, "error occurred while downloading"),
        Err(_) => error!("download timeout"),
    }

    Ok(())
}

fn parse_duration(arg: &str) -> Result<Duration, String> {
    let seconds = arg
        .parse()
        .map_err(|err| format!("failed to parse duration from string: {}", err))?;
    Ok(Duration::from_secs(seconds))
}

/// Configure and initialize tracing.
fn setup_tracing() -> Result<(), FromEnvError> {
    tracing_subscriber::registry()
        .with(
            fmt::layer().with_filter(
                EnvFilter::builder()
                    .with_default_directive(if cfg!(debug_assertions) {
                        LevelFilter::DEBUG.into()
                    } else {
                        LevelFilter::INFO.into()
                    })
                    .from_env()?,
            ),
        )
        .init();
    Ok(())
}
