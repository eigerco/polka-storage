use std::future::Future;

use crate::{
    runtime::{self, SubmissionResult},
    PolkaStorageConfig,
};

/// Client to interact with the faucet pallet.
pub trait FaucetClientExt {
    /// Drip funds into the provided account.
    fn drip(
        &self,
        account_id: <PolkaStorageConfig as subxt::Config>::AccountId,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>;
}

impl FaucetClientExt for crate::runtime::client::Client {
    async fn drip(
        &self,
        account_id: <PolkaStorageConfig as subxt::Config>::AccountId,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error> {
        let payload = runtime::tx()
            .faucet()
            .drip(subxt::utils::AccountId32::from(account_id));

        self.unsigned(&payload, wait_for_finalization).await
    }
}
