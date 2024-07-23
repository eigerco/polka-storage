use hex::ToHex;
use primitives_proofs::DealId;
use subxt::{ext::sp_core::crypto::Ss58Codec, OnlineClient};

use crate::{
    runtime::{self},
    Currency, DealProposal, PolkaStorageConfig,
};

/// The maximum number of deal IDs supported.
// NOTE(@jmg-duarte,17/07/2024): ideally, should be read from the primitives or something
const MAX_N_DEALS: usize = 32;

/// Client to interact with the market pallet extrinsics.
pub struct MarketClient {
    client: OnlineClient<PolkaStorageConfig>,
}

impl MarketClient {
    /// Create a new [`MarketClient`] from a target `rpc_address`.
    ///
    /// By default, this function does not support insecure URLs,
    /// to enable support for them, use the `insecure_url` feature.
    pub async fn new(rpc_address: impl AsRef<str>) -> Result<Self, subxt::Error> {
        let client = if cfg!(feature = "insecure_url") {
            OnlineClient::<_>::from_insecure_url(rpc_address).await?
        } else {
            OnlineClient::<_>::from_url(rpc_address).await?
        };

        Ok(Self { client })
    }

    /// Withdraw the given `amount` of balance.
    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
            amount = amount
        )
    )]
    pub async fn withdraw_balance<Keypair>(
        &self,
        account_keypair: &Keypair,
        amount: Currency,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx().market().withdraw_balance(amount);
        self.traced_submission(&payload, account_keypair).await
    }

    /// Add the given `amount` of balance.
    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
            amount = amount
        )
    )]
    pub async fn add_balance<Keypair>(
        &self,
        account_keypair: &Keypair,
        amount: Currency,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx().market().add_balance(amount);
        self.traced_submission(&payload, account_keypair).await
    }

    /// Settle deal payments for the provided [`DealId`]s.
    ///
    /// If `deal_ids` length is bigger than [`MAX_DEAL_IDS`], it will get truncated.
    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
            deal_ids = ?deal_ids
        )
    )]
    pub async fn settle_deal_payments<Keypair>(
        &self,
        account_keypair: &Keypair,
        mut deal_ids: Vec<DealId>,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        if deal_ids.len() > MAX_N_DEALS {
            tracing::warn!("more than {} deal ids, truncating", MAX_N_DEALS);
            deal_ids.truncate(MAX_N_DEALS);
        }
        // `deal_ids` has been truncated to fit the proper bound, however,
        // the `BoundedVec` defined in the `runtime::runtime_types` is actually just a newtype
        // making the `BoundedVec` actually unbounded
        let bounded_unbounded_deal_ids =
            runtime::runtime_types::bounded_collections::bounded_vec::BoundedVec(deal_ids);

        let payload = runtime::tx()
            .market()
            .settle_deal_payments(bounded_unbounded_deal_ids);

        self.traced_submission(&payload, account_keypair).await
    }

    /// Publish the given storage deals.
    ///
    /// If `deals` length is bigger than [`MAX_DEAL_IDS`], it will get truncated.
    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check()
        )
    )]
    pub async fn publish_storage_deals<Keypair>(
        &self,
        account_keypair: &Keypair,
        mut deals: Vec<DealProposal>,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        if deals.len() > MAX_N_DEALS {
            tracing::warn!("more than {} deals, truncating", MAX_N_DEALS);
            deals.truncate(MAX_N_DEALS);
        }

        let signed_deal_proposals = deals
            .into_iter()
            .map(|deal| deal.sign(account_keypair))
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

        self.traced_submission(&payload, account_keypair).await
    }

    /// Submit an extrinsic and wait for finalization, returning the block hash it was included in.
    ///
    /// Equivalent to performing [`OnlineClient::sign_and_submit_then_watch_default`],
    /// followed by [`TxInBlock::wait_for_finalized`].
    async fn traced_submission<Call, Keypair>(
        &self,
        call: &Call,
        account_keypair: &Keypair,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Call: subxt::tx::Payload,
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        tracing::trace!("submitting extrinsic");
        let submission_progress = self
            .client
            .tx()
            .sign_and_submit_then_watch_default(call, account_keypair)
            .await?;

        tracing::trace!(
            extrinsic_hash = submission_progress.extrinsic_hash().encode_hex::<String>(),
            "waiting for finalization"
        );
        let finalized_xt = submission_progress.wait_for_finalized().await?;

        // Wait for a successful inclusion because finalization != success
        finalized_xt.wait_for_success().await?;

        let block_hash = finalized_xt.block_hash();
        tracing::trace!(
            block_hash = block_hash.encode_hex::<String>(),
            "successfully submitted extrinsic"
        );
        Ok(block_hash)
    }
}
