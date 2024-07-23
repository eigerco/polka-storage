use std::{path::PathBuf, str::FromStr};

use clap::Subcommand;
use primitives_proofs::DealId;
use storagext::{market::MarketClient, PolkaStorageConfig};
use url::Url;

use crate::DealProposal;

/// List of [`DealProposal`]s to publish.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DealProposals(Vec<DealProposal>);

impl DealProposals {
    /// Attempt to parse a command-line argument into [`DealProposals`].
    ///
    /// The command-line argument may be a valid JSON object, or a file path starting with @.
    fn parse(src: &str) -> Result<Self, anyhow::Error> {
        Ok(if let Some(stripped) = src.strip_prefix('@') {
            let path = PathBuf::from_str(stripped)?.canonicalize()?;
            let mut file = std::fs::File::open(path)?;
            serde_json::from_reader(&mut file)
        } else {
            serde_json::from_str(src)
        }?)
    }
}

#[derive(Debug, Subcommand)]
#[command(name = "market", about = "CLI Client to the Market Pallet", version)]
pub enum MarketCommand {
    /// Add balance to an account.
    AddBalance {
        /// Amount to add to the account.
        amount: u128,
    },

    /// Publish storage deals.
    PublishStorageDeals {
        /// Storage deals to publish. Either JSON or a file path, prepended with an @.
        #[arg(value_parser = DealProposals::parse)]
        deals: DealProposals,
    },

    /// Settle deal payments.
    SettleDealPayments {
        /// The IDs for the deals to settle.
        deal_ids: Vec<DealId>,
    },

    /// Withdraw balance from an account.
    WithdrawBalance {
        /// Amount to withdraw from the account.
        amount: u128,
    },
}

impl MarketCommand {
    /// Run a `market` command.
    ///
    /// Requires the target RPC address and a keypair able to sign transactions.
    pub async fn run<Keypair>(
        self,
        node_rpc: Url,
        account_keypair: Keypair,
    ) -> Result<(), anyhow::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let client = MarketClient::new(node_rpc).await?;
        match self {
            MarketCommand::AddBalance { amount } => {
                let block_hash = client.add_balance(&account_keypair, amount).await?;
                tracing::info!(
                    "[{}] Successfully added {} to Market Balance",
                    block_hash,
                    amount
                );
            }
            MarketCommand::PublishStorageDeals { deals } => {
                let block_hash = client
                    .publish_storage_deals(
                        &account_keypair,
                        deals.0.into_iter().map(Into::into).collect(),
                    )
                    .await?;
                tracing::info!("[{}] Successfully published storage deals", block_hash);
            }
            MarketCommand::SettleDealPayments { deal_ids } => {
                let block_hash = client
                    .settle_deal_payments(&account_keypair, deal_ids)
                    .await?;
                tracing::info!("[{}] Successfully settled deal payments", block_hash);
            }
            MarketCommand::WithdrawBalance { amount } => {
                let block_hash = client.withdraw_balance(&account_keypair, amount).await?;
                tracing::info!(
                    "[{}] Successfully withdrew {} from Market Balance",
                    block_hash,
                    amount
                );
            }
        }
        Ok(())
    }
}
