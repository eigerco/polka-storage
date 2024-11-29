use std::time::Duration;

use anyhow::bail;
use clap::{ArgGroup, Subcommand};
use primitives::DealId;
use storagext::{
    deser::DeserializablePath,
    multipair::{DebugPair, MultiPairSigner},
    runtime::SubmissionResult,
    types::market::DealProposal as SxtDealProposal,
    MarketClientExt, PolkaStorageConfig,
};
use subxt::ext::sp_core::{
    ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair, sr25519::Pair as Sr25519Pair,
};
use url::Url;

use crate::{missing_keypair_error, operation_takes_a_while, OutputFormat};

/// Removes the `_` from a the input before calling [`parse`](std::std::FromStr::parse).
fn parse_without_underscore<T>(s: &str) -> Result<T, T::Err>
where
    T: std::str::FromStr,
{
    s.replace('_', "").parse()
}

#[derive(Debug, Subcommand)]
#[command(name = "market", about = "CLI Client to the Market Pallet", version)]
pub(crate) enum MarketCommand {
    /// Add balance to an account.
    AddBalance {
        /// Amount to add to the account.
        #[arg(value_parser=parse_without_underscore::<storagext::Currency>)]
        amount: storagext::Currency,
    },

    /// Withdraw balance from an account.
    WithdrawBalance {
        /// Amount to withdraw from the account.
        #[arg(value_parser=parse_without_underscore::<storagext::Currency>)]
        amount: storagext::Currency,
    },

