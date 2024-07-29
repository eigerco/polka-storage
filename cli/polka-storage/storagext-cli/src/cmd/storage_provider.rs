use std::{path::PathBuf, str::FromStr};

use anyhow::bail;
use clap::Subcommand;
use storagext::{storage_provider::StorageProviderClient, PolkaStorageConfig, RegisteredPoStProof};
use subxt::ext::sp_core::crypto::Ss58Codec;
use url::Url;

use crate::deser::{PreCommitSector, ProveCommitSector};

fn parse_post_proof(src: &str) -> Result<RegisteredPoStProof, anyhow::Error> {
    let post_proof = match src {
        "2KiB" => RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
        unknown => bail!("Unknown PoSt Proof type: {}", unknown),
    };

    Ok(post_proof)
}

#[derive(Debug, Clone)]
pub struct PreCommitSectorWrapper(PreCommitSector);

impl PreCommitSectorWrapper {
    /// Attempt to parse a command-line argument into [`PreCommitSector`].
    ///
    /// The command-line argument may be a valid JSON object, or a file path starting with @.
    pub(crate) fn parse(src: &str) -> Result<Self, anyhow::Error> {
        Ok(Self(if let Some(stripped) = src.strip_prefix('@') {
            let path = PathBuf::from_str(stripped)?.canonicalize()?;
            let mut file = std::fs::File::open(path)?;
            serde_json::from_reader(&mut file)
        } else {
            serde_json::from_str(src)
        }?))
    }
}

#[derive(Debug, Clone)]
pub struct ProveCommitSectorWrapper(ProveCommitSector);

impl ProveCommitSectorWrapper {
    /// Attempt to parse a command-line argument into [`ProveCommitSector`].
    ///
    /// The command-line argument may be a valid JSON object, or a file path starting with @.
    pub(crate) fn parse(src: &str) -> Result<Self, anyhow::Error> {
        Ok(Self(if let Some(stripped) = src.strip_prefix('@') {
            let path = PathBuf::from_str(stripped)?.canonicalize()?;
            let mut file = std::fs::File::open(path)?;
            serde_json::from_reader(&mut file)
        } else {
            serde_json::from_str(src)
        }?))
    }
}

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
        /// PoSt Proof Type
        /// Can only be "2KiB" which means `RegisteredPoStProof::StackedDRGWindow2KiBV1P1` for now.
        #[arg(long, value_parser = parse_post_proof, default_value = "2KiB")]
        post_proof: RegisteredPoStProof,
    },
    /// Pre-commit sector containing deals, so they can be proven.
    /// If deals have been published and not pre-commited and proven, they'll be slashed by Market Pallet.
    PreCommit {
        #[arg(value_parser = PreCommitSectorWrapper::parse)]
        pre_commit_sector: PreCommitSectorWrapper,
    },
    /// Proves sector that has been previously pre-committed.
    /// After proving, a deal in a sector is considered Active.
    ProveCommit {
        #[arg(value_parser = ProveCommitSectorWrapper::parse)]
        prove_commit_sector: ProveCommitSectorWrapper,
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
            StorageProviderCommand::RegisterStorageProvider {
                peer_id,
                post_proof,
            } => {
                let block_hash = client
                    .register_storage_provider(
                        &account_keypair,
                        peer_id.clone(),
                        post_proof.clone(),
                    )
                    .await?;
                tracing::info!(
                    "[{}] Successfully registered {}, seal: {:?} in Storage Provider Pallet",
                    block_hash,
                    peer_id,
                    post_proof
                );
            }
            StorageProviderCommand::PreCommit { pre_commit_sector } => {
                let sector_number = pre_commit_sector.0.sector_number;
                let block_hash = client
                    .pre_commit_sector(&account_keypair, pre_commit_sector.0.into())
                    .await?;

                tracing::info!(
                    "[{}] Successfully pre-commited sector {}.",
                    block_hash,
                    sector_number
                );
            }
            StorageProviderCommand::ProveCommit {
                prove_commit_sector,
            } => {
                let sector_number = prove_commit_sector.0.sector_number;
                let block_hash = client
                    .prove_commit_sector(&account_keypair, prove_commit_sector.0.into())
                    .await?;

                tracing::info!(
                    "[{}] Successfully proven sector {}.",
                    block_hash,
                    sector_number
                );
            }
        }
        Ok(())
    }
}
