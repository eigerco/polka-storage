use std::{path::PathBuf, time::Duration};

use cid::Cid;
use clap::{command, Parser};
use client::{Client, ClientSettings};
use libp2p::Multiaddr;
use tokio::time::timeout;
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{
    filter::FromEnvError, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

mod client;
mod p2p;

#[derive(Parser, Debug)]
#[command()]
struct Cli {
    /// Provider used for data download
    #[arg(long)]
    provider: Vec<Multiaddr>,

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

    /// Payload CID
    #[arg(long)]
    payload_cid: Cid,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    setup_tracing()?;

    let arguments = Cli::parse();

    let settings = ClientSettings::new(arguments.output, arguments.extract, arguments.overwrite);
    let client = Client::new(arguments.provider, arguments.payload_cid, settings).await?;

    let download_result = match arguments.timeout {
        Some(duration) => timeout(duration, client.download()).await,
        None => Ok(client.download().await),
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
