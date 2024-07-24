use clap::Subcommand;
use storagext::{storage_provider::StorageProviderClient, PolkaStorageConfig};
use subxt::ext::sp_core::crypto::Ss58Codec;
use url::Url;

#[derive(Debug, Subcommand)]
#[command(
    name = "storage-provider",
    about = "CLI Client to the Storage Provider Pallet",
    version
)]
pub enum StorageProviderCommand {
    /// Register account as a Storage Provider, so it can perform duties in Storage Provider Pallet.
    #[command(name = "register")]
    RegisterStorageProvider {
        /// PeerId in Storage Provider P2P network
        /// Can be any String for now.
        peer_id: String,
    },
}

impl StorageProviderCommand {
    /// Run a `storage-provider` command.
    ///
    /// Requires the target RPC address and a keypair able to sign transactions.
    #[tracing::instrument(
        level = "info",
        skip_all,
        fields(
            node_rpc,
            address = account_keypair.account_id().to_ss58check()
        )
    )]
    pub async fn run<Keypair>(
        self,
        node_rpc: Url,
        account_keypair: Keypair,
    ) -> Result<(), anyhow::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let client = StorageProviderClient::new(node_rpc).await?;
        match self {
            StorageProviderCommand::RegisterStorageProvider { peer_id } => {
                let block_hash = client
                    .register_storage_provider(&account_keypair, peer_id.clone())
                    .await?;
                tracing::info!(
                    "[{}] Successfully registered {} in Storage Provider Pallet",
                    block_hash,
                    peer_id
                );
            }
        }
        Ok(())
    }
}
