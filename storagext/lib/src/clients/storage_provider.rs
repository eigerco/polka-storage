use std::future::Future;

use libp2p::PeerId as P2PPeerId;
use primitives::{proofs::RegisteredPoStProof, DealId};
use runtime::runtime_types::bounded_collections::bounded_vec::BoundedVec;
use sp_core::crypto::Ss58Codec;
use subxt::{
    ext::futures::TryStreamExt,
    utils::{AccountId32, Static},
};

use crate::{
    runtime::{
        self,
        bounded_vec::IntoBoundedByteVec,
        client::SubmissionResult,
        runtime_types::{
            pallet_storage_provider::{
                deal::parameters::DealParameters as RuntimeDealParameters,
                storage_provider::StorageProviderState,
            },
            primitives::{
                deals::client_deal_proposal::ClientDealProposal as RuntimeClientDealProposal,
                pallets::DeadlineInfo,
            },
        },
        storage_provider::calls::types::register_storage_provider::PeerId,
    },
    types::storage_provider::{
        ClientDealProposal, DeadlineState, DealProposal, FaultDeclaration, OffchainDealParameters,
        ProveCommitSector, RecoveryDeclaration, SectorPreCommitInfo, SubmitWindowedPoStParams,
        TerminationDeclaration,
    },
    BlockNumber, Currency, PolkaStorageConfig,
};

/// Specialized version of [`RuntimeClientDealProposal`] for convenience's sake.
type SpecializedRuntimeClientDealProposal = RuntimeClientDealProposal<
    subxt::ext::subxt_core::utils::AccountId32,
    Currency,
    BlockNumber,
    Static<sp_runtime::MultiSignature>,
>;

type SpecializedRuntimeDealParameters = RuntimeDealParameters<Currency, BlockNumber>;

/// The maximum number of deal IDs supported.
// NOTE(@jmg-duarte,17/07/2024): ideally, should be read from the primitives or something
const MAX_N_DEALS: usize = 32;

