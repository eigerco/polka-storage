use std::{path::PathBuf, time::Duration};

use anyhow::{anyhow, bail};
use cid::Cid;
use clap::{command, Parser, Subcommand};
use libp2p::{Multiaddr, PeerId};
use storagext::{MarketClientExt, StorageProviderClientExt};
use tracing::{error, info, level_filters::LevelFilter};
use tracing_subscriber::{
    filter::FromEnvError, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

use crate::{
    download::{DownloadClient, DownloadClientSettings},
    p2p::resolvers::{get_multiaddr_storage_provider, get_piece_info},
};

mod download;
mod p2p;

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    subcommand: SubCommand,

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
}

#[derive(Debug, Subcommand)]
pub enum SubCommand {
    #[command()]
    ByPayloadCid {
        /// Provider multiaddres used for the data download
        #[arg(long)]
        provider: Vec<Multiaddr>,

        /// Cid of the data being downloaded
        #[arg(long)]
        payload_cid: Cid,
    },
    #[command()]
    ByDealId {
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
    },
}

impl SubCommand {
    pub async fn run(
        self,
        output: PathBuf,
        overwrite: bool,
        extract: bool,
        timeout: Option<Duration>,
    ) -> Result<(), anyhow::Error> {
        let (providers, payload_cid) = match self {
            SubCommand::ByPayloadCid {
                provider,
                payload_cid,
            } => {
                // We know the provider and the payload cid. Nothing more to do.
                (provider, payload_cid)
            }
            SubCommand::ByDealId {
                deal_id,
                bootstrap_address,
                bootstrap_peer,
                parachain_address,
            } => {
                // Get deal info from the chain
                let parachain_client =
                    storagext::Client::new(parachain_address, 0, Duration::ZERO).await?;
                let Some(deal_info) = parachain_client.retrieve_deal(deal_id).await? else {
                    bail!("Deal not found on chain {deal_id}");
                };

                // Get storage provider from the chain
                let Some(storage_provider) = parachain_client
                    .retrieve_storage_provider(&subxt_core::utils::AccountId32(
                        deal_info.provider.into(),
                    ))
                    .await?
                else {
                    bail!("Storage provider not found on chain");
                };
                let sp_peer_id = PeerId::from_bytes(&storage_provider.info.peer_id.0)?;

                // Peer id to multiaddress
                let multiaddrs =
                    get_multiaddr_storage_provider(bootstrap_peer, bootstrap_address, sp_peer_id)
                        .await?;
                tracing::debug!(?multiaddrs, "Found multiaddress for peer");

                // Find payload cid
                let piece_info =
                    get_piece_info(sp_peer_id, multiaddrs[0].clone(), deal_info.piece_cid).await?;
                let payload_cid = piece_info
                    .roots
                    .get(0)
                    .ok_or_else(|| anyhow!("no roots received for a piece"))?;

                (multiaddrs, *payload_cid)
            }
        };

        let settings = DownloadClientSettings::new(output, extract, overwrite);
        let client = DownloadClient::new(providers, payload_cid, settings).await?;

        let download_result = match timeout {
            Some(duration) => tokio::time::timeout(duration, client.download()).await,
            None => Ok(client.download().await),
        };

        match download_result {
            Ok(Ok(_)) => {
                info!("download successfully finished")
            }
            Ok(Err(err)) => error!(?err, "error occurred while downloading"),
            Err(_) => error!("download timeout"),
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    setup_tracing()?;

    let cli_arguments = Cli::parse();

    cli_arguments
        .subcommand
        .run(
            cli_arguments.output,
            cli_arguments.overwrite,
            cli_arguments.extract,
            cli_arguments.timeout,
        )
        .await?;

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
