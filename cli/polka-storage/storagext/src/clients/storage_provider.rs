use subxt::ext::sp_core::crypto::Ss58Codec;

use crate::{
    runtime::{
        self, bounded_vec::IntoBoundedByteVec,
        runtime_types::pallet_storage_provider::proofs::SubmitWindowedPoStParams,
    },
    BlockNumber, FaultDeclaration, PolkaStorageConfig, ProveCommitSector, RecoveryDeclaration,
    RegisteredPoStProof, SectorPreCommitInfo,
};

/// The maximum number of deal IDs supported.
/// Client to interact with the market pallet extrinsics.
pub struct StorageProviderClient {
    client: crate::runtime::client::Client,
}

impl StorageProviderClient {
    /// Create a new [`MarketClient`] from a target `rpc_address`.
    ///
    /// By default, this function does not support insecure URLs,
    /// to enable support for them, use the `insecure_url` feature.
    pub async fn new(rpc_address: impl AsRef<str>) -> Result<Self, subxt::Error> {
        Ok(Self {
            client: crate::runtime::client::Client::new(rpc_address).await?,
        })
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    pub async fn register_storage_provider<Keypair>(
        &self,
        account_keypair: &Keypair,
        peer_id: String,
        post_proof: RegisteredPoStProof,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .register_storage_provider(peer_id.into_bounded_byte_vec(), post_proof);

        self.client
            .traced_submission(&payload, account_keypair)
            .await
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    pub async fn pre_commit_sector<Keypair>(
        &self,
        account_keypair: &Keypair,
        sector: SectorPreCommitInfo,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .pre_commit_sector(sector.into());

        self.client
            .traced_submission(&payload, account_keypair)
            .await
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    pub async fn prove_commit_sector<Keypair>(
        &self,
        account_keypair: &Keypair,
        prove_commit_sector: ProveCommitSector,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .prove_commit_sector(prove_commit_sector.into());

        self.client
            .traced_submission(&payload, account_keypair)
            .await
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    pub async fn submit_windowed_post<Keypair>(
        &self,
        account_keypair: &Keypair,
        windowed_post: SubmitWindowedPoStParams<BlockNumber>,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .submit_windowed_post(windowed_post);

        self.client
            .traced_submission(&payload, account_keypair)
            .await
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    pub async fn declare_faults<Keypair>(
        &self,
        account_keypair: &Keypair,
        faults: Vec<FaultDeclaration>,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .declare_faults(faults.into());

        self.client
            .traced_submission(&payload, account_keypair)
            .await
    }

    #[tracing::instrument(
        level = "trace",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    pub async fn declare_faults_recovered<Keypair>(
        &self,
        account_keypair: &Keypair,
        recoveries: Vec<RecoveryDeclaration>,
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .storage_provider()
            .declare_faults_recovered(recoveries.into());

        self.client
            .traced_submission(&payload, account_keypair)
            .await
    }
}