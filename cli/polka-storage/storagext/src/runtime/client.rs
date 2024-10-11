use std::{sync::Arc, time::Duration, vec::Vec};

use hex::ToHex;
use subxt::{
    ext::subxt_core::error::{CustomError, Error as SxtCoreError, ExtrinsicParamsError},
    utils::AccountId32,
    OnlineClient,
};
use tokio::{select, sync::RwLock};
use tokio_util::sync::CancellationToken;

use crate::{
    runtime::{market::events as MarketEvents, storage_provider::events as SpEvents},
    PolkaStorageConfig,
};

type HashOf<C> = <C as subxt::Config>::Hash;
pub type HashOfPsc = HashOf<PolkaStorageConfig>;

type ParaEvents<C> = Arc<RwLock<Vec<(u64, HashOf<C>, subxt::events::EventDetails<C>)>>>;
type ParaErrors = Arc<RwLock<Vec<Box<dyn CustomError>>>>;

/// Helper type for [`Client::traced_submission`] successful results.
#[derive(Debug)]
pub struct SubmissionResult<Hash, Event> {
    /// Submission block hash.
    pub hash: Vec<Hash>,
    /// Resulting extrinsic's events.
    pub event: Vec<Event>,
}

impl<Hash, Event> SubmissionResult<Hash, Event> {
    /// New type pattern with empty vectors.
    pub fn new() -> Self {
        Self {
            hash: Vec::new(),
            event: Vec::new(),
        }
    }

    // Like any len() method.
    pub fn len(&self) -> usize {
        debug_assert_eq!(self.hash.len(), self.event.len());
        self.hash.len()
    }
}

