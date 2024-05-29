use clap::Parser;

/// The `stop-rpc` command used to stop a server that listen for RPC calls.
#[derive(Debug, Clone, Parser)]
pub(crate) struct StopRpcCmd {}
