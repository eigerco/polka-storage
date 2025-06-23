use frame_support::{
    dispatch::DispatchResult,
    ensure,
    pallet_prelude::*,
    sp_runtime::{
        traits::{CheckedAdd, Verify},
        ArithmeticError, BoundedBTreeMap,
    },
};
use frame_system::{
    ensure_signed,
    pallet_prelude::{BlockNumberFor, OriginFor},
};
use primitives::{
    deals::{ClientDealProposal, DealProposal, DealState},
    DealId,
};
use sp_runtime::{BoundedVec, DispatchError};
use sp_std::vec::Vec;

use crate::{
    deal::PublishedDeal, lock_funds, BalanceOf, BalanceTable, Config, DealsForBlock, Error, Event,
    NextDealId, Pallet, PendingProposals, Proposals, SPDealParameters, LOG_TARGET,
};

pub fn publish_storage_deals<T>(
    origin: OriginFor<T>,
    deals: BoundedVec<
        ClientDealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>, T::OffchainSignature>,
        T::MaxDeals,
    >,
) -> DispatchResult
where
    T: Config,
{
    let provider = ensure_signed(origin)?;
    ensure!(
        Pallet::<T>::is_registered_storage_provider(&provider),
        Error::<T>::StorageProviderNotRegistered
    );
    let current_block = <frame_system::Pallet<T>>::block_number();
    let (valid_deals, total_provider_lockup) =
        validate_deals::<T>(provider.clone(), deals, current_block)?;

    let mut published_deals = BoundedVec::new();

    // Lock up funds for the clients and emit events
    for deal in valid_deals.into_iter() {
        // PRE-COND: always succeeds, validated by `validate_deals`
        let client_fee: BalanceOf<T> = deal
            .total_storage_fee()
            .ok_or(Error::<T>::UnexpectedValidationError)?
            .try_into()
            .map_err(|_| Error::<T>::UnexpectedValidationError)?;

        // PRE-COND: always succeeds, validated by `validate_deals`
        lock_funds::<T>(&deal.client, client_fee)?;

        let deal_id = generate_deal_id::<T>();

        let mut deals_for_block = DealsForBlock::<T>::get(&deal.start_block);
        deals_for_block.try_insert(deal_id).map_err(|_| {
            log::error!(
                "there is not enough space to activate all of the deals at the given block {:?}",
                deal.start_block
            );
            Error::<T>::TooManyDealsPerBlock
        })?;
        DealsForBlock::<T>::insert(deal.start_block, deals_for_block);
        Proposals::<T>::insert(deal_id, deal.clone());

        // Only deposit the event after storing everything
        // force_push is ok since the bound is the same as the input one
        published_deals.force_push(PublishedDeal {
            client: deal.client,
            deal_id,
        });
    }

    // Lock up funds for the Storage Provider
    // PRE-COND: always succeeds, validated by `validate_deals`
    lock_funds::<T>(&provider, total_provider_lockup)?;

    Pallet::<T>::deposit_event(Event::<T>::DealsPublished {
        deals: published_deals,
        provider,
    });

    Ok(())
}

fn generate_deal_id<T>() -> DealId
where
    T: Config,
{
    let ret = NextDealId::<T>::get();
    let next = ret
        .checked_add(1)
        .expect("we ran out of free deal ids, not ideal");
    NextDealId::<T>::set(next);
    ret
}

fn validate_deals<T>(
    caller: T::AccountId,
    deals: BoundedVec<
        ClientDealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>, T::OffchainSignature>,
        T::MaxDeals,
    >,
    current_block: BlockNumberFor<T>,
) -> Result<
    (
        Vec<DealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>>>,
        BalanceOf<T>,
    ),
    DispatchError,