    /// Publish storage deals and sign by client_<key_type>_key
    #[command(group(ArgGroup::new("client_keypair").required(true).args(&["client_sr25519_key", "client_ecdsa_key", "client_ed25519_key"])))]
    PublishStorageDeals {
        /// Storage deals to publish. Either JSON or a file path, prepended with an @.
        #[arg(value_parser = <Vec<SxtDealProposal> as DeserializablePath>::deserialize_json)]
        deals: std::vec::Vec<SxtDealProposal>,
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

    /// Retrieve the balance for a given account.
    RetrieveBalance {
        /// The target account's ID.
        account_id: <PolkaStorageConfig as subxt::Config>::AccountId,
    },

    /// Retrieve the deal for a given deal ID.
    RetrieveDeal {
        /// The target deal's ID.
        deal_id: DealId,
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
        n_retries: u32,
        retry_interval: Duration,
        output_format: OutputFormat,
        wait_for_finalization: bool,
    ) -> Result<(), anyhow::Error> {
        let client = storagext::Client::new(node_rpc, n_retries, retry_interval).await?;

        match self {
            // NOTE: subcommand_negates_reqs does not work for this since it only negates the parents'
            // requirements, and the global arguments (keys) are at the grandparent level
            // https://users.rust-lang.org/t/clap-ignore-global-argument-in-sub-command/101701/8
            MarketCommand::RetrieveBalance { account_id } => {
                if let Some(balance) = client.retrieve_balance(account_id.clone()).await? {
                    tracing::debug!(
                        "Account {} {{ free: {}, locked: {} }}",
                        account_id,
                        balance.free,
                        balance.locked
                    );

                    println!("{}", output_format.format(&balance)?);
                } else {
                    tracing::error!("Could not find account {}", account_id);
                }
            }
            MarketCommand::RetrieveDeal { deal_id } => {
                if let Some(deal) = client.retrieve_deal(deal_id).await? {
                    tracing::debug!("Deal {:?}", deal);

                    println!("{}", output_format.format(&deal)?);
                } else {
                    tracing::error!("Could not find deal {}", deal_id);
                }
            }
            else_ => {
                let Some(account_keypair) = account_keypair else {
                    return Err(missing_keypair_error::<Self>().into());
                };
                else_
                    .with_keypair(
                        client,
                        account_keypair,
                        output_format,
                        wait_for_finalization,
                    )
                    .await?;
            }
        };

        Ok(())
    }

    async fn with_keypair<Client>(
        self,
        client: Client,
        account_keypair: MultiPairSigner,
        output_format: OutputFormat,
        wait_for_finalization: bool,
    ) -> Result<(), anyhow::Error>
    where
        Client: MarketClientExt,
    {
        operation_takes_a_while(wait_for_finalization);

        let submission_result = match self {
            MarketCommand::AddBalance { amount } => {
                Self::add_balance(client, account_keypair, amount, wait_for_finalization).await?
            }
            MarketCommand::SettleDealPayments { deal_ids } => {
                if deal_ids.is_empty() {
                    bail!("No deals provided to settle");
                }

                Self::settle_deal_payments(client, account_keypair, deal_ids, wait_for_finalization)
                    .await?
            }
            MarketCommand::WithdrawBalance { amount } => {
                Self::withdraw_balance(client, account_keypair, amount, wait_for_finalization)
                    .await?
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
                Self::publish_storage_deals(
                    client,
                    account_keypair,
                    client_keypair,
                    deals,
                    wait_for_finalization,
                )
                .await?
            }
            _unsigned => unreachable!("unsigned commands should have been previously handled"),
        };
        let Some(submission_result) = submission_result else {
            // Didn't wait for finalization
            return Ok(());
        };

        // This monstrosity first converts incoming events into a "generic" (subxt generated) event,
        // and then we extract only the Market events. We could probably extract this into a proper
        // iterator but the effort to improvement ratio seems low (for 2 pallets at least).
        let submission_results = submission_result
            .events
            .iter()
            .flat_map(|event| {
                event.map(|details| details.as_root_event::<storagext::runtime::Event>())
            })
            .filter_map(|event| match event {
                Ok(storagext::runtime::Event::Market(e)) => Some(Ok(e)),
                Err(err) => Some(Err(err)),
                _ => None,
            });
        for event in submission_results {
            let event = event?;
            let output = output_format.format(&event)?;
            match output_format {
                OutputFormat::Plain => println!("[{}] {}", submission_result.hash, output),
                OutputFormat::Json => println!("{}", output),
            }
        }
        Ok(())
    }

    async fn add_balance<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        amount: u128,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: MarketClientExt,
    {
        let submission_result = client
            .add_balance(&account_keypair, amount, wait_for_finalization)
            .await?
            .inspect(|result| {
                tracing::debug!(
                    "[{}] Successfully added {} to Market Balance",
                    result.hash,
                    amount
                );
            });

        Ok(submission_result)
    }

    async fn publish_storage_deals<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        client_keypair: MultiPairSigner,
        deals: Vec<SxtDealProposal>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: MarketClientExt,
    {
        let submission_result = client
            .publish_storage_deals(
                &account_keypair,
                &client_keypair,
                deals.into_iter().map(Into::into).collect(),
                wait_for_finalization,
            )
            .await?
            .inspect(|result| {
                tracing::debug!("[{}] Successfully published storage deals", result.hash)
            });

        Ok(submission_result)
    }

    async fn settle_deal_payments<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        deal_ids: Vec<u64>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: MarketClientExt,
    {
        let submission_result = client
            .settle_deal_payments(&account_keypair, deal_ids, wait_for_finalization)
            .await?
            .inspect(|result| {
                tracing::debug!("[{}] Successfully settled deal payments", result.hash);
            });

        Ok(submission_result)
    }

    async fn withdraw_balance<Client>(
        client: Client,
        account_keypair: MultiPairSigner,
        amount: u128,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Client: MarketClientExt,
    {
        let submission_result = client
            .withdraw_balance(&account_keypair, amount, wait_for_finalization)
            .await?
            .inspect(|result| {
                tracing::debug!(
                    "[{}] Successfully withdrew {} from Market Balance",
                    result.hash,
                    amount
                )
            });

        Ok(submission_result)
    }
}

#[cfg(test)]
mod test {
    use super::parse_without_underscore;

    #[test]
    fn test_parse() {
        let parsed = parse_without_underscore::<u128>("1_000_0").unwrap();
        assert_eq!(parsed, 1_000_0);
    }
}
