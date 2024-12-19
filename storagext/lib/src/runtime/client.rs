use std::time::Duration;

use hex::ToHex;
use subxt::{
    backend::{
        legacy::LegacyRpcMethods,
        rpc::reconnecting_rpc_client::{FixedInterval, RpcClient},
    },
    blocks::ExtrinsicEvents,
    config::DefaultExtrinsicParamsBuilder,
    tx::SubmittableExtrinsic,
    OnlineClient,
};
use tokio::sync::Mutex;

use crate::PolkaStorageConfig;

/// Helper type for [`Client::traced_submission`] successful results.
#[derive(Debug)]
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
    pub(crate) legacy_rpc: LegacyRpcMethods<PolkaStorageConfig>,
    /// We're not using AtomicU64 as we need to hold the critical sections across many instructions.
    /// Look at [`Self::traced_submission`].
    last_sent_nonce: Mutex<u64>,
}

impl Client {
    /// Create a new [`RuntimeClient`] from a target `rpc_address`.
    #[tracing::instrument(skip_all, fields(rpc_address = rpc_address.as_ref()))]
    pub async fn new(
        rpc_address: impl AsRef<str>,
        n_retries: u32,
        retry_interval: Duration,
    ) -> Result<Self, subxt::Error> {
        let rpc_address = rpc_address.as_ref();

        let rpc_client = RpcClient::builder()
            // the cast should never pose an issue since storagext is target at 64bit systems
            .retry_policy(FixedInterval::new(retry_interval).take(n_retries as usize))
            .build(rpc_address)
            // subxt-style conversion
            // https://github.com/paritytech/subxt/blob/v0.38.0/subxt/src/backend/rpc/rpc_client.rs#L38
            .await
            .map_err(|e| subxt::error::RpcError::ClientError(Box::new(e)))?;

        Ok(Self {
            client: OnlineClient::<_>::from_rpc_client(rpc_client.clone()).await?,
            legacy_rpc: LegacyRpcMethods::<_>::new(rpc_client.into()),
            last_sent_nonce: Mutex::new(0),
        })
    }

    pub(crate) async fn unsigned<Call>(
        &self,
        call: &Call,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Call: subxt::tx::Payload,
    {
        if wait_for_finalization {
            let tx = self
                .client
                .tx()
                .create_unsigned(call)?
                .submit_and_watch()
                .await?;
            let events = tx.wait_for_finalized_success().await?;
            Ok(Some(SubmissionResult {
                hash: events.extrinsic_hash(),
                events: events,
            }))
        } else {
            tracing::trace!("submitting unsigned extrinsic");
            let extrinsic_hash = self.client.tx().create_unsigned(call)?.submit().await?;

            tracing::trace!(
                extrinsic_hash = extrinsic_hash.encode_hex::<String>(),
                "waiting for finalization"
            );

            Ok(None)
        }
    }

    /// Submit an extrinsic and wait for finalization, returning the block hash it was included in.
    /// It is thread-safe, allows to submit multiple extrinsics at the same time.
    /// If another process is submitting the transactions at the same time, the retry mechanism at the higher layer is needed.
    ///
    /// Equivalent to performing [`OnlineClient::sign_and_submit_then_watch_default`],
    /// followed by [`TxInBlock::wait_for_finalized`] and [`TxInBlock::wait_for_success`].
    ///
    /// ## Nonce mechanism
    ///
    /// ### Context
    /// Each transaction sent to the blockchain must have a nonce. Nonces are incremented sequentially and cannot have gaps.
    /// If you submit a transaction with the same nonce, one of them will fail or be replaced. Dependent on the priority (transaction size).
    ///
    /// ### Solution
    ///
    /// The current solution for this is optimistic. It is fetching the nonce using `system_account_next_index` from the **best block** and using it as a nonce.
    /// Returned index is taking into the account transactions already included in the blocks and the ones pending (in the transaction pool).
    /// To avoid the race condition between the tasks in the same process a critical section is introduced.
    /// It locks the extrinsic submission, so the next task is allowed to fetch the next index only after the previous has been submitted (txpool updated).
    ///
    /// 1. We assume we connect to the same node for each transaction performed, if we didn't, then the possibility of nonce collisions would be more frequent.
    /// 2. When we `.submit()` a transaction and it fails, the nonce is not updated, so next time we call `system_account_next_index`, it'll return the same nonce.
    /// 3. When we `.submit()` a transaction and it succeeds, the nonce is updated, next returned nonce will be incremented.
    /// 4. If any other process submits the transaction, after we fetch the current_nonce, this call will:
    ///      a) fail (transaction outdated)
    ///      b) fail (will be replaced by the other process transaction)
    ///      c) succeed (replace the other process transaction)
    /// 5. Because of the 1. and 4., the retry mechanism would be needed and the error is detectable:
    ///     a) at the `.submit()` level, when nonce < chain_nonce OR  nonce == chain_nonce && tx1_priority < tx2_priority.
    ///     b) only after waiting for finalization and not getting the event (TimeoutError).
    pub(crate) async fn traced_submission<Call, Keypair>(
        &self,
        call: &Call,
        account_keypair: &Keypair,
        wait_for_finalization: bool,
    ) -> Result<Option<SubmissionResult<PolkaStorageConfig>>, subxt::Error>
    where
        Call: subxt::tx::Payload,
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        if wait_for_finalization {
            // Critical section so the lock is released after nonce is updated in the node's txpool.
            let tx = {
                let mut last_sent_nonce = self.last_sent_nonce.lock().await;

                let (current_nonce, ext) = self.extrinsic_with_nonce(call, account_keypair).await?;
                let tx = ext.submit_and_watch().await?;

                tracing::debug!(
                    "Waiting for finalization, Previous nonce: {}, next nonce: {}",
                    last_sent_nonce,
                    current_nonce
                );
                *last_sent_nonce = current_nonce;
                tx
            };

            let events = tx.wait_for_finalized_success().await?;
            Ok(Some(SubmissionResult {
                hash: events.extrinsic_hash(),
                events: events,
            }))
        } else {
            // Critical section so the lock is released after nonce is updated in the node's txpool.
            let hash = {
                let mut last_sent_nonce = self.last_sent_nonce.lock().await;

                let (current_nonce, ext) = self.extrinsic_with_nonce(call, account_keypair).await?;
                let tx_hash = ext.submit().await?;

                tracing::debug!(
                    "Not waiting for finalization, previous nonce: {}, next nonce: {}",
                    last_sent_nonce,
                    current_nonce
                );
                *last_sent_nonce = current_nonce;
                tx_hash
            };
            tracing::trace!(
                extrinsic_hash = hash.encode_hex::<String>(),
                "extrinsic published, not waiting for the finalization"
            );
            Ok(None)
        }
    }

    async fn extrinsic_with_nonce<Keypair, Call>(
        &self,
        call: &Call,
        account_keypair: &Keypair,
    ) -> Result<
        (
            u64,
            SubmittableExtrinsic<PolkaStorageConfig, OnlineClient<PolkaStorageConfig>>,
        ),
        subxt::Error,
    >
    where
        Call: subxt::tx::Payload,
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        let current_nonce = self
            .legacy_rpc
            .system_account_next_index(&account_keypair.account_id())
            .await?;
        let current_header = self.legacy_rpc.chain_get_header(None).await?.unwrap();
        let ext_params = DefaultExtrinsicParamsBuilder::new()
            .mortal(&current_header, 8)
            .nonce(current_nonce)
            .build();

        let ext = self
            .client
            .tx()
            .create_signed_offline(call, account_keypair, ext_params)?;

        Ok((current_nonce, ext))
    }
}
