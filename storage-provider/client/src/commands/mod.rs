mod proofs;

use clap::Parser;
use primitives::proofs::RegisteredSealProof;
use storagext::{
    deser::DeserializablePath,
    multipair::{MultiPairArgs, MultiPairSigner},
    runtime::storage_provider::calls::types::register_storage_provider::WindowPostProofType,
    types::storage_provider::DealProposal as SxtDealProposal,
};

use self::proofs::ProofsCommand;

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

    #[error("Error occurred while working with a car file: {0}")]
    MaterError(#[from] mater::Error),

    #[error(transparent)]
    UtilsCommand(#[from] crate::commands::proofs::UtilsCommandError),

    #[error("no signer key was provider")]
    NoSigner,

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
    /// Utility commands for storage related actions.
    #[command(subcommand)]
    Proofs(ProofsCommand),

    /// Sign a storage deal using the provided key, will output the deal as a JSON
    /// — no information is shared across the network.
    SignDeal {
        #[arg(value_parser = <SxtDealProposal as DeserializablePath>::deserialize_json)]
        deal_proposal: SxtDealProposal,

        #[command(flatten)]
        signer_key: MultiPairArgs,
    },
}

impl Cli {
    /// Parses command line arguments into the service configuration and runs the
    /// specified command with it.
    pub(crate) async fn run() -> Result<(), CliError> {
        // CLI arguments parsed and mapped to the struct.
        let cli_arguments: Cli = Cli::parse();

        match cli_arguments {
            Self::Proofs(utils) => Ok(utils.run().await?),
            Self::SignDeal {
                deal_proposal,
                signer_key,
            } => Self::sign_deal(deal_proposal, signer_key),
        }
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
}
