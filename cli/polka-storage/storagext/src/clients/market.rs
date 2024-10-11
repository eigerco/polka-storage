use std::future::Future;

use primitives_proofs::DealId;
use subxt::{ext::sp_core::crypto::Ss58Codec, utils::Static};

use crate::{
    runtime::{
        self,
        client::{HashOfPsc, SubmissionResult},
        market::events as MarketEvents,
        runtime_types::pallet_market::pallet::{
            BalanceEntry, ClientDealProposal as RuntimeClientDealProposal,
        },
    },
    types::market::{ClientDealProposal, DealProposal},
    BlockNumber, Currency, PolkaStorageConfig,
};

/// Specialized version of [`RuntimeClientDealProposal`] for convenience's sake.
type SpecializedRuntimeClientDealProposal = RuntimeClientDealProposal<
    subxt::ext::subxt_core::utils::AccountId32,
    Currency,
    BlockNumber,
    Static<subxt::ext::sp_runtime::MultiSignature>,
>;

/// The maximum number of deal IDs supported.
// NOTE(@jmg-duarte,17/07/2024): ideally, should be read from the primitives or something
const MAX_N_DEALS: usize = 32;

/// Client to interact with the market pallet extrinsics.
pub trait MarketClientExt {
    /// Withdraw the given `amount` of balance.
    fn withdraw_balance<Keypair>(
        &self,
        account_keypair: &Keypair,
        amount: Currency,
        wait_for_finalization: bool,
    ) -> impl Future<
        Output = Result<
            Option<SubmissionResult<HashOfPsc, MarketEvents::BalanceWithdrawn>>,
            subxt::Error,
        >,
    >
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    /// Add the given `amount` of balance.
    fn add_balance<Keypair>(
        &self,
        account_keypair: &Keypair,
        amount: Currency,
        wait_for_finalization: bool,
    ) -> impl Future<
        Output = Result<
            Option<SubmissionResult<HashOfPsc, MarketEvents::BalanceAdded>>,
            subxt::Error,
        >,
    >
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    /// Settle deal payments for the provided [`DealId`]s.
    ///
    /// If `deal_ids` length is bigger than [`MAX_DEAL_IDS`], it will get truncated.
    fn settle_deal_payments<Keypair>(
        &self,
        account_keypair: &Keypair,
        deal_ids: Vec<DealId>,
        wait_for_finalization: bool,
    ) -> impl Future<
        Output = Result<
            Option<SubmissionResult<HashOfPsc, MarketEvents::DealsSettled>>,
            subxt::Error,
        >,
    >
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    /// Publish the given storage deals.
    ///
    /// If `deals` length is bigger than [`MAX_DEAL_IDS`], it will get truncated.
    fn publish_storage_deals<Keypair, ClientKeypair>(
        &self,
        account_keypair: &Keypair,
        client_keypair: &ClientKeypair,
        deals: Vec<DealProposal>,
        wait_for_finalization: bool,
    ) -> impl Future<
        Output = Result<
            Option<SubmissionResult<HashOfPsc, MarketEvents::DealPublished>>,
            subxt::Error,
        >,
    >
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
        ClientKeypair: subxt::tx::Signer<PolkaStorageConfig>;

    /// Publish the given *signed* storage deals.
    ///
    /// If `deals` length is bigger than [`MAX_DEAL_IDS`], it will get truncated.
    fn publish_signed_storage_deals<Keypair>(
        &self,
        account_keypair: &Keypair,
        deals: Vec<ClientDealProposal>,
        wait_for_finalization: bool,
    ) -> impl Future<
        Output = Result<
            Option<SubmissionResult<HashOfPsc, MarketEvents::DealPublished>>,
            subxt::Error,
        >,
    >
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    /// Retrieve the balance for a given account (includes the `free` and `locked` balance).
    fn retrieve_balance(
        &self,
        account_id: <PolkaStorageConfig as subxt::Config>::AccountId,
    ) -> impl Future<Output = Result<Option<BalanceEntry<u128>>, subxt::Error>>;
}

