use std::{error::Error, path::PathBuf, str::FromStr};

use clap::Subcommand;
use frame_support::sp_runtime::{MultiSigner, AccountId32};
use primitives_proofs::DealId;
use storagext::{market::MarketClient, DealProposal};
use subxt::{tx::Signer, , OnlineClient, SubstrateConfig};
use subxt_signer::{
    sr25519::{dev, Keypair},
    SecretUri,
};
use url::Url;

#[derive(Debug, Clone, serde::Deserialize)]
struct DealProposals(Vec<DealProposal>);

impl DealProposals {
    fn parse(src: &str) -> Result<Self, anyhow::Error> {
        Ok(if src.starts_with('@') {
            let path = PathBuf::from_str(&src[1..])?;
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
    AddBalance { amount: u128 },

    /// Publish storage deals.
    PublishStorageDeals {
        #[arg(value_parser = DealProposals::parse)]
        deals: DealProposals,
    },

    /// Settle deal payments.
    SettleDealPayments { deal_ids: Vec<DealId> },

    /// Withdraw balance from an account.
    WithdrawBalance { amount: u128 },
}

impl MarketCommand {
    pub async fn run<Keypair>(
        self,
        node_rpc: Url,
        account_keypair: Keypair,
    ) -> Result<(), anyhow::Error>
    where
        Keypair: subxt::tx::Signer<SubstrateConfig>,
    {
        let client = MarketClient::<SubstrateConfig>::new(node_rpc).await?;
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
                    .publish_storage_deals(&account_keypair, deals.0)
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
