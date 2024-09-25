#![deny(clippy::unwrap_used)]

mod cmd;
mod deser;

use std::{fmt::Debug, time::Duration};

use clap::{ArgGroup, Parser, Subcommand};
use cmd::{market::MarketCommand, storage_provider::StorageProviderCommand, system::SystemCommand};
use storagext::multipair::{DebugPair, MultiPairSigner};
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

/// The default value for the number of connection retries.
const DEFAULT_N_RETRIES: u32 = 10;

/// The default interval between connection retries.
///
/// It's a string because `clap` requires `Display` when using `default_value_t`,
/// which `std::time::Duration` does not implement.
const DEFAULT_RETRY_INTERVAL_MS: &str = "3000";

/// Parse milliseconds into [`Duration`].
fn parse_ms(s: &str) -> Result<Duration, String> {
    match s.parse() {
        Ok(ms) => Ok(Duration::from_millis(ms)),
        Err(err) => Err(err.to_string()),
    }
}

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

    /// The number of connection retries when trying to initially connect to the parachain.
    #[arg(long, env, default_value_t = DEFAULT_N_RETRIES)]
    pub n_retries: u32,

    /// The interval between connection retries, in milliseconds.
    #[arg(long, env, default_value = DEFAULT_RETRY_INTERVAL_MS, value_parser = parse_ms)]
    pub retry_interval: Duration,

    /// Output format.
    #[arg(long, env, value_parser = OutputFormat::value_parser, default_value_t = OutputFormat::Plain)]
    pub format: OutputFormat,
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
        n_retries: u32,
        retry_interval: Duration,
        output_format: OutputFormat,
    ) -> Result<(), anyhow::Error> {
        match self {
            SubCommand::Market(cmd) => {
                cmd.run(
                    node_rpc,
                    account_keypair,
                    n_retries,
                    retry_interval,
                    output_format,
                )
                .await?;
            }
            SubCommand::StorageProvider(cmd) => {
                cmd.run(
                    node_rpc,
                    account_keypair,
                    n_retries,
                    retry_interval,
                    output_format,
                )
                .await?;
            }
            SubCommand::System(cmd) => {
                cmd.run(node_rpc, n_retries, retry_interval, output_format)
                    .await?;
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
        .run(
            cli_arguments.node_rpc,
            multi_pair_signer,
            cli_arguments.n_retries,
            cli_arguments.retry_interval,
            cli_arguments.format,
        )
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
    // You can't trust the tracing enabled level for this purpose
    // https://docs.rs/tracing/latest/tracing/macro.enabled.html
    // https://users.rust-lang.org/t/how-to-get-to-tracing-subscriber-pub-fn-current-levelfilter-please/101575/3
    if std::env::var_os("DISABLE_XT_WAIT_WARNING").is_none() {
        eprintln!(concat!(
            "This operation takes a while — we're submitting your transaction to the chain and ensuring all goes according to plan.\n",
            "If you're curious about what's going on under the hood, try using `RUST_LOG=trace` on your next submission.\n",
            "To disable this message, set the environment variable `DISABLE_XT_WAIT_WARNING` to any value!\n\n",
            "Close your eyes, take a deep breath and think about blocks, running wild and free in a green field of bits.\n",
        ));
    }
}

/// Supported output formats.
#[derive(Debug, Clone)]
pub(crate) enum OutputFormat {
    /// Plaintext output, free-form, does not provide any format guarantees.
    Plain,
    /// JSON output.
    Json,
}

impl std::fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <Self as std::fmt::Debug>::fmt(&self, f)
    }
}

impl OutputFormat {
    fn value_parser(s: &str) -> Result<OutputFormat, String> {
        match s.to_lowercase().as_str() {
            "plain" => Ok(OutputFormat::Plain),
            "json" => Ok(OutputFormat::Json),
            format => Err(format!("unknown format: {}", format)),
        }
    }

    pub fn format<T>(&self, value: &T) -> Result<String, serde_json::Error>
    where
        T: std::fmt::Display + serde::Serialize,
    {
        match self {
            OutputFormat::Plain => Ok(value.to_string()),
            OutputFormat::Json => serde_json::to_string(value),
        }
    }
}
