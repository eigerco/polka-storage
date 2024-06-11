//! A CLI application that facilitates management operations over a running full node and other components.

#![deny(unused_crate_dependencies)]

mod cli;

pub(crate) mod commands;
pub(crate) use cli::Cli;
use cli_primitives::Result;
use commands::runner;

#[tokio::main]
async fn main() -> Result<()> {
    // Logger initialization.
    env_logger::init();

    // Run requested command.
    runner::run().await
}