pub trait StorageProviderClientExt {
    /// Settle deal payments for the provided [`DealId`]s.
    ///
    /// If `deal_ids` length is bigger than [`MAX_DEAL_IDS`], it will get truncated.
    fn settle_deal_payments<Keypair>(
        &self,
        account_keypair: &Keypair,
        deal_ids: Vec<DealId>,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
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
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
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
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    /// Publish the given deal parameters
    fn publish_deal_parameters<Keypair>(
        &self,
        account_keypair: &Keypair,
        deal_parameters: OffchainDealParameters,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    /// Remove set deal parameters
    fn remove_deal_parameters<Keypair>(
        &self,
        account_keypair: &Keypair,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    /// Retrieves the deal parameter for the given storage provider account
    fn retrieve_sp_deal_parameters_for(
        &self,
        account_id: <PolkaStorageConfig as subxt::Config>::AccountId,
    ) -> impl Future<Output = Result<Option<SpecializedRuntimeDealParameters>, subxt::Error>>;

    /// Retrieves all deal parameters stored in the market pallet.
    fn retrieve_sp_deal_parameters(
        &self,
    ) -> impl Future<
        Output = Result<
            Vec<(
                <crate::PolkaStorageConfig as subxt::Config>::AccountId,
                SpecializedRuntimeDealParameters,
            )>,
            subxt::Error,
        >,
    >;

    /// Retrieve the deal for a given deal ID.
    fn retrieve_deal(
        &self,
        deal_id: DealId,
    ) -> impl Future<Output = Result<Option<DealProposal>, subxt::Error>>;

    fn register_storage_provider<Keypair>(
        &self,
        account_keypair: &Keypair,
        peer_id: P2PPeerId,
        post_proof: RegisteredPoStProof,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn pre_commit_sectors<Keypair>(
        &self,
        account_keypair: &Keypair,
        sectors: Vec<SectorPreCommitInfo>,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn prove_commit_sectors<Keypair>(
        &self,
        account_keypair: &Keypair,
        sectors: Vec<ProveCommitSector>,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn submit_windowed_post<Keypair>(
        &self,
        account_keypair: &Keypair,
        windowed_post: SubmitWindowedPoStParams,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn declare_faults<Keypair>(
        &self,
        account_keypair: &Keypair,
        faults: Vec<FaultDeclaration>,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn declare_faults_recovered<Keypair>(
        &self,
        account_keypair: &Keypair,
        recoveries: Vec<RecoveryDeclaration>,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn terminate_sectors<Keypair>(
        &self,
        account_keypair: &Keypair,
        terminations: Vec<TerminationDeclaration>,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn retrieve_storage_provider(
        &self,
        account_id: &AccountId32,
    ) -> impl Future<
        Output = Result<Option<StorageProviderState<PeerId, Currency, BlockNumber>>, subxt::Error>,
    >;

    fn retrieve_registered_storage_providers(
        &self,
    ) -> impl Future<Output = Result<Vec<String>, subxt::Error>>;

    fn deadline_info(
        &self,
        account_id: &AccountId32,
        deadline_index: u64,
    ) -> impl Future<Output = Result<Option<DeadlineInfo<BlockNumber>>, subxt::Error>>;

    fn deadline_state(
        &self,
        account_id: &AccountId32,
        deadline_index: u64,
    ) -> impl Future<Output = Result<Option<DeadlineState>, subxt::Error>>;

    fn proving_period_info(&self) -> Result<ProvingPeriodInfo, subxt::Error>;

    fn sector_expiration_bounds(&self) -> Result<(BlockNumber, BlockNumber), subxt::Error>;
}

pub struct ProvingPeriodInfo {
    /// Number of deadlines in a proving period,
    pub deadlines: u64,
}

impl StorageProviderClientExt for crate::runtime::client::Client {
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
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
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
            .storage_provider()
            .settle_deal_payments(bounded_unbounded_deal_ids);

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
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
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
        ClientKeypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        if deals.len() > MAX_N_DEALS {
            tracing::warn!("more than {} deals, truncating", MAX_N_DEALS);
            deals.truncate(MAX_N_DEALS);
        }

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
            .storage_provider()
            .publish_storage_deals(bounded_unbounded_deals);

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
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
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        if deals.len() > MAX_N_DEALS {
            tracing::warn!("more than {} deals, truncating", MAX_N_DEALS);
            deals.truncate(MAX_N_DEALS);
        }

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
            .storage_provider()
            .publish_storage_deals(bounded_unbounded_deals);

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check()
        )
    )]
    async fn publish_deal_parameters<Keypair>(
        &self,
        account_keypair: &Keypair,
        deal_parameters: OffchainDealParameters,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .publish_deal_parameters(deal_parameters.into());

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check()
        )
    )]
    async fn remove_deal_parameters<Keypair>(
        &self,
        account_keypair: &Keypair,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx().storage_provider().remove_deal_parameters();

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_id.to_ss58check()
        )
    )]
    async fn retrieve_sp_deal_parameters_for(
        &self,
        account_id: <PolkaStorageConfig as subxt::Config>::AccountId,
    ) -> Result<Option<SpecializedRuntimeDealParameters>, subxt::Error> {
        let deal_parameter_query = runtime::storage()
            .storage_provider()
            .sp_deal_parameters(subxt::utils::AccountId32(account_id.into()));

        self.client
            .storage()
            .at_latest()
            .await?
            .fetch(&deal_parameter_query)
            .await
    }

    async fn retrieve_sp_deal_parameters(
        &self,
    ) -> Result<
        Vec<(
            <crate::PolkaStorageConfig as subxt::Config>::AccountId,
            SpecializedRuntimeDealParameters,
        )>,
        subxt::Error,
    > {
        let deal_parameters_query = runtime::storage()
            .storage_provider()
            .sp_deal_parameters_iter();

        let mut deal_params = self
            .client
            .storage()
            .at_latest()
            .await?
            .iter(deal_parameters_query)
            .await?;

        let mut params = vec![];

        while let Some(Ok(kv)) = deal_params.next().await {
            // The bytes for the AccountId are at the end of the key bytes.
            // The format of the key bytes is concat(hash(scale_encoded_key), scale_encoded_key)
            // https://github.com/paritytech/subxt/issues/1201
            let mut account_buffer = [0; 32];
            account_buffer.copy_from_slice(&kv.key_bytes[(kv.key_bytes.len() - 32)..]);
            let account =
                <crate::PolkaStorageConfig as subxt::Config>::AccountId::from(account_buffer);
            params.push((account, kv.value))
        }

        Ok(params)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(deal_id))]
    async fn retrieve_deal(&self, deal_id: DealId) -> Result<Option<DealProposal>, subxt::Error> {
        let deal_table_query = runtime::storage().storage_provider().proposals(deal_id);
        let Some(deal) = self
            .client
            .storage()
            .at_latest()
            .await?
            .fetch(&deal_table_query)
            .await?
        else {
            return Ok(None);
        };

        let deal = DealProposal::try_from(deal).map_err(|e| {
            subxt::Error::Other(format!("failed to convert deal proposal: {:?}", e))
        })?;

        Ok(Some(deal))
    }

    #[tracing::instrument(level = "debug", skip_all, fields(deadline_index))]
    async fn deadline_state(
        &self,
        account_id: &AccountId32,
        deadline_index: u64,
    ) -> Result<Option<DeadlineState>, subxt::Error> {
        let payload = runtime::apis()
            .storage_provider_api()
            .deadline_state(account_id.clone(), deadline_index);

        self.client
            .runtime_api()
            .at_latest()
            .await?
            .call(payload)
            .await
            .and_then(|state| Ok(state.map(Into::into)))
    }

    async fn deadline_info(
        &self,
        account_id: &AccountId32,
        deadline_index: u64,
    ) -> Result<Option<DeadlineInfo<BlockNumber>>, subxt::Error> {
        let payload = runtime::apis()
            .storage_provider_api()
            .deadline_info(account_id.clone(), deadline_index);

        self.client
            .runtime_api()
            .at_latest()
            .await?
            .call(payload)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn register_storage_provider<Keypair>(
        &self,
        account_keypair: &Keypair,
        peer_id: P2PPeerId,
        post_proof: RegisteredPoStProof,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .register_storage_provider(peer_id.to_bytes().into_bounded_byte_vec(), post_proof);

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn pre_commit_sectors<Keypair>(
        &self,
        account_keypair: &Keypair,
        sectors: Vec<SectorPreCommitInfo>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let sectors = BoundedVec(sectors.into_iter().map(Into::into).collect());
        let payload = runtime::tx().storage_provider().pre_commit_sectors(sectors);

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn prove_commit_sectors<Keypair>(
        &self,
        account_keypair: &Keypair,
        sectors: Vec<ProveCommitSector>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let sectors = BoundedVec(sectors.into_iter().map(Into::into).collect());
        let payload = runtime::tx()
            .storage_provider()
            .prove_commit_sectors(sectors);

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn submit_windowed_post<Keypair>(
        &self,
        account_keypair: &Keypair,
        windowed_post: SubmitWindowedPoStParams,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .submit_windowed_post(windowed_post.into());

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn declare_faults<Keypair>(
        &self,
        account_keypair: &Keypair,
        faults: Vec<FaultDeclaration>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .declare_faults(faults.into());

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }

    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn declare_faults_recovered<Keypair>(
        &self,
        account_keypair: &Keypair,
        recoveries: Vec<RecoveryDeclaration>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .declare_faults_recovered(recoveries.into());

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn terminate_sectors<Keypair>(
        &self,
        account_keypair: &Keypair,
        terminations: Vec<TerminationDeclaration>,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .terminate_sectors(terminations.into());

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn retrieve_storage_provider(
        &self,
        account_id: &AccountId32,
    ) -> Result<Option<StorageProviderState<PeerId, Currency, BlockNumber>>, subxt::Error> {
        let storage_provider = runtime::storage()
            .storage_provider()
            .storage_providers(account_id);

        self.client
            .storage()
            .at_latest()
            .await?
            .fetch(&storage_provider)
            .await
    }

    #[tracing::instrument(level = "debug", skip_all)]
    async fn retrieve_registered_storage_providers(&self) -> Result<Vec<String>, subxt::Error> {
        let storage_providers = runtime::storage()
            .storage_provider()
            .storage_providers_iter();
        let storage_providers = self
            .client
            .storage()
            .at_latest()
            .await?
            // The iter uses pagination under the hood
            .iter(storage_providers)
            .await?;

        storage_providers
            .map_ok(|kv| bs58::encode(kv.value.info.peer_id.0.as_slice()).into_string())
            .try_collect()
            .await
    }

    // NOTE: the constants API does not use the network, relying instead on the compiled metadata
    // this means that the subxt MUST match the runtime it's connected to, otherwise, the constants
    // may make no sense (in case they're not equal across runtimes)

    fn proving_period_info(&self) -> Result<ProvingPeriodInfo, subxt::Error> {
        let query = runtime::constants()
            .storage_provider()
            .w_po_st_period_deadlines();
        let deadlines = self.client.constants().at(&query)?;

        Ok(ProvingPeriodInfo { deadlines })
    }

    fn sector_expiration_bounds(&self) -> Result<(BlockNumber, BlockNumber), subxt::Error> {
        let min_sect_exp_addr = runtime::ConstantsApi
            .storage_provider()
            .min_sector_expiration();
        let max_sect_exp_addr = runtime::ConstantsApi
            .storage_provider()
            .max_sector_expiration();

        // Since this API doesn't query the network, I'm not sure when this can fail
        // at least in our case, where this library is our "fence" around the subxt API
        let min_sector_expiration = self.client.constants().at(&min_sect_exp_addr)?;
        let max_sector_expiration = self.client.constants().at(&max_sect_exp_addr)?;

        Ok((min_sector_expiration, max_sector_expiration))
    }
}