>
where
    T: Config,
{
    ensure!(deals.len() > 0, Error::<T>::NoProposalsToBePublished);

    // All deals should have the same provider, so get it once.
    let provider = deals[0].proposal.provider.clone();
    ensure!(
        caller == provider,
        Error::<T>::ProposalsPublishedByIncorrectStorageProvider
    );

    let mut total_client_lockup: BoundedBTreeMap<T::AccountId, BalanceOf<T>, T::MaxDeals> =
        BoundedBTreeMap::new();
    let mut total_provider_lockup: BalanceOf<T> = Default::default();
    let mut message_proposals: BoundedBTreeSet<T::Hash, T::MaxDeals> = BoundedBTreeSet::new();
    let mut valid_deals = Vec::new();

    for (idx, deal) in deals.into_iter().enumerate() {
        if let Err(e) = sanity_check::<T>(&deal, &provider, current_block) {
            log::error!(target: LOG_TARGET, "insane deal: idx {idx}, error: {e:?}");
            return Err(e.into());
        }

        // Safety check on deal parameters, these should be checked by the submitting SP before publishing.
        if let Some(params) = SPDealParameters::<T>::get(&provider) {
            ensure!(params.check_against_proposed_deal(&deal.proposal), {
                log::error!(
                    target: LOG_TARGET,
                    "Proposed deal does not fit within the deal bounds set by the storage provider. Set deal parameters: {:?}, proposed deal: {:?}",
                    params,
                    deal.proposal
                );
                Error::<T>::OutOfBoundsDeal
            });
        }

        // there is no Entry API in BoundedBTreeMap
        let mut client_lockup =
            if let Some(client_lockup) = total_client_lockup.get(&deal.proposal.client) {
                *client_lockup
            } else {
                Default::default()
            };
        let client_fees: BalanceOf<T> = deal
            .proposal
            .total_storage_fee()
            .unwrap()
            .try_into()
            .ok()
            .unwrap();
        client_lockup = client_lockup
            .checked_add(&client_fees)
            .ok_or(DispatchError::Arithmetic(ArithmeticError::Overflow))?;

        let client_balance = BalanceTable::<T>::get(&deal.proposal.client);
        if client_lockup > client_balance.free {
            log::error!(target: LOG_TARGET, "invalid deal: client {:?} not enough free balance {:?} < {:?} to cover deal idx: {}",
                            deal.proposal.client, client_balance.free, client_lockup, idx);
            return Err(Error::<T>::InsufficientFreeFunds.into());
        }

        // Provider collateral
        let provider_collateral: BalanceOf<T> = deal
            .proposal
            .provider_collateral()
            .ok_or(Error::<T>::UnexpectedValidationError)?
            .try_into()
            .map_err(|_| Error::<T>::UnexpectedValidationError)?;

        let mut provider_lockup = total_provider_lockup;
        provider_lockup = provider_lockup
            .checked_add(&provider_collateral)
            .ok_or(DispatchError::Arithmetic(ArithmeticError::Overflow))?;

        let provider_balance = BalanceTable::<T>::get(&deal.proposal.provider);
        if provider_lockup > provider_balance.free {
            log::error!(target: LOG_TARGET, "invalid deal: storage provider {:?} not enough free balance {:?} < {:?} to cover deal idx: {}",
                            deal.proposal.provider, provider_balance.free, provider_lockup, idx);
            return Err(Error::<T>::InsufficientFreeFunds.into());
        }

        let hash = Pallet::<T>::hash_proposal(&deal.proposal);
        let duplicate_in_state = PendingProposals::<T>::get().contains(&hash);
        let duplicate_in_message = message_proposals.contains(&hash);
        if duplicate_in_state || duplicate_in_message {
            log::error!(target: LOG_TARGET, "invalid deal: cannot publish duplicate deal idx: {}", idx);
            return Err(Error::<T>::DuplicateDeal.into());
        }
        let mut pending = PendingProposals::<T>::get();
        if let Err(e) = pending.try_insert(hash) {
            log::error!(target: LOG_TARGET, "cannot publish: too many pending deal proposals, wait for them to be expired/activated, deal idx: {}, err: {:?}", idx, e);
            return Err(Error::<T>::TooManyPendingDeals.into());
        }
        PendingProposals::<T>::set(pending);
        // PRE-COND: always succeeds, as there cannot be more deals than T::MaxDeals and this the size of the set
        message_proposals.try_insert(hash).map_err(|_| {
            DispatchError::Other("Unable to insert hash. More deals than T::MaxDeals")
        })?;
        // PRE-COND: always succeeds as there cannot be more clients than T::MaxDeals
        total_client_lockup
            .try_insert(deal.proposal.client.clone(), client_lockup)
            .map_err(|_| {
                DispatchError::Other(
                    "Unable to update client lockup. More clients than T::MaxDeals",
                )
            })?;
        total_provider_lockup = provider_lockup;

        valid_deals.push(deal.proposal)
    }

    Ok((valid_deals, total_provider_lockup))
}

