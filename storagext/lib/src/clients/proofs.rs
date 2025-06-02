use futures::Future;
use primitives::proofs::{RegisteredPoStProof, RegisteredSealProof};
use sp_core::crypto::Ss58Codec;

use crate::{
    runtime::{self, SubmissionResult},
    types::proofs::VerifyingKey,
    PolkaStorageConfig,
};

pub trait ProofsClientExt {
    fn set_porep_verifying_key<Keypair>(
        &self,
        account_keypair: &Keypair,
        registered_proof: RegisteredSealProof,
        verifying_key: VerifyingKey,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;

    fn set_post_verifying_key<Keypair>(
        &self,
        account_keypair: &Keypair,
        registered_proof: RegisteredPoStProof,
        verifying_key: VerifyingKey,
        wait_for_finalization: bool,
    ) -> impl Future<Output = Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>;
}

impl ProofsClientExt for crate::runtime::client::Client {
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            address = account_keypair.account_id().to_ss58check(),
        )
    )]
    async fn set_porep_verifying_key<Keypair>(
        &self,
        account_keypair: &Keypair,
        registered_proof: RegisteredSealProof,
        verifying_key: VerifyingKey,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .proofs()
            .set_porep_verifying_key(registered_proof, verifying_key.into());

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
    async fn set_post_verifying_key<Keypair>(
        &self,
        account_keypair: &Keypair,
        registered_proof: RegisteredPoStProof,
        verifying_key: VerifyingKey,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let payload = runtime::tx()
            .proofs()
            .set_post_verifying_key(registered_proof, verifying_key.into());

        self.traced_submission(&payload, account_keypair, wait_for_finalization)
            .await
    }
}
