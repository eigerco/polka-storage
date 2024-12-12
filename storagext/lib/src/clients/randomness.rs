use std::future::Future;

use crate::{runtime, BlockNumber};

/// Client to interact with the randomness pallet.
pub trait RandomnessClientExt {
    /// Get randomness for a specific block
    fn get_randomness(
        &self,
        block_number: BlockNumber,
    ) -> impl Future<Output = Result<Option<[u8; 32]>, subxt::Error>>;
}

impl RandomnessClientExt for crate::runtime::client::Client {
    async fn get_randomness(
        &self,
        _block_number: BlockNumber,
    ) -> Result<Option<[u8; 32]>, subxt::Error> {
        let seed_query = runtime::storage().randomness().author_vrf();

        self.client
            .storage()
            .at_latest()
            .await?
            .fetch(&seed_query)
            .await
            .map(|opt_hash| opt_hash.map(|hash| *hash.as_fixed_bytes()))
    }
}
