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

pub(crate) const FULL_NODE_DEFAULT_RPC_ADDR: &str = "ws://127.0.0.1:42069";

#[derive(Debug, Parser)]
#[command(group(ArgGroup::new("keypair").required(true).args(
    &["sr25519_key", "ecdsa_key", "ed25519_key"]
)))]
struct Cli {
    #[command(subcommand)]
    pub subcommand: SubCommand,

    /// RPC server's URL.
    #[arg(long, default_value = FULL_NODE_DEFAULT_RPC_ADDR)]
    pub node_rpc: Url,

    /// Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, value_parser = DebugPair::<subxt::ext::sp_core::sr25519::Pair>::value_parser)]
    pub sr25519_key: Option<DebugPair<subxt::ext::sp_core::sr25519::Pair>>,

    /// ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, value_parser = DebugPair::<subxt::ext::sp_core::ecdsa::Pair>::value_parser)]
    pub ecdsa_key: Option<DebugPair<subxt::ext::sp_core::ecdsa::Pair>>,

    /// Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, value_parser = DebugPair::<subxt::ext::sp_core::ed25519::Pair>::value_parser)]
    pub ed25519_key: Option<DebugPair<subxt::ext::sp_core::ed25519::Pair>>,
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

    match (
        cli_arguments.sr25519_key,
        cli_arguments.ecdsa_key,
        cli_arguments.ed25519_key,
    ) {
        (Some(account_keypair), _, _) => {
            cli_arguments
                .subcommand
                .run_with_keypair(
                    cli_arguments.node_rpc,
                    subxt::tx::PairSigner::new(account_keypair.0),
                )
                .await?
        }
        (_, Some(account_keypair), _) => {
            cli_arguments
                .subcommand
                .run_with_keypair(
                    cli_arguments.node_rpc,
                    subxt::tx::PairSigner::new(account_keypair.0),
                )
                .await?
        }
        (_, _, Some(account_keypair)) => {
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
    pub state: storagext::runtime::runtime_types::pallet_market::pallet::DealState<BlockNumber>,
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
