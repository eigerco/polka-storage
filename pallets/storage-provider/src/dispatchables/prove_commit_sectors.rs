extern crate alloc;

use alloc::vec::Vec;

use codec::Encode;
use frame_support::{
    dispatch::DispatchResult,
    ensure,
    pallet_prelude::{CheckedSub, Zero},
};
use frame_system::{
    ensure_signed,
    pallet_prelude::{BlockNumberFor, OriginFor},
};
use primitives::{
    commitment::{CommD, CommR, Commitment},
    configs::BalanceOf,
    pallets::ProofVerification,
    proofs::derive_prover_id,
    randomness::DomainSeparationTag,
    sector::{ProveCommitSector, SectorNumber},
    MAX_POREP_PROOFS_PER_BLOCK, MAX_SEAL_PROOF_BYTES, MAX_SECTORS_PER_CALL,
};
use sp_core::{ConstU32, Get};
use sp_runtime::{BoundedVec, DispatchError};

use super::{calculate_pre_commit_deposit, get_randomness};
use crate::{
    dispatchables::activate_deals,
    sector::{ProveCommitResult, SectorOnChainInfo, SectorPreCommitOnChainInfo},
    unlock_funds, Config, Error, Event, Pallet, StorageProviders, LOG_TARGET,
};

pub fn prove_commit_sectors<T>(
    origin: OriginFor<T>,
    sectors: BoundedVec<ProveCommitSector, ConstU32<MAX_SECTORS_PER_CALL>>,
) -> DispatchResult
where
    T: Config,
{
    let owner = ensure_signed(origin)?;
    let mut sp =
        StorageProviders::<T>::try_get(&owner).map_err(|_| Error::<T>::StorageProviderNotFound)?;
    let current_block = <frame_system::Pallet<T>>::block_number();
    // Create vectors for activating all prove commits at once.
    let mut sector_deals = BoundedVec::new();
    let mut new_sectors = BoundedVec::new();
    let mut sector_numbers: BoundedVec<SectorNumber, ConstU32<MAX_SECTORS_PER_CALL>> =
        BoundedVec::new();
    let mut pre_commit_deposit_to_unlock = BalanceOf::<T>::zero();

    for sector in sectors {
        // Get pre-committed sector. This is the sector we are currently
        // proving.
        let precommit = sp
            .get_pre_committed_sector(sector.sector_number)
            .map_err(|e| Error::<T>::GeneralPalletError(e))?;
        let prove_commit_due = precommit.pre_commit_block_number + T::MaxProveCommitDuration::get();
        ensure!(current_block < prove_commit_due, {
            log::error!(target: LOG_TARGET, "prove_commit_sectors: Prove commit submitted after the deadline. {current_block:?} > {prove_commit_due:?}");
            Error::<T>::ProveCommitAfterDeadline
        });

        // Validate the proof
        validate_seal_proof::<T>(&owner, &precommit, sector.proofs)?;

        // Sector deals that will be activated after the sector is
        // successfully proven.
        sector_deals
            .try_push(precommit.into())
            .expect("Programmer error: Sector deals should fit in bound of MAX_SECTORS");
        sector_numbers
            .try_push(sector.sector_number)
            .expect("Programmer error: Sector numbers should fit in bound of MAX_SECTORS");
        // Sector that will be activated and required to be periodically
        // proven
        let new_sector = SectorOnChainInfo::from_pre_commit(precommit.info.clone(), current_block);
        new_sectors
            .try_push(new_sector)
            .expect("Programmer error: New sectors should fit in bound of MAX_SECTORS");

        pre_commit_deposit_to_unlock += calculate_pre_commit_deposit::<T>();
    }

    // Activate the deals for the sectors that will be proven. This
    // action is not applied if Err is returned from the extrinsic.
    let compute_commd = sector_deals.len() > 0;
    activate_deals::<T>(&owner, sector_deals, compute_commd)?;

    // Activate the new sectors and remove from pre-committed sectors.
    sector_numbers.iter().zip(&new_sectors).try_for_each(
        |(&sector_number, new_sector)| -> Result<(), Error<T>> {
            // Activate the new sector
            sp.activate_sector(sector_number, new_sector.clone())?;
            // Remove sector from the pre-committed map
            sp.remove_pre_committed_sector(sector_number)?;

            Ok(())
        },
    )?;

    // Assign sectors to deadlines which specify when sectors needs
    // to be proven
    sp.assign_sectors_to_deadlines(
        current_block,
        new_sectors,
        sp.info.window_post_partition_sectors,
        T::MaxPartitionsPerDeadline::get(),
        T::WPoStPeriodDeadlines::get(),
        T::WPoStProvingPeriod::get(),
        T::WPoStChallengeWindow::get(),
        T::WPoStChallengeLookBack::get(),
        T::FaultDeclarationCutoff::get(),
    )
    .map_err(|e| Error::<T>::GeneralPalletError(e))?;

    let sectors_proven = sector_numbers
        .iter()
        .map(|&sector_number| {
            // Find where the sector was placed. In worst case this goes through
            // all deadlines. It starts to look in the last partition of the
            // deadline. Usually the new sector will be there.
            let (deadline_idx, partition_number) =
                sp.deadlines
                    .due
                    .iter()
                    .enumerate()
                    .find_map(|(deadline_idx, deadline)| {
                        deadline.partitions.iter().rev().find_map(
                            |(partition_number, partition)| {
                                if partition.sectors.contains(&sector_number) {
                                    Some((deadline_idx as u64, *partition_number))
                                } else {
                                    None
                                }
                            },
                        )
                    })
                    .expect("sector should be assigned to a deadline");
            ProveCommitResult::new(sector_number, partition_number, deadline_idx)
        })
        .collect::<Vec<ProveCommitResult>>()
        .try_into()
        .expect("Programmer error: ProveCommitResult's should fit in bound of MAX_SECTORS");

    // Reduce pre commit deposit amount in state
    if let Some(pre_commit_deposits) = sp
        .pre_commit_deposits
        .checked_sub(&pre_commit_deposit_to_unlock)
    {
        sp.pre_commit_deposits = pre_commit_deposits
    } else {
        log::error!(target: LOG_TARGET, "catastrophe, failed to subtract from pre_commit_deposits {:?} - {:?} < 0", sp.pre_commit_deposits, pre_commit_deposit_to_unlock);
        return Err(Error::<T>::FailedToReturnPreCommitDeposit.into());
    };

    // Unlock pre commit deposit funds.
    unlock_funds::<T>(&owner, pre_commit_deposit_to_unlock)?;
    StorageProviders::<T>::set(owner.clone(), Some(sp));
    Pallet::<T>::deposit_event(Event::SectorsProven {
        owner,
        sectors: sectors_proven,
    });

    Ok(())
}

