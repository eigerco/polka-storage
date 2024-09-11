use std::future::Future;

use runtime::runtime_types::bounded_collections::bounded_vec::BoundedVec;
use subxt::ext::{futures::TryStreamExt, sp_core::crypto::Ss58Codec};

use crate::{
    runtime::{
        self, bounded_vec::IntoBoundedByteVec, client::SubmissionResult,
        runtime_types::pallet_storage_provider::proofs::SubmitWindowedPoStParams,
    },
    FaultDeclaration, PolkaStorageConfig, ProveCommitSector, RecoveryDeclaration,
    RegisteredPoStProof, SectorPreCommitInfo,
};

pub trait StorageProviderClientExt {
    fn register_storage_provider<Keypair>(
        &self,
        account_keypair: &Keypair,
        peer_id: String,
        post_proof: RegisteredPoStProof,
    ) -> impl Future<Output = Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn pre_commit_sectors<Keypair>(
        &self,
        account_keypair: &Keypair,
        sectors: Vec<SectorPreCommitInfo>,
    ) -> impl Future<Output = Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn prove_commit_sector<Keypair>(
        &self,
        account_keypair: &Keypair,
        prove_commit_sector: ProveCommitSector,
    ) -> impl Future<Output = Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn submit_windowed_post<Keypair>(
        &self,
        account_keypair: &Keypair,
        windowed_post: SubmitWindowedPoStParams,
    ) -> impl Future<Output = Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn declare_faults<Keypair>(
        &self,
        account_keypair: &Keypair,
        faults: Vec<FaultDeclaration>,
    ) -> impl Future<Output = Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn declare_faults_recovered<Keypair>(
        &self,
        account_keypair: &Keypair,
        recoveries: Vec<RecoveryDeclaration>,
    ) -> impl Future<Output = Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn retrieve_registered_storage_providers(
        &self,
    ) -> impl Future<Output = Result<Vec<String>, subxt::Error>>;
}

impl StorageProviderClientExt for crate::runtime::client::Client {
    #[tracing::instrument(
        level = "trace",
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
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .register_storage_provider(peer_id.into_bounded_byte_vec(), post_proof);

        self.traced_submission(&payload, account_keypair).await
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn pre_commit_sectors<Keypair>(
        &self,
        account_keypair: &Keypair,
        sectors: Vec<SectorPreCommitInfo>,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let sectors = BoundedVec(sectors.into_iter().map(Into::into).collect());
        let payload = runtime::tx().storage_provider().pre_commit_sectors(sectors);

        self.traced_submission(&payload, account_keypair).await
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn prove_commit_sector<Keypair>(
        &self,
        account_keypair: &Keypair,
        prove_commit_sector: ProveCommitSector,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .prove_commit_sector(prove_commit_sector.into());

        self.traced_submission(&payload, account_keypair).await
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn submit_windowed_post<Keypair>(
        &self,
        account_keypair: &Keypair,
        windowed_post: SubmitWindowedPoStParams,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .submit_windowed_post(windowed_post);

        self.traced_submission(&payload, account_keypair).await
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn declare_faults<Keypair>(
        &self,
        account_keypair: &Keypair,
        faults: Vec<FaultDeclaration>,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .declare_faults(faults.into());

        self.traced_submission(&payload, account_keypair).await
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn declare_faults_recovered<Keypair>(
        &self,
        account_keypair: &Keypair,
        recoveries: Vec<RecoveryDeclaration>,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .declare_faults_recovered(recoveries.into());

        self.traced_submission(&payload, account_keypair).await
    }

    #[tracing::instrument(level = "trace", skip_all)]
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
