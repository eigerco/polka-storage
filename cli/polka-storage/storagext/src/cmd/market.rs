use std::{error::Error, str::FromStr};

use clap::Subcommand;
use frame_support::sp_runtime::MultiSigner;
use storagext::market::MarketClient;
use subxt::{tx::Signer, utils::AccountId32, OnlineClient, SubstrateConfig};
use subxt_signer::{
    sr25519::{dev, Keypair},
    SecretUri,
};
use url::Url;

#[derive(Debug, Subcommand)]
#[command(name = "market", about = "CLI Client to the Market Pallet", version)]
pub enum MarketCommand {
    /// Add balance to an account.
    AddBalance { amount: u128 },

    /// Publish storage deals.
    PublishStorageDeals,

    /// Settle deal payments.
    SettleDealPayments,

    /// Withdraw balance from an account.
    WithdrawBalance { amount: u128 },
}

impl MarketCommand {
    pub async fn run<Keypair>(
        self,
        node_rpc: Url,
        account_keypair: Keypair,
    ) -> Result<(), Box<dyn Error>>
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
            MarketCommand::PublishStorageDeals => todo!(),
            MarketCommand::SettleDealPayments => todo!(),
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
