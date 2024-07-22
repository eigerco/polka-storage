#![deny(clippy::unwrap_used)]

mod cmd;

use std::fmt::Debug;

use cid::Cid;
use clap::{ArgGroup, Parser, Subcommand};
use cmd::market::MarketCommand;
use storagext::{BlockNumber, Currency, PolkaStorageConfig};
use subxt::ext::sp_core::crypto::Ss58Codec;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    filter, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};
use url::Url;

pub(crate) const FULL_NODE_DEFAULT_RPC_ADDR: &str = "ws://127.0.0.1:9944";

#[derive(Debug, Parser)]
#[command(group(ArgGroup::new("key").required(true).args(&["sr25519_key", "ecdsa_key"])))]
struct Cli {
    #[command(subcommand)]
    pub subcommand: SubCommand,

    /// URL of the providers RPC server.
    #[arg(long, default_value = FULL_NODE_DEFAULT_RPC_ADDR)]
    pub node_rpc: Url,

    /// An hex encoded Sr25519 key
    #[arg(long, value_parser = DebugPair::<subxt::ext::sp_core::sr25519::Pair>::value_parser)]
    pub sr25519_key: Option<DebugPair<subxt::ext::sp_core::sr25519::Pair>>,

    #[arg(long, value_parser = DebugPair::<subxt::ext::sp_core::ecdsa::Pair>::value_parser)]
    pub ecdsa_key: Option<DebugPair<subxt::ext::sp_core::ecdsa::Pair>>,
}

#[derive(Debug, Subcommand)]
pub enum SubCommand {
    // Perform market operations.
    #[command(subcommand)]
    Market(MarketCommand),
}

impl SubCommand {
    async fn run_with_keypair<Keypair>(
        self,
        node_rpc: Url,
        account_keypair: Keypair,
    ) -> Result<(), anyhow::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        match self {
            SubCommand::Market(cmd) => {
                cmd.run(node_rpc, account_keypair).await?;
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // TODO: replace the box/dyn
    // Logger initialization.
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_filter(
                    EnvFilter::builder()
                        .with_default_directive(LevelFilter::INFO.into())
                        .from_env()?,
                )
                .with_filter(filter::filter_fn(|metadata| {
                    if let Some(module_path) = metadata.module_path() {
                        module_path.starts_with("storagext")
                    } else {
                        true
                    }
                })),
        )
        .init();

    let cli_arguments = Cli::parse();

    match (cli_arguments.sr25519_key, cli_arguments.ecdsa_key) {
        (Some(account_keypair), _) => {
            cli_arguments
                .subcommand
                .run_with_keypair(
                    cli_arguments.node_rpc,
                    subxt::tx::PairSigner::new(account_keypair.0),
                )
                .await?
        }
        (_, Some(account_keypair)) => {
            cli_arguments
                .subcommand
                .run_with_keypair(
                    cli_arguments.node_rpc,
                    subxt::tx::PairSigner::new(account_keypair.0),
                )
                .await?
        }
        _ => unreachable!("should be handled by clap::ArgGroup"),
    }

    Ok(())
}

#[derive(Clone)]
struct DebugPair<Pair>(Pair)
where
    Pair: subxt::ext::sp_core::Pair;

impl<Pair> Debug for DebugPair<Pair>
where
    Pair: subxt::ext::sp_core::Pair,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("DebugPair")
            .field(&self.0.public().to_ss58check())
            .finish()
    }
}

impl<Pair> DebugPair<Pair>
where
    Pair: subxt::ext::sp_core::Pair,
{
    fn value_parser(src: &str) -> Result<Self, String> {
        Ok(Self(
            Pair::from_string(&src, None).map_err(|err| format!("{}", err))?,
        ))
    }
}

/// CID doesn't deserialize from a string, hence we need our work wrapper.
///
/// <https://github.com/multiformats/rust-cid/issues/162>
#[derive(Debug, Clone)]
pub struct CidWrapper(Cid);

// The CID has some issues that require a workaround for strings.
// For more details, see: <https://github.com/multiformats/rust-cid/issues/162>
impl<'de> serde::de::Deserialize<'de> for CidWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self(
            Cid::try_from(s.as_str()).map_err(|e| serde::de::Error::custom(format!("{e:?}")))?,
        ))
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ActiveDealState {
    pub sector_number: u64,
    pub sector_start_block: BlockNumber,
    pub last_updated_block: Option<BlockNumber>,
    pub slash_block: Option<BlockNumber>,
}

impl Into<storagext::ActiveDealState<BlockNumber>> for ActiveDealState {
    fn into(self) -> storagext::ActiveDealState<BlockNumber> {
        storagext::ActiveDealState {
            sector_number: self.sector_number,
            sector_start_block: self.sector_start_block,
            last_updated_block: self.last_updated_block,
            slash_block: self.slash_block,
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub enum DealState {
    Published,
    Active(ActiveDealState),
}

impl Into<storagext::DealState<BlockNumber>> for DealState {
    fn into(self) -> storagext::DealState<BlockNumber> {
        match self {
            DealState::Published => storagext::DealState::Published,
            DealState::Active(v) => storagext::DealState::Active(v.into()),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DealProposal {
    pub piece_cid: CidWrapper,
    pub piece_size: u64,
    pub client: <PolkaStorageConfig as subxt::Config>::AccountId,
    pub provider: <PolkaStorageConfig as subxt::Config>::AccountId,
    pub label: String,
    pub start_block: BlockNumber,
    pub end_block: BlockNumber,
    pub storage_price_per_block: Currency,
    pub provider_collateral: Currency,
    pub state: DealState,
}

impl Into<storagext::DealProposal> for DealProposal {
    fn into(self) -> storagext::DealProposal {
        storagext::DealProposal {
            piece_cid: self.piece_cid.0,
            piece_size: self.piece_size,
            client: self.client,
            provider: self.provider,
            label: self.label,
            start_block: self.start_block,
            end_block: self.end_block,
            storage_price_per_block: self.storage_price_per_block,
            provider_collateral: self.provider_collateral,
            state: self.state.into(),
        }
    }
}