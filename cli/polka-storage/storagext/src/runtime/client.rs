use std::time::Duration;

use codec::Encode;
use hex::ToHex;
use subxt::{blocks::Block, events::Events, OnlineClient};

use crate::PolkaStorageConfig;

/// Helper type for [`Client::traced_submission`] successful results.
#[derive(Debug)]
pub struct SubmissionResult<Config>
where
    Config: subxt::Config,
{
    /// Submission block hash.
    pub hash: Config::Hash,

    /// Submission block height.
    pub height: u64,

    /// Resulting extrinsic's events.
    pub events: Events<Config>,
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
    #[tracing::instrument(skip_all, fields(rpc_address = rpc_address.as_ref()))]
    pub async fn new(
        rpc_address: impl AsRef<str>,
        n_retries: u32,
        retry_interval: Duration,
    ) -> Result<Self, subxt::Error> {
        let rpc_address = rpc_address.as_ref();

        let mut current_retries = 0;
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
                        attempt = current_retries,
                        "failed to connect to node, error: {}",
                        err
                    );
                    current_retries += 1;
                    if current_retries >= n_retries {
                        return Err(err);
                    }
                    tokio::time::sleep(retry_interval).await;
                }
            }
        }
    }

    /// Submit an extrinsic and wait for finalization, returning the block hash it was included in.
    ///
    /// Equivalent to performing [`OnlineClient::sign_and_submit_then_watch_default`],
    /// followed by [`TxInBlock::wait_for_finalized`] and [`TxInBlock::wait_for_success`].
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
            self.traced_submission_with_finalization(call, account_keypair)
                .await
                .map(Option::Some)
        } else {
            tracing::trace!("submitting extrinsic");
            let extrinsic_hash = self
                .client
                .tx()
                .sign_and_submit_default(call, account_keypair)
                .await?;

            tracing::trace!(
                extrinsic_hash = extrinsic_hash.encode_hex::<String>(),
                "waiting for finalization"
            );
            Ok(None)
        }
    }

    pub(crate) async fn traced_submission_with_finalization<Call, Keypair>(
        &self,
        call: &Call,
        account_keypair: &Keypair,
    ) -> Result<SubmissionResult<PolkaStorageConfig>, subxt::Error>
    where
        Call: subxt::tx::Payload,
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
    {
        tracing::trace!("submitting extrinsic");

        let mut finalized_block_stream = self.client.blocks().subscribe_finalized().await?;

        let submitted_extrinsic_hash = self
            .client
            .tx()
            .sign_and_submit_default(call, account_keypair)
            .await?;

        tracing::debug!(
            extrinsic_hash = submitted_extrinsic_hash.encode_hex::<String>(),
            "waiting for finalization"
        );

        let metadata = self.client.metadata();

        tracing::debug!("ext metadata {:?}", metadata.extrinsic());

        let finalized_block = tokio::task::spawn(async move {
            'outer: loop {
                let Some(block) = finalized_block_stream.next().await else {
                    return Err(subxt::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "stream was closed",
                    )));
                };

                let block: Block<PolkaStorageConfig, _> = block?;
                tracing::debug!(
                    "checking block number: {} hash: {}",
                    block.number(),
                    block.hash()
                );

                for extrinsic in block.extrinsics().await?.iter() {
                    let extrinsic = extrinsic?;

                    // There's a bug on subxt that forces us to use this thing,
                    // in 0.38 we can just use .hash(), in fact, in 0.38 this line doesn't work!
                    // https://github.com/paritytech/subxt/discussions/1851#discussioncomment-11133684
                    let extrinsic_hash = extrinsic.events().await?.extrinsic_hash();

                    if submitted_extrinsic_hash == extrinsic_hash {
                        // Extrinsic failures are placed in the same block as the extrinsic.
                        let failed_extrinsic_event: Option<
                            crate::runtime::system::events::ExtrinsicFailed,
                        > = block.events().await?.find_first()?;

                        if let Some(event) = failed_extrinsic_event {
                            // debug level since we're returning the error upwards
                            tracing::debug!("found a failing extrinsic: {:?}", event);
                            // this weird encode/decode is the shortest and simplest way to convert the
                            // generated subxt types into the canonical types since we can't replace them
                            // with the proper ones
                            let encoded_event = event.encode();
                            let dispatch_error =
                                subxt::error::DispatchError::decode_from(encoded_event, metadata)?;
                            return Err(dispatch_error.into());
                        }

                        break 'outer Ok(block);
                    }
                }
            }
        });

        // 1 block = 6 seconds -> 120 seconds = 20 blocks
        // since the subscription has like a ~6 block delay
        let timeout = tokio::time::timeout(Duration::from_secs(120), finalized_block).await;

        match timeout {
            Ok(Ok(result)) => {
                let result = result?;
                Ok(SubmissionResult {
                    hash: result.hash(),
                    height: result.number(),
                    events: result.events().await?,
                })
            }
            Ok(Err(_)) => Err(subxt::Error::Other("failed to join tasks".to_string())),
            Err(_) => Err(subxt::Error::Other(
                "timeout while waiting for the extrinsic call to be finalized".to_string(),
            )),
        }
    }
}

impl From<OnlineClient<PolkaStorageConfig>> for Client {
    fn from(client: OnlineClient<PolkaStorageConfig>) -> Self {
        Self { client }
    }
}
