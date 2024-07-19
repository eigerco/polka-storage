#![deny(clippy::unwrap_used)]

mod cmd;

use clap::{ArgGroup, Parser, Subcommand};
use cli::cmd::market::MarketCommand;
use storagext::PolkaStorageConfig;
use subxt_signer::{ecdsa, sr25519};
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
    #[arg(long, value_parser = parse_sr25519_keypair)]
    pub sr25519_key: Option<sr25519::Keypair>,

    #[arg(long, value_parser = parse_ecdsa_keypair)]
    pub ecdsa_key: Option<ecdsa::Keypair>,
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
                .run_with_keypair(cli_arguments.node_rpc, account_keypair)
                .await?
        }
        (_, Some(account_keypair)) => {
            cli_arguments
                .subcommand
                .run_with_keypair(cli_arguments.node_rpc, account_keypair)
                .await?
        }
        _ => unreachable!("should be handled by clap::ArgGroup"),
    }

    Ok(())
}

fn parse_sr25519_keypair(mut src: &str) -> Result<sr25519::Keypair, String> {
    if src.starts_with("0x") {
        src = &src[2..]
    }
    let mut key_bytes = [0u8; 32];
    hex::decode_to_slice(src, &mut key_bytes).unwrap();
    Ok(sr25519::Keypair::from_secret_key(key_bytes).unwrap())
}

fn parse_ecdsa_keypair(mut src: &str) -> Result<ecdsa::Keypair, String> {
    if src.starts_with("0x") {
        src = &src[2..]
    }
    let mut key_bytes = [0u8; 32];
    hex::decode_to_slice(src, &mut key_bytes).unwrap();
    Ok(ecdsa::Keypair::from_secret_key(key_bytes).unwrap())
}

/// Currency as specified by the SCALE-encoded runtime.
type Currency = u128;

/// BlockNumber as specified by the SCALE-encoded runtime.
type BlockNumber = u32;

/// CID wrapper to get deserialization.
#[derive(Debug, Clone)]
pub struct CidWrapper(Cid);

impl Deref for CidWrapper {
    type Target = Cid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Into<Cid> for CidWrapper {
    fn into(self) -> Cid {
        self.0
    }
}

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

#[derive(Debug, Clone, serde::Deserialize)]
pub struct DealProposal {
    pub piece_cid: CidWrapper,
    pub piece_size: u64,
    pub client: AccountId32,
    pub provider: AccountId32,
    pub label: String,
    pub start_block: BlockNumber,
    pub end_block: BlockNumber,
    pub storage_price_per_block: Currency,
    pub provider_collateral: Currency,
    pub state: DealState,
}
