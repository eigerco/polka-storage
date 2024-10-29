use std::{sync::Arc, time::Duration, vec::Vec};

use hex::ToHex;
use subxt::{blocks::ExtrinsicEvents, error::BlockError, OnlineClient};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::PolkaStorageConfig;

/// Helper type to access the hash type of a given subxt configuration.
type HashOf<C> = <C as subxt::Config>::Hash;
/// Hash type of our default used subxt configuration.
pub type HashOfPsc = HashOf<PolkaStorageConfig>;

/// Helper definition to define a stack for storing collected events with block hash.
type ParaEvents<C> = Arc<RwLock<Vec<(HashOf<C>, ExtrinsicEvents<C>)>>>;

/// This definition defines one single, successful event of an extrinsic execution. For example,
/// one published deal, or one settled deal.
#[derive(Debug)]
pub struct ExtrinsicEvent<Hash, Variant> {
    /// Submission block hash, final enum variant.
    pub hash: Hash,
    /// Resulting extrinsic's event.
    /// This additional more complex type is needed by the formatter `OutputFormat`.
    pub event: crate::runtime::Event,
    /// Resulting extrinsic's event-variant.
    pub variant: Variant,
}

/// Helper type for [`Client::traced_submission`] successful results.
///
/// Currently, our pallet's extrinsic calls do emit either a single event or multiple events. For
/// that reason we need here `Vec<ExtrinsicEvent<...>>`. If we would harmonize that that way
/// extrinsics are emitting pnly one event per extrinsic call, we could remove the `Vec`.
pub type SubmissionResult<Hash, Variant> = Result<Vec<ExtrinsicEvent<Hash, Variant>>, ()>;

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
    pub(crate) async fn traced_submission<Call, Keypair, Variant>(
        &self,
        call: &Call,
        account_keypair: &Keypair,
        wait_for_finalization: bool,
        expected_events: usize,
    ) -> Result<Option<SubmissionResult<HashOfPsc, Variant>>, subxt::Error>
    where
        Call: subxt::tx::Payload + std::fmt::Debug,
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
        Variant: subxt::events::StaticEvent + std::fmt::Debug,
    {
        if wait_for_finalization {
            let para_events: ParaEvents<PolkaStorageConfig> = Arc::new(RwLock::new(Vec::new()));
            let cancel_token = CancellationToken::new();

            let p_api = self.client.clone();
            let p_events = para_events.clone();
            let p_cancel = cancel_token.clone();

            let pw_handle =
                tokio::spawn(async move { para_watcher(p_api, p_cancel, p_events).await });

            tracing::trace!("submitting extrinsic");
            let submission_progress = self
                .client
                .tx()
                .sign_and_submit_then_watch_default(call, account_keypair)
                .await?;

            let extrinsic_hash = submission_progress.extrinsic_hash();
            tracing::trace!(
                "waiting for finalization {}",
                extrinsic_hash.encode_hex::<String>(),
            );

            let submission_result = wait_for_para_event::<PolkaStorageConfig, Variant>(
                para_events.clone(),
                extrinsic_hash,
                expected_events,
            )
            .await?;
            cancel_token.cancel();
            pw_handle
                .await
                .map_err(|e| subxt::Error::Other(format!("JoinHandle: {e:?}")))??;

            Ok(Some(submission_result))
        } else {
            self.client
                .tx()
                .sign_and_submit_then_watch_default(call, account_keypair)
                .await?;
            Ok(None)
        }
    }
}

impl From<OnlineClient<PolkaStorageConfig>> for Client {
    fn from(client: OnlineClient<PolkaStorageConfig>) -> Self {
        Self { client }
    }
}

/// Methods iterates through the given stack of collected events from the listener and compares for
/// a given expected event type, for example `pallet_market::Event::BalanceAdded`. If the event has
/// been found it will be returned.
async fn wait_for_para_event<C, V>(
    event_stack: ParaEvents<C>,
    extrinsic_hash: HashOf<C>,
    expected_events: usize,
) -> Result<SubmissionResult<HashOf<C>, V>, subxt::Error>
where
    C: subxt::Config + Clone + std::fmt::Debug,
    V: subxt::events::StaticEvent + std::fmt::Debug,
{
    let mut catched_events = Vec::<ExtrinsicEvent<HashOf<C>, V>>::new();

    loop {
        // Check for new events from a finalised block.
        let mut events_lock = event_stack.write().await;
        while let Some((hash, ex_events)) = events_lock.pop() {
            if ex_events.extrinsic_hash() == extrinsic_hash {
                for entry in ex_events.iter() {
                    let entry = entry?;
                    let event_name = format!("{}::{}", entry.pallet_name(), entry.variant_name());

                    if entry.pallet_name() == V::PALLET && entry.variant_name() == V::EVENT {
                        let event =
                            entry
                                .as_root_event::<crate::runtime::Event>()
                                .map_err(|e| {
                                    subxt::Error::Other(format!(
                                        "{event_name}.as_root_event(): {e:?}"
                                    ))
                                })?;
                        let variant = entry
                            .as_event::<V>()
                            .map_err(|e| {
                                subxt::Error::Other(format!("{event_name}.as_event() {e:?}"))
                            })?
                            .ok_or(subxt::Error::Other(format!(
                                "{event_name}: inner option error"
                            )))?;
                        tracing::trace!(
                            "Found related event {event_name} to extrinsic with hash {:?}",
                            extrinsic_hash
                        );
                        catched_events.push(ExtrinsicEvent::<HashOf<C>, V> {
                            hash,
                            event,
                            variant,
                        });
                    } else if entry.pallet_name() == "System"
                        && entry.variant_name() == "ExtrinsicFailed"
                    {
                        return Ok(Err(()));
                    }

                    if catched_events.len() == expected_events {
                        return Ok(Ok(catched_events));
                    }
                }
            }
        }
        drop(events_lock);

        // Blocks are generated only every couple of seconds, so don't waste CPU time.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// Method listens to finalised blocks, collects all events and pushes them to a given stack.
async fn para_watcher<C: subxt::Config + Clone>(
    api: OnlineClient<C>,
    token: CancellationToken,
    events: ParaEvents<C>,
) -> Result<(), subxt::Error>
where
    <C::Header as subxt::config::Header>::Number: std::fmt::Display,
{
    tracing::trace!("start listening to events on finalised blocks");
    let mut blocks_sub = api.blocks().subscribe_finalized().await?;

    loop {
        let block = tokio::select! {
            _ = token.cancelled() => {
                break
            }
            block = blocks_sub.next() => {
                if let Some(block) = block {
                    block?
                } else {
                    return Err(subxt::Error::Block(BlockError::NotFound("blocks_sub::next() returned None".to_string())))
                }
            }
        };

        let block_hash = block.hash();

        for extrinsic in block.extrinsics().await?.iter() {
            match extrinsic {
                Ok(extrinsic) => {
                    let ex_events = extrinsic.events().await?;
                    events.write().await.push((block_hash, ex_events));
                }
                Err(error) => {
                    tracing::trace!("found error: {:?}", error);
                }
            }
        }
    }

    tracing::trace!("stopped event-listener");
    Ok(())
}
