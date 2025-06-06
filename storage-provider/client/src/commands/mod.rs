mod proofs;
mod wallet;

use std::path::PathBuf;

use clap::Parser;
use ed25519_dalek::pkcs8::{DecodePublicKey, PublicKeyBytes};
use jsonrpsee::core::ClientError;
use libp2p::{identity::ed25519::PublicKey as EdPubKey, PeerId};
use polka_storage_provider_common::rpc::StorageProviderRpcClient;
use primitives::{proofs::RegisteredSealProof, DealId};
use storagext::{
    deser::DeserializablePath,
    multipair::{MultiPairArgs, MultiPairSigner},
    runtime::storage_provider::calls::types::register_storage_provider::WindowPostProofType,
    types::storage_provider::{
        ClientDealProposal as SxtClientDealProposal, DealProposal as SxtDealProposal,
    },
};
use url::Url;

use self::proofs::ProofsCommand;
pub(super) use crate::commands::wallet::WalletCommand;
use crate::rpc_client::PolkaStorageRpcClient;

/// Default RPC server's URL.
const DEFAULT_RPC_SERVER_URL: &str = "http://127.0.0.1:8000";

/// CLI components error handling implementor.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("FromEnv error: {0}")]
    EnvError(#[from] tracing_subscriber::filter::FromEnvError),

    #[error("URL parse error: {0}")]
    ParseUrl(#[from] url::ParseError),

    #[error("Substrate error: {0}")]
    Substrate(#[from] subxt::Error),

    #[error(transparent)]
    SubstrateCli(#[from] sc_cli::Error),

    #[error("Error occurred while working with a car file: {0}")]
    MaterError(#[from] mater::Error),

    #[error(transparent)]
    UtilsCommand(#[from] crate::commands::proofs::UtilsCommandError),

    #[error("the RPC client failed: {0}")]
    RpcClient(#[from] ClientError),

    #[error("no signer key was provider")]
    NoSigner,

    #[error(transparent)]
    PubKeyError(#[from] ed25519_dalek::pkcs8::spki::Error),

    #[error(transparent)]
    DecodingError(#[from] libp2p::identity::DecodingError),

    #[error("PoRep params for seal proof {seal_proof:?} not found, please download or generate them first")]
    MissingPoRepParams { seal_proof: RegisteredSealProof },

    #[error(
        "PoSt params for PoSt type {post_type:?} not found, please download or generate them first"
    )]
    MissingPoStParams { post_type: WindowPostProofType },
}

/// A CLI application that facilitates management operations over a running full
/// node and other components.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub(crate) enum Cli {
    /// Command to manage wallet operations.
    #[command(subcommand)]
    Wallet(WalletCommand),

    /// Utility commands for storage related actions.
    #[command(subcommand)]
    Proofs(ProofsCommand),

    /// Retrieve information about the provider's node.
    Info {
        /// URL of the providers RPC server.
        #[arg(long, default_value = DEFAULT_RPC_SERVER_URL)]
        rpc_server_url: Url,
    },

    /// Propose a storage deal.
    ProposeDeal {
        /// URL of the providers RPC server.
        #[arg(long, default_value = DEFAULT_RPC_SERVER_URL)]
        rpc_server_url: Url,
        /// Storage deal to propose. Either JSON or a file path, prepended with an @.
        #[arg(value_parser = <SxtDealProposal as DeserializablePath>::deserialize_json )]
        deal_proposal: SxtDealProposal,
    },

    /// Publish a signed storage deal.
    PublishDeal {
        /// URL of the providers RPC server.
        #[arg(long, default_value = DEFAULT_RPC_SERVER_URL)]
        rpc_server_url: Url,
        /// Storage deal to publish. Either JSON or a file path, prepended with an @.
        #[arg(value_parser = <SxtClientDealProposal as DeserializablePath>::deserialize_json)]
        client_deal_proposal: SxtClientDealProposal,
    },

    /// Retrieve a deal proposal.
    RetrieveDeal {
        /// URL of the providers RPC server.
        #[arg(long, default_value = DEFAULT_RPC_SERVER_URL)]
        rpc_server_url: Url,
        /// The target deal ID.
        deal_id: DealId,
    },

    /// Sign a storage deal using the provided key, will output the deal as a JSON
    /// — no information is shared across the network.
    SignDeal {
        #[arg(value_parser = <SxtDealProposal as DeserializablePath>::deserialize_json)]
        deal_proposal: SxtDealProposal,

        #[command(flatten)]
        signer_key: MultiPairArgs,
    },

    /// Generate a Peer ID from a ED25519 public key pem file
    GeneratePeerID {
        /// Path to the ED25519 public key pem file
        #[arg(long)]
        pubkey: PathBuf,
    },
}

impl Cli {
    /// Parses command line arguments into the service configuration and runs the
    /// specified command with it.
    pub(crate) async fn run() -> Result<(), CliError> {
        // CLI arguments parsed and mapped to the struct.
        let cli_arguments: Cli = Cli::parse();

        match cli_arguments {
            Self::Wallet(cmd) => match cmd {
                WalletCommand::GenerateNodeKey(cmd) => Ok(cmd.run()?),
                WalletCommand::Generate(cmd) => Ok(cmd.run()?),
                WalletCommand::Inspect(cmd) => Ok(cmd.run()?),
                WalletCommand::InspectNodeKey(cmd) => Ok(cmd.run()?),
                WalletCommand::Vanity(cmd) => Ok(cmd.run()?),
                WalletCommand::Verify(cmd) => Ok(cmd.run()?),
                WalletCommand::Sign(cmd) => Ok(cmd.run()?),
            },
            Self::Proofs(utils) => Ok(utils.run().await?),
            Self::Info { rpc_server_url } => Self::info(rpc_server_url).await,
            Self::ProposeDeal {
                rpc_server_url,
                deal_proposal,
            } => Self::propose_deal(rpc_server_url, deal_proposal).await,
            Self::PublishDeal {
                rpc_server_url,
                client_deal_proposal,
            } => Self::publish_deal(rpc_server_url, client_deal_proposal).await,
            Self::RetrieveDeal {
                rpc_server_url,
                deal_id,
            } => Self::retrieve_deal(rpc_server_url, deal_id).await,
            Self::SignDeal {
                deal_proposal,
                signer_key,
            } => Self::sign_deal(deal_proposal, signer_key),
            Self::GeneratePeerID { pubkey } => Self::generate_peer_id(pubkey),
        }
    }

    async fn info(rpc_server_url: Url) -> Result<(), CliError> {
        let client = PolkaStorageRpcClient::new(&rpc_server_url).await?;
        let info = client.info().await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&info)
                .expect("type is serializable so this call should never fail")
        );
        Ok(())
    }

    async fn propose_deal(
        rpc_server_url: Url,
        deal_proposal: SxtDealProposal,
    ) -> Result<(), CliError> {
        let client = PolkaStorageRpcClient::new(&rpc_server_url).await?;
        let result = client.propose_deal(deal_proposal).await?;
        println!("{}", result);
        Ok(())
    }

    async fn publish_deal(
        rpc_server_url: Url,
        client_deal_proposal: SxtClientDealProposal,
    ) -> Result<(), CliError> {
        let client = PolkaStorageRpcClient::new(&rpc_server_url).await?;
        let result = client.publish_deal(client_deal_proposal).await?;
        println!("Successfully published deal of id: {}", result);
        Ok(())
    }

    async fn retrieve_deal(rpc_server_url: Url, deal_id: DealId) -> Result<(), CliError> {
        let client = PolkaStorageRpcClient::new(&rpc_server_url).await?;
        let result = client.retrieve_deal(deal_id).await?;
        println!("{}", result);
        Ok(())
    }

    fn sign_deal(
        deal_proposal: SxtDealProposal,
        signer_key: MultiPairArgs,
    ) -> Result<(), CliError> {
        let Some(signer) = Option::<MultiPairSigner>::from(signer_key) else {
            return Err(CliError::NoSigner);
        };

        let signature = deal_proposal.sign_serializable(&signer);

        println!(
            "{}",
            serde_json::to_string_pretty(&signature)
                .expect("the type is serializable, so this should never fail")
        );
        Ok(())
    }

    fn generate_peer_id(path: PathBuf) -> Result<(), CliError> {
        let pubkey_bytes = PublicKeyBytes::read_public_key_pem_file(path)?;
        let pubkey = EdPubKey::try_from_bytes(&pubkey_bytes.to_bytes())?;
        let key = libp2p::identity::PublicKey::from(pubkey);
        let peer_id = PeerId::from_public_key(&key);

        println!("{peer_id}");
        Ok(())
    }
}
