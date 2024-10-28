use std::{sync::Arc, time::Duration, vec::Vec};

use hex::ToHex;
use subxt::{
    blocks::ExtrinsicEvents,
    error::BlockError,
    ext::subxt_core::error::{CustomError, ExtrinsicParamsError},
    OnlineClient,
};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::PolkaStorageConfig;

type HashOf<C> = <C as subxt::Config>::Hash;
pub type HashOfPsc = HashOf<PolkaStorageConfig>;

type ParaEvents<C> = Arc<RwLock<Vec<(HashOf<C>, ExtrinsicEvents<C>)>>>;
type ParaErrors = Arc<RwLock<Vec<Box<dyn CustomError>>>>;

/// This definition defines one single, successful event of an extrinsic execution. For example,
/// one published deal, or one settled deal.
#[derive(Debug)]
pub struct ExtrinsicEvent<Hash, Event, Variant> {
    /// Submission block hash.
    pub hash: Hash,
    /// Resulting extrinsic's event.
    pub event: Event,
    /// Resulting extrinsic's event-variant.
    pub variant: Variant,
}

/// Helper type for [`Client::traced_submission`] successful results.
pub type SubmissionResult<Hash, Event, Variant> =
    Result<Vec<ExtrinsicEvent<Hash, Event, Variant>>, Box<dyn CustomError>>;

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
    pub(crate) async fn traced_submission<Call, Keypair, Event, Variant>(
        &self,
        call: &Call,
        account_keypair: &Keypair,
        wait_for_finalization: bool,
        expected_results: usize,
    ) -> Result<Option<SubmissionResult<HashOfPsc, Event, Variant>>, subxt::Error>
    where
        Call: subxt::tx::Payload + std::fmt::Debug,
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
        Event: scale_decode::DecodeAsType + std::fmt::Display,
        Variant: subxt::events::StaticEvent,
    {
        let para_events: ParaEvents<PolkaStorageConfig> = Arc::new(RwLock::new(Vec::new()));
        let para_errors: ParaErrors = Arc::new(RwLock::new(Vec::new()));
        let cancel_token = CancellationToken::new();

        let p_api = self.client.clone();
        let p_events = para_events.clone();
        let p_errors = para_errors.clone();
        let p_cancel = cancel_token.clone();

        let pw_handle = if wait_for_finalization {
            // Start parachain-event listener for collecting all events of finalised blocks.
            tokio::spawn(async move { para_watcher(p_api, p_cancel, p_events, p_errors).await })
        } else {
            // Run dummy task that does nothing except for being Ok.
            tokio::spawn(async move { Ok(()) })
        };

        tracing::trace!("submitting extrinsic");
        let submission_progress = self
            .client
            .tx()
            .sign_and_submit_then_watch_default(call, account_keypair)
            .await?;

        if !wait_for_finalization {
            return Ok(None);
        }
        let extrinsic_hash = submission_progress.extrinsic_hash();
        tracing::trace!(
            "waiting for finalization {}",
            extrinsic_hash.encode_hex::<String>(),
        );

        let submission_result = wait_for_para_event::<PolkaStorageConfig, Event, Variant>(
            para_events.clone(),
            para_errors.clone(),
            extrinsic_hash,
            expected_results,
        )
        .await?;
        cancel_token.cancel();
        pw_handle
            .await
            .map_err(|e| subxt::Error::Other(format!("JoinHandle: {e:?}")))??;

        Ok(Some(submission_result))
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
async fn wait_for_para_event<C, E, V>(
    event_stack: ParaEvents<C>,
    _error_stack: ParaErrors,
    extrinsic_hash: HashOf<C>,
    expected_results: usize,
) -> Result<SubmissionResult<HashOf<C>, E, V>, subxt::Error>
where
    C: subxt::Config + Clone + std::fmt::Debug,
    E: scale_decode::DecodeAsType + std::fmt::Display,
    V: subxt::events::StaticEvent,
{
    let mut catched_events = Vec::<ExtrinsicEvent<HashOf<C>, E, V>>::new();

    loop {
        // Check for new events from a finalised block.
        let mut events_lock = event_stack.write().await;
        while let Some((hash, ex_events)) = events_lock.pop() {
            if ex_events.extrinsic_hash() == extrinsic_hash {
                // Currently, it is assumed only one event to be contained, because only one event
                // will be emitted in case of a successful extrinsoc.
                if let Some(entry) = ex_events.iter().find(|_| true) {
                    let entry = entry?;
                    let event = entry
                        .as_root_event::<E>()
                        .map_err(|e| subxt::Error::Other(format!("{entry:?}: {e:?}")))?;
                    let variant = entry
                        .as_event::<V>()
                        .map_err(|e| subxt::Error::Other(format!("{entry:?}: {e:?}")))?
                        .ok_or(subxt::Error::Other(format!(
                            "{entry:?}: inner option error"
                        )))?;
                    tracing::trace!(
                        "Found related event to extrinsic with hash {:?}",
                        extrinsic_hash
                    );
                    catched_events.push(ExtrinsicEvent::<HashOf<C>, E, V> {
                        hash,
                        event,
                        variant,
                    });
                    if catched_events.len() == expected_results {
                        return Ok(Ok(catched_events));
                    }
                }
            }
        }
        drop(events_lock);

        // Check for new collected custom errors (extrinsic errors).
        // Check if one error is sufficient for compound exeuctions (i.e. multiple sectors).
        // TODO(@neutrinoks,25.10.24): Implement error filtering and test it.
        // let mut errors = errors.write().await;
        // while let Some(error) = errors.pop() {
        //     let error = match error.downcast_ref::<ErrorType>() {
        //         Some(e) => e,
        //         None => continue,
        //     };
        //     // Push found pallet-related error somewhere.
        // }
        // drop(errors);

        // Blocks are generated only every couple of seconds, so don't waste CPU time.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

/// Method listens to finalised blocks, collects all events and pushes them to a given stack.
async fn para_watcher<C: subxt::Config + Clone>(
    api: OnlineClient<C>,
    token: CancellationToken,
    events: ParaEvents<C>,
    errors: ParaErrors,
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
                    if let subxt::Error::ExtrinsicParams(ExtrinsicParamsError::Custom(
                        boxed_custom_err,
                    )) = error
                    {
                        errors.write().await.push(boxed_custom_err);
                    }
                }
            }
        }
    }

    tracing::trace!("stopped event-listener");
    Ok(())
}