fn sanity_check<T>(
    deal: &ClientDealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>, T::OffchainSignature>,
    provider: &T::AccountId,
    current_block: BlockNumberFor<T>,
) -> Result<(), Error<T>>
where
    T: Config,
{
    let encoded = Encode::encode(&deal.proposal);
    log::trace!(target: LOG_TARGET, "sanity_check: encoded proposal: {}", hex::encode(&encoded));
    validate_signature(&encoded, &deal.client_signature, &deal.proposal.client)?;

    // piece_commitment calls Commitment::from_cid_bytes -> Commitment::from_cid checking validity.
    let _ = deal.proposal.piece_commitment().map_err(|e| {
        log::error!(target: LOG_TARGET, "sanity_check: Invalid piece Cid {e}");
        Error::<T>::InvalidPieceCid
    })?;

    ensure!(
        deal.proposal.provider == *provider,
        Error::<T>::ProposalsPublishedByIncorrectStorageProvider
    );

    ensure!(
        deal.proposal.start_block < deal.proposal.end_block,
        Error::<T>::DealEndBeforeStart
    );

    ensure!(
        deal.proposal.start_block >= current_block,
        Error::<T>::DealStartExpired
    );

    ensure!(
        deal.proposal.state == DealState::Published,
        Error::<T>::DealNotPublished
    );

    let min_dur = T::MinDealDuration::get();
    let deal_duration = deal.proposal.duration();
    ensure!(deal_duration >= min_dur, {
        log::error!(target: LOG_TARGET, "deal duration too short: {deal_duration:?} < {min_dur:?}");
        Error::<T>::DealDurationOutOfBounds
    });

    let max_dur = T::MaxDealDuration::get();
    ensure!(deal_duration <= max_dur, {
        log::error!(target: LOG_TARGET, "deal_duration too long: {deal_duration:?} > {max_dur:?}");
        Error::<T>::DealDurationOutOfBounds
    });

    // TODO(@th7nder,#81,18/06/2024): figure out the minimum collateral limits
    // <https://spec.filecoin.io/#section-systems.filecoin_markets.onchain_storage_market.storage_market_actor.storage-deal-collateral>

    Ok(())
}

/// Validates the signature of the given data with the provided signer's account ID.
///
/// # Errors
///
/// This function returns a [`WrongSignature`](crate::Error::WrongClientSignatureOnProposal)
/// error if the signature is invalid or the verification process fails.
fn validate_signature<T>(
    data: &[u8],
    signature: &T::OffchainSignature,
    signer: &T::AccountId,
) -> Result<(), Error<T>>
where
    T: Config,
{
    if signature.verify(data, &signer) {
        return Ok(());
    }

    // NOTE: for security reasons modern UIs implicitly wrap the data requested to sign into
    // <Bytes></Bytes>, that's why we support both wrapped and raw versions.
    let prefix = b"<Bytes>";
    let suffix = b"</Bytes>";
    let mut wrapped = Vec::with_capacity(data.len() + prefix.len() + suffix.len());
    wrapped.extend(prefix);
    wrapped.extend(data);
    wrapped.extend(suffix);

    ensure!(
        signature.verify(&*wrapped, &signer),
        Error::<T>::WrongClientSignatureOnProposal
    );

    Ok(())
}