impl<Hash, Event> Default for SubmissionResult<Hash, Event> {
    fn default() -> Self {
        Self::new()
    }
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
    pub(crate) async fn traced_submission<Call, Keypair, Event>(
        &self,
        call: &Call,
        account_keypair: &Keypair,
        wait_for_finalization: bool,
        n_events: usize,
    ) -> Result<Option<SubmissionResult<HashOfPsc, Event>>, subxt::Error>
    where
        Call: subxt::tx::Payload + std::fmt::Debug,
        Keypair: subxt::tx::Signer<PolkaStorageConfig>,
        Event: subxt::events::StaticEvent + std::fmt::Debug,
        EventFilterProvider: EventFilterType<Event>,
    {
        let para_events: ParaEvents<PolkaStorageConfig> = Arc::new(RwLock::new(Vec::new()));
        let para_errors: ParaErrors = Arc::new(RwLock::new(Vec::new()));
        let account_id = AccountId32::from(account_keypair.account_id());
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
        tracing::trace!(
            extrinsic_hash = submission_progress.extrinsic_hash().encode_hex::<String>(),
            "waiting for finalization"
        );

        static EVENT_META_PROVIDER: EventFilterProvider = EventFilterProvider;
        let submission_result = wait_for_para_event::<PolkaStorageConfig, Event>(
            para_events.clone(),
            para_errors.clone(),
            <EventFilterProvider as EventFilterType<Event>>::pallet_name(&EVENT_META_PROVIDER),
            <EventFilterProvider as EventFilterType<Event>>::event_name(&EVENT_META_PROVIDER),
            <EventFilterProvider as EventFilterType<Event>>::filter_fn(
                &EVENT_META_PROVIDER,
                account_id,
            ),
            n_events,
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
async fn wait_for_para_event<C, E>(
    events: ParaEvents<C>,
    _errors: ParaErrors,
    pallet: &'static str,
    variant: &'static str,
    predicate: impl Fn(&E) -> bool,
    n_events: usize,
) -> Result<SubmissionResult<HashOf<C>, E>, subxt::Error>
where
    C: subxt::Config + Clone + std::fmt::Debug,
    E: subxt::events::StaticEvent + std::fmt::Debug,
{
    let mut result = SubmissionResult::<HashOf<C>, E>::new();

    loop {
        // Check for new events from a finalised block.
        let mut events = events.write().await;
        if let Some(entry) = events
            .iter()
            .find(|&e| e.2.pallet_name() == pallet && e.2.variant_name() == variant)
        {
            let event_variant = entry
                .2
                .as_event::<E>()
                .map_err(|e| subxt::Error::Other(format!("{entry:?}: {e:?}")))?
                .ok_or(subxt::Error::Other(format!(
                    "{entry:?}: inner Option::None"
                )))?;
            if !predicate(&event_variant) {
                continue;
            }
            let entry = entry.clone();
            events.retain(|e| e.0 > entry.0);
            tracing::trace!(
                "Found related event {}::{} on block {}",
                pallet,
                variant,
                entry.0
            );
            result.hash.push(entry.1);
            result.event.push(event_variant);
            if result.len() == n_events {
                return Ok(result);
            }
        }
        drop(events);

        // Check for new collected custom errors (extrinsic errors).
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
        let block = select! {
            _ = token.cancelled() => {
                break
            }
            block = blocks_sub.next() => {
                if let Some(block) = block {
                    block?
                } else {
                    continue
                }
            }
        };

        let hash = block.hash();

        for event in block.events().await?.iter() {
            match event {
                Ok(event) => {
                    events
                        .write()
                        .await
                        .push((block.number().into(), hash, event.clone()));
                }
                Err(error) => {
                    if let SxtCoreError::ExtrinsicParams(ExtrinsicParamsError::Custom(
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

// TODO(@neutrinoks,25.10.24): Check whether we can search for that event/extrinsic by comparing
// the hash or bytes instead, to avoid all the following definitions.
/// There is `subxt::events::StaticEvent` which provides the pallet's name and its event's name. On
/// top we need individualized filter methods to check whether the event in focus is exactly ours
/// (consider a situation with multiple events of same type but from different users). This trait
/// extends the existing `StaticEvent` by that filter function which can be specified here
/// individually to adapt to polka-storage needs.
pub trait EventFilterType<Event: subxt::events::StaticEvent> {
    fn pallet_name(&self) -> &str {
        <Event as subxt::events::StaticEvent>::PALLET
    }

    fn event_name(&self) -> &str {
        <Event as subxt::events::StaticEvent>::EVENT
    }

    fn filter_fn(&self, acc: AccountId32) -> impl Fn(&Event) -> bool;
}

/// A default implementation of `EventFilterType` that implements every `Event` variant in
/// polka-storage.
pub struct EventFilterProvider;

impl EventFilterType<MarketEvents::BalanceAdded> for EventFilterProvider {
    fn filter_fn(&self, acc: AccountId32) -> impl Fn(&MarketEvents::BalanceAdded) -> bool {
        move |e: &MarketEvents::BalanceAdded| e.who == acc
    }
}

impl EventFilterType<MarketEvents::BalanceWithdrawn> for EventFilterProvider {
    fn filter_fn(&self, acc: AccountId32) -> impl Fn(&MarketEvents::BalanceWithdrawn) -> bool {
        move |e: &MarketEvents::BalanceWithdrawn| e.who == acc
    }
}

impl EventFilterType<MarketEvents::DealsSettled> for EventFilterProvider {
    fn filter_fn(&self, _: AccountId32) -> impl Fn(&MarketEvents::DealsSettled) -> bool {
        move |_: &MarketEvents::DealsSettled| true
    }
}

impl EventFilterType<MarketEvents::DealPublished> for EventFilterProvider {
    fn filter_fn(&self, _: AccountId32) -> impl Fn(&MarketEvents::DealPublished) -> bool {
        move |_: &MarketEvents::DealPublished| true
    }
}

impl EventFilterType<SpEvents::StorageProviderRegistered> for EventFilterProvider {
    fn filter_fn(&self, acc: AccountId32) -> impl Fn(&SpEvents::StorageProviderRegistered) -> bool {
        move |e: &SpEvents::StorageProviderRegistered| e.owner == acc
    }
}

impl EventFilterType<SpEvents::SectorsPreCommitted> for EventFilterProvider {
    fn filter_fn(&self, acc: AccountId32) -> impl Fn(&SpEvents::SectorsPreCommitted) -> bool {
        move |e: &SpEvents::SectorsPreCommitted| e.owner == acc
    }
}

impl EventFilterType<SpEvents::SectorsProven> for EventFilterProvider {
    fn filter_fn(&self, acc: AccountId32) -> impl Fn(&SpEvents::SectorsProven) -> bool {
        move |e: &SpEvents::SectorsProven| e.owner == acc
    }
}

impl EventFilterType<SpEvents::ValidPoStSubmitted> for EventFilterProvider {
    fn filter_fn(&self, acc: AccountId32) -> impl Fn(&SpEvents::ValidPoStSubmitted) -> bool {
        move |e: &SpEvents::ValidPoStSubmitted| e.owner == acc
    }
}

impl EventFilterType<SpEvents::FaultsDeclared> for EventFilterProvider {
    fn filter_fn(&self, acc: AccountId32) -> impl Fn(&SpEvents::FaultsDeclared) -> bool {
        move |e: &SpEvents::FaultsDeclared| e.owner == acc
    }
}

impl EventFilterType<SpEvents::FaultsRecovered> for EventFilterProvider {
    fn filter_fn(&self, acc: AccountId32) -> impl Fn(&SpEvents::FaultsRecovered) -> bool {
        move |e: &SpEvents::FaultsRecovered| e.owner == acc
    }
}

impl EventFilterType<SpEvents::SectorsTerminated> for EventFilterProvider {
    fn filter_fn(&self, acc: AccountId32) -> impl Fn(&SpEvents::SectorsTerminated) -> bool {
        move |e: &SpEvents::SectorsTerminated| e.owner == acc
    }
}
