//! A CLI application that facilitates management operations over a running full node and other components.
#![deny(unused_crate_dependencies)]
#![deny(clippy::unwrap_used)]
// TODO(#274,@cernicc,26/08/2024): Remove after #274 is done
#![allow(dead_code)]

pub(crate) mod commands;
mod rpc;
mod storage;

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
