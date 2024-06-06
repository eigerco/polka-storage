//! A CLI application that facilitates management operations over a running full node and other components.

#![deny(unused_crate_dependencies)]

mod cli;
mod polkadot;
mod rpc;

pub(crate) mod commands;
pub(crate) use cli::Cli;
use cli_primitives::Result;
use commands::runner;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    // Logger initialization.
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    // Run requested command.
    runner::run().await
}
