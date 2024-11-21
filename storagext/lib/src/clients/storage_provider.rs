use std::future::Future;

use primitives_proofs::RegisteredPoStProof;
use runtime::runtime_types::bounded_collections::bounded_vec::BoundedVec;
use subxt::{
    ext::{futures::TryStreamExt, sp_core::crypto::Ss58Codec},
    utils::AccountId32,
};

use crate::{
    runtime::{
        self,
        bounded_vec::IntoBoundedByteVec,
        client::SubmissionResult,
        runtime_types::pallet_storage_provider::{
            proofs::SubmitWindowedPoStParams, storage_provider::StorageProviderState,
        },
        storage_provider::calls::types::register_storage_provider::PeerId,
    },
    types::storage_provider::{
        FaultDeclaration, ProveCommitSector, RecoveryDeclaration, SectorPreCommitInfo,
        TerminationDeclaration,
    },
    BlockNumber, Currency, PolkaStorageConfig,
};
pub trait StorageProviderClientExt {
    fn register_storage_provider<Keypair>(
        &self,
        account_keypair: &Keypair,
        peer_id: String,
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


    fn current_deadline(&self, account_id: &AccountId32) -> impl Future<Output = Result<u64, subxt::Error>>;
}

impl StorageProviderClientExt for crate::runtime::client::Client {
    async fn current_deadline(&self, account_id: &AccountId32) -> Result<u64, subxt::Error> {
        // let _ = runtime::apis().storage_provider_api().current_deadline(account_id);
        // let _ = runtime::apis().account_nonce_api().account_nonce(account_id);

        Ok(0)
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
        peer_id: String,
        post_proof: RegisteredPoStProof,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .register_storage_provider(peer_id.into_bounded_byte_vec(), post_proof);

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
            .submit_windowed_post(windowed_post);

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
}