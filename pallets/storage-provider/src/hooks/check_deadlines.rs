extern crate alloc;

use alloc::{collections::BTreeMap, vec::Vec};

use frame_support::{pallet_prelude::ConstU32, BoundedBTreeSet};
use frame_system::pallet_prelude::*;
use primitives::{sector::SectorNumber, PartitionNumber, MAX_SECTORS};
use sp_core::Get;

use crate::{dispatchables::process_early_terminations, Config, Event, Pallet, StorageProviders};

/// Goes through each Storage Provider and its current deadline.
///
/// If the deadline elapsed (current_block >= deadline.close_at) it checks all of the partitions and their sectors.
/// If a proof for a partition has not been submitted, all sectors in the partition are marked as faulty.
/// A deadline is checked once every [`T::WPoStProvingPeriod`]. If a Partition was marked as faulty in a deadline (deadline_idx, proving_period_idx),
/// it's rechecked in the next [`T::WPoStProvingPeriod`] in the next deadline (deadline_idx, proving_period_idx + 1).
/// `pre_commit_deposit` is slashed by 1 for each partition for each proving period a partition is faulty.
///
/// TODO:
/// - If a partition is faulty for too long [`T::FaultMaxAge`], it needs to be be terminated. (#165, #167)
/// - A proper slashing mechanism `pre_commit_deposit` and calculation. (#187)
///
/// Reference implementation:
/// * <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/state.rs#L1128>
/// * <https://github.com/filecoin-project/builtin-actors/blob/82d02e58f9ef456aeaf2a6c737562ac97b22b244/actors/miner/src/state.rs#L1192>
pub fn check_deadlines<T>(current_block: BlockNumberFor<T>)
where
    T: Config,
{
    const LOG_TARGET: &'static str = "runtime::storage_provider::check_deadlines";
    log::debug!(target: LOG_TARGET, "block: {:?}", current_block);

    // We cannot modify storage map while inside `iter_keys()` as docs say it's undefined results.
    // And we can use `alloc::Vec`, because it's bounded by StorageProviders data structure anyways.
    let storage_providers: Vec<_> = StorageProviders::<T>::iter_keys().collect();
    // TODO(@th7nder,13/08/2024): this approach is suboptimal, as it's time complexity is O(StorageProviders * PreCommitedSectors).
    // We can reduce this by indexing pre-committed sectors by BlockNumber in which they're supposed to be activated in PreCommit and remove them in ProveCommit.
    for storage_provider in storage_providers {
        log::debug!(target: LOG_TARGET, "block: {:?}, checking storage provider {:?}", current_block, storage_provider);
        let Ok(mut state) = StorageProviders::<T>::try_get(storage_provider.clone()) else {
            log::error!(target: LOG_TARGET, "missing storage provider {:?} (should have been added before)", storage_provider);
            continue;
        };

        if current_block < state.proving_period_start {
            log::debug!(target: LOG_TARGET, "skipping checking sp: {:?} on block: {:?} < proving_start {:?}, because it hasn't started yet.",
                    storage_provider, current_block, state.proving_period_start);
            continue;
        }

        let Ok(current_deadline) = state.deadline_info(
            current_block,
            T::WPoStPeriodDeadlines::get(),
            T::WPoStProvingPeriod::get(),
            T::WPoStChallengeWindow::get(),
            T::WPoStChallengeLookBack::get(),
            T::FaultDeclarationCutoff::get(),
        ) else {
            log::error!(target: LOG_TARGET, "block: {:?}, there are no deadlines for storage provider {:?}", current_block, storage_provider);
            continue;
        };

        if !current_deadline.period_started() {
            log::debug!(target: LOG_TARGET, "block: {:?}, period for deadline {:?}, sp {:?} has not yet started...", current_block, current_deadline.idx, storage_provider);
            continue;
        }

        if !current_deadline.has_elapsed() {
            log::debug!(target: LOG_TARGET,
            "block: {:?}, deadline {:?} for sp {:?} not yet elapsed. open_at: {:?} < current {:?} < close_at {:?}",
            current_block,
            current_deadline.idx, storage_provider, current_deadline.open_at, current_block, current_deadline.close_at
            );
            continue;
        }

        log::debug!(target: LOG_TARGET, "block: {:?}, checking storage provider {:?} deadline: {:?}",
            current_block,
            storage_provider,
            current_deadline.idx,
        );

        let Ok(deadline) = (&mut state.deadlines).load_deadline_mut(current_deadline.idx as usize)
        else {
            log::error!(target: LOG_TARGET, "block: {:?}, failed to get deadline {}, sp: {:?}",
                        current_block, current_deadline.idx, storage_provider);
            continue;
        };

        let mut faulty_partitions_amount = 0;
        // Create collection for fault partitions, 1 event per SP
        let mut faulty_partitions: BTreeMap<
            PartitionNumber,
            BoundedBTreeSet<SectorNumber, ConstU32<MAX_SECTORS>>,
        > = BTreeMap::new();
        for (partition_number, partition) in deadline.partitions.iter_mut() {
            if partition.sectors.len() == 0 {
                continue;
            }
            // WindowPoSt Proof was submitted for a partition.
            if deadline.partitions_posted.contains(&partition_number) {
                continue;
            }

            log::debug!(target: LOG_TARGET, "block: {:?}, going through partition: {:?}", current_block, partition);

            // Mark all Sectors in a partition as faulty
            let fault_expiration_block = current_deadline.last() + T::FaultMaxAge::get();
            let Ok(new_faults) = partition.record_faults(
                &state.sectors,
                &partition.sectors.clone(),
                fault_expiration_block,
            ) else {
                log::error!(target: LOG_TARGET, "block: {:?}, failed to mark {} sectors as faulty, deadline: {}, sp: {:?}",
                            current_block, partition.sectors.len(), current_deadline.idx, storage_provider);
                continue;
            };

            if let Err(e) = process_early_terminations::<T>(current_block, &storage_provider) {
                log::error!(target: LOG_TARGET, "could not process early terminations for {storage_provider:?}: {e:?}");
                continue;
            }
            log::info!(target: LOG_TARGET, "block: {:?}, sp: {:?}, detected partition {} with {} new faults...",
                    current_block, storage_provider, partition_number, new_faults.len());

            if new_faults.len() > 0 {
                faulty_partitions.insert(
                    *partition_number,
                    new_faults.try_into().expect(
                        "should be able to create BoundedBTreeSet due to input being bounded",
                    ),
                );
                faulty_partitions_amount += 1;
            }
        }

        // TODO(@th7nder,[#106,#187],08/08/2024): figure out slashing amounts (for continued faults, new faults).
        if faulty_partitions_amount > 0 {
            log::warn!(target: LOG_TARGET, "block: {:?}, sp: {:?}, deadline: {:?} - should have slashed {} partitions...",
                current_block,
                storage_provider,
                current_deadline.idx,
                faulty_partitions_amount,
            );

            Pallet::<T>::deposit_event(Event::PartitionsFaulty {
                owner: storage_provider.clone(),
                faulty_partitions: faulty_partitions.try_into().expect("should be able to create a BTreeMap with a MAX_PARTITIONS_PER_DEADLINE bound after iterating over a map with the same bound"),
            })
        } else if !deadline.partitions.is_empty() {
            log::info!(target: LOG_TARGET, "block: {:?}, sp: {:?}, deadline: {:?} - all proofs submitted on time.",
                current_block,
                storage_provider,
                current_deadline.idx,
            );
        }

        // Reset posted partitions, as deadline has been processed.
        // Next processing will happen in the next proving period.
        deadline.partitions_posted = BoundedBTreeSet::new();
        state
            .advance_deadline(
                current_block,
                T::WPoStPeriodDeadlines::get(),
                T::WPoStProvingPeriod::get(),
                T::WPoStChallengeWindow::get(),
                T::WPoStChallengeLookBack::get(),
                T::FaultDeclarationCutoff::get(),
            )
            .expect("Could not advance deadline");

        StorageProviders::<T>::insert(storage_provider, state);
    }
}