impl MarketClientExt for crate::runtime::client::Client {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
            amount = amount
        )
    )]
    async fn withdraw_balance<Keypair>(
        &self,
        account_keypair: &Keypair,
        amount: Currency,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<HashOfPsc, MarketEvents::BalanceWithdrawn>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx().market().withdraw_balance(amount);
        self.traced_submission(&payload, account_keypair, wait_for_finalization, 1)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
            amount = amount
        )
    )]
    async fn add_balance<Keypair>(
        &self,
        account_keypair: &Keypair,
        amount: Currency,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<HashOfPsc, MarketEvents::BalanceAdded>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx().market().add_balance(amount);
        self.traced_submission(&payload, account_keypair, wait_for_finalization, 1)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
            deal_ids = ?deal_ids
        )
    )]
    async fn settle_deal_payments<Keypair>(
        &self,
        account_keypair: &Keypair,
        mut deal_ids: Vec<DealId>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<HashOfPsc, MarketEvents::DealsSettled>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        if deal_ids.len() > MAX_N_DEALS {
            tracing::warn!("more than {} deal ids, truncating", MAX_N_DEALS);
            deal_ids.truncate(MAX_N_DEALS);
        }
        let n_deal_ids = deal_ids.len();
        // `deal_ids` has been truncated to fit the proper bound, however,
        // the `BoundedVec` defined in the `runtime::runtime_types` is actually just a newtype
        // making the `BoundedVec` actually unbounded
        let bounded_unbounded_deal_ids =
            runtime::runtime_types::bounded_collections::bounded_vec::BoundedVec(deal_ids);

        let payload = runtime::tx()
            .market()
            .settle_deal_payments(bounded_unbounded_deal_ids);

        self.traced_submission(&payload, account_keypair, wait_for_finalization, n_deal_ids)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check()
        )
    )]
    async fn publish_storage_deals<Keypair, ClientKeypair>(
        &self,
        account_keypair: &Keypair,
        client_keypair: &ClientKeypair,
        mut deals: Vec<DealProposal>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<HashOfPsc, MarketEvents::DealPublished>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
        ClientKeypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        if deals.len() > MAX_N_DEALS {
            tracing::warn!("more than {} deals, truncating", MAX_N_DEALS);
            deals.truncate(MAX_N_DEALS);
        }

        let n_deals = deals.len();
        let signed_deal_proposals = deals
            .into_iter()
            .map(|deal| deal.sign(client_keypair))
            .collect();

        // `deals` has been truncated to fit the proper bound, however,
        // the `BoundedVec` defined in the `runtime::runtime_types` is actually just a newtype
        // making the `BoundedVec` actually unbounded
        let bounded_unbounded_deals =
            runtime::runtime_types::bounded_collections::bounded_vec::BoundedVec(
                signed_deal_proposals,
            );

        let payload = runtime::tx()
            .market()
            .publish_storage_deals(bounded_unbounded_deals);

        self.traced_submission(&payload, account_keypair, wait_for_finalization, n_deals)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check()
        )
    )]
    async fn publish_signed_storage_deals<Keypair>(
        &self,
        account_keypair: &Keypair,
        mut deals: Vec<ClientDealProposal>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<HashOfPsc, MarketEvents::DealPublished>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        if deals.len() > MAX_N_DEALS {
            tracing::warn!("more than {} deals, truncating", MAX_N_DEALS);
            deals.truncate(MAX_N_DEALS);
        }

        let n_deals = deals.len();
        let deals = deals
            .into_iter()
            .map(|deal| SpecializedRuntimeClientDealProposal::from(deal))
            .collect();

        // `deals` has been truncated to fit the proper bound, however,
        // the `BoundedVec` defined in the `runtime::runtime_types` is actually just a newtype
        // making the `BoundedVec` actually unbounded
        let bounded_unbounded_deals =
            runtime::runtime_types::bounded_collections::bounded_vec::BoundedVec(deals);

        let payload = runtime::tx()
            .market()
            .publish_storage_deals(bounded_unbounded_deals);

        self.traced_submission(&payload, account_keypair, wait_for_finalization, n_deals)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_id.to_ss58check()
        )
    )]
    async fn retrieve_balance(
        &self,
        account_id: <PolkaStorageConfig as subxt::Config>::AccountId,
    ) -> Result<Option<BalanceEntry<u128>>, subxt::Error> {
        let balance_table_query = runtime::storage()
            .market()
            .balance_table(subxt::utils::AccountId32::from(account_id));
        self.client
            .storage()
            .at_latest()
            .await?
            .fetch(&balance_table_query)
            .await
    }
}
