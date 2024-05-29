use clap::Parser;
use url::Url;

/// The `run-rpc` command used to run a server to listen for RPC calls.
#[derive(Debug, Clone, Parser)]
pub(crate) struct RunRpcCmd {
    /// RPC API endpoint of the parachain node.
    #[arg(short = 'n', long, default_value = "ws://127.0.0.1:9944")]
    pub node_rpc_address: Url,
}
