//! A CLI application that facilitates management operations over a running full node and other components.
#![warn(unused_crate_dependencies)]
#![deny(clippy::unwrap_used)]

// Rust thinks that primitives_p2p is unused even though it's in dev_dependencies & used in examples
#[cfg(test)]
use primitives_p2p as _;

pub(crate) mod commands;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::commands::CliError;

#[tokio::main]
async fn main() -> Result<(), CliError> {
    // Logger initialization.
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env()?,
        )
        .init();

    // Run requested command.
    commands::Cli::run().await
}
