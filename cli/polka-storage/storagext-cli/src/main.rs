#![deny(clippy::unwrap_used)]

mod cmd;
mod deser;
mod pair;

use std::fmt::Debug;

use clap::{ArgGroup, Parser, Subcommand};
use cmd::{market::MarketCommand, storage_provider::StorageProviderCommand, system::SystemCommand};
use deser::DealProposal;
use pair::{DebugPair, MultiPairSigner};
use subxt::ext::sp_core::{
    ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair, sr25519::Pair as Sr25519Pair,
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    filter::{self, FromEnvError},
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};
use url::Url;

pub(crate) const FULL_NODE_DEFAULT_RPC_ADDR: &str = "ws://127.0.0.1:42069";

#[derive(Debug, Parser)]
#[command(group(ArgGroup::new("keypair").args(
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
    #[arg(long, env, value_parser = DebugPair::<Sr25519Pair>::value_parser)]
    pub sr25519_key: Option<DebugPair<Sr25519Pair>>,

    /// ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, env, value_parser = DebugPair::<ECDSAPair>::value_parser)]
    pub ecdsa_key: Option<DebugPair<ECDSAPair>>,

    /// Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
    ///
    /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
    #[arg(long, env, value_parser = DebugPair::<Ed25519Pair>::value_parser)]
    pub ed25519_key: Option<DebugPair<Ed25519Pair>>,
}

#[derive(Debug, Subcommand)]
enum SubCommand {
    // Perform market operations.
    #[command(subcommand)]
    Market(MarketCommand),
    #[command(subcommand)]
    StorageProvider(StorageProviderCommand),
    #[command(subcommand)]
    System(SystemCommand),
}

impl SubCommand {
    #[tracing::instrument(level = "info", skip(self, node_rpc), fields(node_rpc = node_rpc.as_str()))]
    async fn run(
        self,
        node_rpc: Url,
        account_keypair: Option<MultiPairSigner>,
    ) -> Result<(), anyhow::Error> {
        match self {
            SubCommand::Market(cmd) => {
                cmd.run(node_rpc, account_keypair).await?;
            }
            SubCommand::StorageProvider(cmd) => {
                cmd.run(node_rpc, account_keypair).await?;
            }
            SubCommand::System(cmd) => {
                cmd.run(node_rpc).await?;
            }
        }

        Ok(())
    }
}

/// Configure and initalize tracing.
fn setup_tracing() -> Result<(), FromEnvError> {
    tracing_subscriber::registry()
        .with(
            fmt::layer()
                .with_filter(
                    EnvFilter::builder()
                        .with_default_directive(if cfg!(debug_assertions) {
                            LevelFilter::DEBUG.into()
                        } else {
                            LevelFilter::WARN.into()
                        })
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
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    setup_tracing()?;

    let cli_arguments = Cli::parse();

    let multi_pair_signer = MultiPairSigner::new(
        cli_arguments.sr25519_key.map(DebugPair::into_inner),
        cli_arguments.ecdsa_key.map(DebugPair::into_inner),
        cli_arguments.ed25519_key.map(DebugPair::into_inner),
    );

    cli_arguments
        .subcommand
        .run(cli_arguments.node_rpc, multi_pair_signer)
        .await?;
    Ok(())
}

/// Return a `clap::error::Error` with the [`MissingRequiredArgument`] kind.
///
/// It is impossible to print a proper usage error because clap makes all those useful constructs private.
///
/// <https://github.com/clap-rs/clap/blob/fe810907bdba9c81b980ed340addace44cefd8ff/clap_builder/src/parser/validator.rs#L454-L458>
/// <https://github.com/clap-rs/clap/blob/fe810907bdba9c81b980ed340addace44cefd8ff/clap_builder/src/output/usage.rs#L39-L58>
fn missing_keypair_error<C>() -> clap::error::Error
where
    C: clap::Subcommand,
{
    C::augment_subcommands(<Cli as clap::CommandFactory>::command()).error(
        clap::error::ErrorKind::MissingRequiredArgument,
        "signed extrinsics require a keypair",
    )
}

/// Print a message for the user warning the operation will take a bit.
fn operation_takes_a_while() {
    if !tracing::event_enabled!(tracing::Level::TRACE) {
        println!(concat!(
            "If you're curious about what's going on under the hood, try using `RUST_LOG=trace` on your next submission.\n\n",
            "This operation takes a while — we're submitting your transaction to the chain and ensuring all goes according to plan.\n",
            "Close your eyes, take a deep breath and think about blocks, running wild and free in a green field of bits.\n",
        ));
    }
}
