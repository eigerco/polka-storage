use anyhow::bail;
use clap::{ArgGroup, Subcommand};
use primitives_proofs::DealId;
use storagext::{clients::MarketClient, runtime, PolkaStorageConfig};
use subxt::ext::sp_core::{
    ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair, sr25519::Pair as Sr25519Pair,
};
use url::Url;

use crate::{
    deser::ParseablePath, missing_keypair_error, pair::DebugPair, DealProposal, MultiPairSigner,
};

/// List of [`DealProposal`]s to publish.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct DealProposals(Vec<DealProposal>);

#[derive(Debug, Subcommand)]
#[command(name = "market", about = "CLI Client to the Market Pallet", version)]
pub(crate) enum MarketCommand {
    /// Add balance to an account.
    AddBalance {
        /// Amount to add to the account.
        amount: storagext::Currency,
    },

    /// Publish storage deals and sign by client_<key_type>_key
    #[command(group(ArgGroup::new("client_keypair").required(true).args(&["client_sr25519_key", "client_ecdsa_key", "client_ed25519_key"])))]
    PublishStorageDeals {
        /// Storage deals to publish. Either JSON or a file path, prepended with an @.
        #[arg(value_parser = <DealProposals as ParseablePath>::parse_json)]
        deals: DealProposals,
        /// Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
        ///
        /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
        #[arg(long, value_parser = DebugPair::<Sr25519Pair>::value_parser)]
        client_sr25519_key: Option<DebugPair<Sr25519Pair>>,

        /// ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
        ///
        /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
        #[arg(long, value_parser = DebugPair::<ECDSAPair>::value_parser)]
        client_ecdsa_key: Option<DebugPair<ECDSAPair>>,

        /// Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
        ///
        /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
        #[arg(long, value_parser = DebugPair::<Ed25519Pair>::value_parser)]
        client_ed25519_key: Option<DebugPair<Ed25519Pair>>,
    },

    /// Settle deal payments.
    SettleDealPayments {
        /// The IDs for the deals to settle.
        deal_ids: Vec<DealId>,
    },

    /// Withdraw balance from an account.
    WithdrawBalance {
        /// Amount to withdraw from the account.
        amount: storagext::Currency,
    },

    /// Retrieve the balance for a given account.
    RetrieveBalance {
        /// The target account's ID.
        account_id: <PolkaStorageConfig as subxt::Config>::AccountId,
    },
}

impl MarketCommand {
    /// Run a `market` command.
    ///
    /// Requires the target RPC address and a keypair able to sign transactions.
    #[tracing::instrument(level = "info", skip(self, node_rpc), fields(node_rpc = node_rpc.as_str()))]
    pub async fn run(
        self,
        node_rpc: Url,
        account_keypair: Option<MultiPairSigner>,
    ) -> Result<(), anyhow::Error> {
        let client = MarketClient::new(node_rpc).await?;

        match self {
            // Only command that doesn't need a key.
            //
            // NOTE: subcommand_negates_reqs does not work for this since it only negates the parents'
            // requirements, and the global arguments (keys) are at the grandparent level
            // https://users.rust-lang.org/t/clap-ignore-global-argument-in-sub-command/101701/8
            MarketCommand::RetrieveBalance { account_id } => {
                if let Some(balance) = client.retrieve_balance(account_id.clone()).await? {
                    tracing::info!(
                        "Account {} {{ free: {}, locked: {} }}",
                        account_id,
                        balance.free,
                        balance.locked
                    );
                } else {
                    tracing::error!("Could not find account {}", account_id);
                }
            }
            else_ => {
                let Some(account_keypair) = account_keypair else {
                    return Err(missing_keypair_error::<Self>().into());
                };
                else_.with_keypair(client, account_keypair).await?;
            }
        };

        Ok(())
    }

    async fn with_keypair(
        self,
        client: MarketClient,
        account_keypair: MultiPairSigner,
    ) -> Result<(), anyhow::Error> {
        match self {
            MarketCommand::AddBalance { amount } => {
                let submission_result = client.add_balance(&account_keypair, amount).await?;
                tracing::info!(
                    "[{}] Successfully added {} to Market Balance",
                    submission_result.hash,
                    amount
                );

                for event in submission_result
                    .events
                    .find::<runtime::market::events::BalanceAdded>()
                {
                    let event = event?;
                    println!("[{}] Balance added: {}", submission_result.hash, event);
                }
            }
            MarketCommand::PublishStorageDeals {
                deals,
                client_sr25519_key,
                client_ecdsa_key,
                client_ed25519_key,
            } => {
                let client_keypair =
                    MultiPairSigner::new(
                        client_sr25519_key.map(DebugPair::into_inner),
                        client_ecdsa_key.map(DebugPair::into_inner),
                        client_ed25519_key.map(DebugPair::into_inner)
                    )
                    .expect("client is required to submit at least one key, this should've been handled by clap's ArgGroup");
                let submission_result = client
                    .publish_storage_deals(
                        account_keypair,
                        &client_keypair,
                        deals.0.into_iter().map(Into::into).collect(),
                    )
                    .await?;
                tracing::info!(
                    "[{}] Successfully published storage deals",
                    submission_result.hash
                );

                for event in submission_result
                    .events
                    .find::<runtime::market::events::DealPublished>()
                {
                    let event = event?;
                    println!("[{}] Deal published: {}", submission_result.hash, event);
                }
            }
            MarketCommand::SettleDealPayments { deal_ids } => {
                if deal_ids.is_empty() {
                    bail!("No deals provided to settle");
                }

                let submission_result = client
                    .settle_deal_payments(&account_keypair, deal_ids)
                    .await?;
                tracing::info!(
                    "[{}] Successfully settled deal payments",
                    submission_result.hash
                );

                for event in submission_result
                    .events
                    .find::<runtime::market::events::DealsSettled>()
                {
                    let event = event?;
                    println!("[{}] Deals settled: {}", submission_result.hash, event);
                }
            }
            MarketCommand::WithdrawBalance { amount } => {
                let submission_result = client.withdraw_balance(&account_keypair, amount).await?;
                tracing::info!(
                    "[{}] Successfully withdrew {} from Market Balance",
                    submission_result.hash,
                    amount
                );

                for event in submission_result
                    .events
                    .find::<runtime::market::events::BalanceWithdrawn>()
                {
                    let event = event?;
                    println!("[{}] Balance withdrawn: {}", submission_result.hash, event);
                }
            }
            _unsigned => unreachable!("unsigned extrinsics should have been previously handled"),
        }
        Ok(())
    }
}
