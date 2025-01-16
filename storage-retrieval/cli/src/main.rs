use std::{path::PathBuf, time::Duration};

use cid::Cid;
use clap::{command, Parser};
use libp2p::Multiaddr;
use polka_storage_retrieval::client::Client;
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{
    filter::FromEnvError, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

#[derive(Parser, Debug)]
#[command()]
struct Cli {
    /// Provider used for data download
    #[arg(long)]
    provider: Multiaddr,
    /// The CAR file to write to.
    #[arg(long)]
    output: PathBuf,
    /// Cancel the download if not completed after the specified duration in
    /// seconds. If not set the download will never timeout.
    #[arg(long, value_parser = parse_duration)]
    timeout: Option<Duration>,
    /// payload CID
    #[arg(long)]
    payload_cid: Cid,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    setup_tracing()?;

    let arguments = Cli::parse();

    let client = Client::new(
        arguments.output,
        vec![arguments.provider],
        vec![arguments.payload_cid],
        arguments.timeout,
    )
    .await?;

    match client.download().await {
        Ok(_) => info!("download finished"),
        Err(err) => error!(?err, "error while downloading"),
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
                        LevelFilter::WARN.into()
                    })
                    .from_env()?,
            ),
        )
        .init();
    Ok(())
}
