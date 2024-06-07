//! A CLI application that facilitates management operations over a running full node and other components.
#![deny(unused_crate_dependencies)]

mod cli;
pub(crate) mod commands;
mod rpc;
mod substrate;

pub(crate) use cli::Cli;
use cli_primitives::Result;
use commands::runner;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Logger initialization.
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    // Run requested command.
    runner::run().await
}