fn validate_seal_proof<T: Config>(
    owner: &T::AccountId,
    precommit: &SectorPreCommitOnChainInfo<BalanceOf<T>, BlockNumberFor<T>>,
    proofs: BoundedVec<
        BoundedVec<u8, ConstU32<MAX_SEAL_PROOF_BYTES>>,
        ConstU32<MAX_POREP_PROOFS_PER_BLOCK>,
    >,
) -> Result<(), DispatchError> {
    let max_proof_size = precommit.info.seal_proof.proof_size();

    // Check proof size
    if let Some(proof) = proofs.iter().filter(|p| p.len() > max_proof_size).nth(0) {
        log::error!(target: LOG_TARGET, "sector proof size {} exceeds max {}", proof.len(), max_proof_size);
        return Err(Error::<T>::InvalidProof)?;
    }

    let current_block_number = <frame_system::Pallet<T>>::block_number();

    // https://github.com/filecoin-project/builtin-actors/blob/a45fb87910bca74d62215b0d58ed90cf78b6c8ff/actors/miner/src/lib.rs#L4865
    // Check if we are too early with the proof submit
    let interactive_block_number =
        precommit.pre_commit_block_number + T::PreCommitChallengeDelay::get();

    if current_block_number < interactive_block_number {
        log::error!(target: LOG_TARGET, "too early to prove sector: current_block_number: {current_block_number:?} < interactive_block_number: {interactive_block_number:?}");
        return Err(Error::<T>::InvalidProof)?;
    }

    // Validate the data commitment
    let commd = Commitment::<CommD>::from_cid_bytes(&precommit.info.unsealed_cid[..])
            .map_err(|err| {
                log::error!(target: LOG_TARGET, err:?; "validate_seal_proof: invalid unsealed_cid {:?}", &precommit.info.unsealed_cid);
                Error::<T>::InvalidCid
            })?;

    // Validate the replica commitment
    let commr = Commitment::<CommR>::from_cid_bytes(&precommit.info.sealed_cid[..])
            .map_err(|err| {
                log::error!(target: LOG_TARGET, err:?; "validate_seal_proof: invalid sealed_cid {:?}", &precommit.info.sealed_cid);
                Error::<T>::InvalidCid
            })?;

    let entropy = owner.encode();
    let interactive_randomness = get_randomness::<T>(
        DomainSeparationTag::InteractiveSealChallengeSeed,
        interactive_block_number,
        &entropy,
    )?;

    let prover_id = derive_prover_id(owner);

    log::debug!(target: LOG_TARGET, "Performing prove commit for, seal_randomness_height {:?}, pre_commit_block: {:?}, prove_commit_block: {:?}, entropy: {}, ticket: {}, seed: {}",
            precommit.info.seal_randomness_height, precommit.pre_commit_block_number, interactive_block_number, hex::encode(entropy), hex::encode(precommit.seal_randomness), hex::encode(interactive_randomness));
    log::debug!(target: LOG_TARGET, "Prover Id: {}, Sector Number: {}", hex::encode(prover_id), precommit.info.sector_number);

    // Verify the porep proof
    T::ProofVerification::verify_porep(
        prover_id,
        precommit.info.seal_proof,
        commr.raw(),
        commd.raw(),
        precommit.info.sector_number,
        precommit.seal_randomness,
        interactive_randomness,
        proofs,
    )
}
