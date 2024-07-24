// TODO:
// - precommit sector
// - prove commit sector
use subxt::ext::sp_core::crypto::Ss58Codec;

use crate::{
    runtime::{self, bounded_vec::IntoBoundedByteVec},
    PolkaStorageConfig,
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
    ) -> Result<<PolkaStorageConfig as subxt::Config>::Hash, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx().storage_provider().register_storage_provider(
                peer_id.into_bounded_byte_vec(),
                runtime::runtime_types::primitives_proofs::types::RegisteredPoStProof::StackedDRGWindow2KiBV1P1
        );

        self.client
            .traced_submission(&payload, account_keypair)
            .await
    }
}
