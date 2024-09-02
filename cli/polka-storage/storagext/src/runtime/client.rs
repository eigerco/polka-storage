use std::time::Duration;

use hex::ToHex;
use subxt::{backend::rpc, blocks::ExtrinsicEvents, OnlineClient};

use crate::PolkaStorageConfig;

const ATTEMPTS: usize = 5;
const ATTEMPT_INTERVAL: Duration = Duration::from_secs(1);

/// Helper type for [`Client::traced_submission`] successful results.
pub struct SubmissionResult<Config>
where
    Config: subxt::Config,
{
    /// Submission block hash.
    pub hash: Config::Hash,

    /// Resulting extrinsic's events.
    pub events: ExtrinsicEvents<Config>,
}

/// Client to interact with a pallet extrinsics.
/// You can call any extrinsic via [`Client::traced_submission`].
pub struct Client {
    pub(crate) client: OnlineClient<PolkaStorageConfig>,
}

impl Client {
    /// Create a new [`RuntimeClient`] from a target `rpc_address`.
    ///
    /// By default, this function does not support insecure URLs,
    /// to enable support for them, use the `insecure_url` feature.
    pub async fn new(rpc_address: impl AsRef<str>) -> Result<Self, subxt::Error> {
        let mut n_attempts = 0;
        let rpc_address = rpc_address.as_ref();
        loop {
            let client = if cfg!(feature = "insecure_url") {
                OnlineClient::<_>::from_insecure_url(rpc_address).await
            } else {
                OnlineClient::<_>::from_url(rpc_address).await
            };

            match client {
                Ok(client) => return Ok(Self { client }),
                Err(err) => {
                    tracing::error!(
                        attempt = n_attempts,
                        "failed to connect to {}, error: {}",
                        rpc_address,
                        err
                    );
                    if n_attempts >= ATTEMPTS {
                        return Err(err);
                    }
                    n_attempts += 1;
                    tokio::time::sleep(ATTEMPT_INTERVAL).await;
                }
            }
        }
    }

    /// Submit an extrinsic and wait for finalization, returning the block hash it was included in.
    ///
    /// Equivalent to performing [`OnlineClient::sign_and_submit_then_watch_default`],
    /// followed by [`TxInBlock::wait_for_finalized`] and [`TxInBlock::wait_for_success`].
    pub async fn traced_submission<Call, Keypair>(
        &self,
        call: &Call,
        account_keypair: &Keypair,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>
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
        let block_hash = finalized_xt.block_hash();
        tracing::trace!(
            block_hash = block_hash.encode_hex::<String>(),
            "successfully submitted extrinsic"
        );

        // finalized != successful
        let xt_events = finalized_xt.wait_for_success().await?;

        Ok(SubmissionResult {
            hash: block_hash,
            events: xt_events,
        })
    }
}
