pub mod client;
pub mod server;

/// Default parachain node adress.
pub(self) const DEFAULT_NODE_ADDRESS: &str = "ws://127.0.0.1:42069";

/// Default address to bind the RPC server to.
pub(self) const DEFAULT_LISTEN_ADDRESS: &str = "127.0.0.1:8000";

#[derive(Debug, thiserror::Error)]
pub enum RpcCommandError {
    #[error("the RPC server command failed with the following error: {0}")]
    Server(#[from] server::ServerCommandError),

    #[error("the RPC client command failed with the following error: {0}")]
    Client(#[from] client::ClientCommandError),
}

/// RPC commands, like the `server` and `client`
#[derive(Debug, clap::Subcommand)]
pub enum RpcCommand {
    /// Run the server.
    Server(server::ServerCommand),
    /// Run client RPC commands.
    Client(client::ClientCommand),
}

impl RpcCommand {
    pub(crate) async fn run(self) -> Result<(), RpcCommandError> {
        match self {
            Self::Server(cmd) => cmd.run().await?,
            Self::Client(cmd) => cmd.run().await?,
        }
        Ok(())
    }
}