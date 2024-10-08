pub mod client;
pub mod server;

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
