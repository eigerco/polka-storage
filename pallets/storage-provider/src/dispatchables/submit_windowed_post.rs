use codec::Encode;
use frame_support::ensure;
use frame_system::{
    ensure_signed,
    pallet_prelude::{BlockNumberFor, OriginFor},
};
use primitives::{
    commitment::{CommR, Commitment},
    pallets::ProofVerification,
    proofs::PublicReplicaInfo,
    randomness::DomainSeparationTag,
};
use sp_core::Get;
use sp_runtime::{BoundedBTreeMap, BoundedVec, DispatchResult};

use super::get_randomness;
use crate::{
    deadline::DeadlineInfo, proofs::SubmitWindowedPoStParams, Config, Error, Event, Pallet,
    StorageProviders, LOG_TARGET,
};

pub fn submit_windowed_post<T>(
    origin: OriginFor<T>,
    windowed_post: SubmitWindowedPoStParams,
) -> DispatchResult
where
    T: Config,
{
    let owner = ensure_signed(origin)?;
    let current_block = <frame_system::Pallet<T>>::block_number();
    let mut sp =
        StorageProviders::<T>::try_get(&owner).map_err(|_| Error::<T>::StorageProviderNotFound)?;

    for (idx, proof) in windowed_post.proofs.iter().enumerate() {
        // Ensure proof matches the expected kind
        ensure!(proof.post_proof == sp.info.window_post_proof_type, {
            log::error!(
                target: LOG_TARGET,
                "submit_window_post: idx: {}, expected PoSt type {:?} but received {:?} instead",
                idx,
                sp.info.window_post_proof_type,
                proof.post_proof
            );
            Error::<T>::InvalidProofType
        });

        ensure!(
            proof.proof_bytes.len() <= primitives::MAX_POST_PROOF_BYTES as usize,
            {
                log::error!("submit_window_post: invalid proof size");
                Error::<T>::PoStProofInvalid
            }
        );
    }

    // If the proving period is in the future, we can't submit a proof yet
    // Related issue: https://github.com/filecoin-project/specs-actors/issues/946
    ensure!(current_block >= sp.proving_period_start, {
        log::error!(target: LOG_TARGET,
            "proving period hasn't opened yet (current_block: {:?}, proving_period_start: {:?})",
            current_block,
            sp.proving_period_start
        );
        Error::<T>::InvalidDeadlineSubmission
    });
    let current_deadline = sp
        .deadline_info(
            current_block,
            T::WPoStPeriodDeadlines::get(),
            T::WPoStProvingPeriod::get(),
            T::WPoStChallengeWindow::get(),
            T::WPoStChallengeLookBack::get(),
            T::FaultDeclarationCutoff::get(),
        )
        .map_err(|e| Error::<T>::GeneralPalletError(e))?;

    validate_deadline::<T>(&current_deadline, &windowed_post)?;

    // record sector as proven
    let all_sectors = sp.sectors.clone();
    let deadlines = sp.get_deadlines_mut();
    deadlines
        .record_proven(
            windowed_post.deadline as usize,
            &all_sectors,
            windowed_post.partitions.clone(),
        )
        .map_err(|e| Error::<T>::GeneralPalletError(e))?;

    // TODO(@th7nder,#592, 19/11/2024): handle faulty and recovered sectors, we don't take them into account now
    let deadlines = &sp.deadlines;
    let mut replicas = BoundedBTreeMap::new();
    // Take all the sectors that were assigned to all of the partitions
    for partition in &windowed_post.partitions {
        // Deadline is validated by `Self::validate_deadline`, so we're sure it can be used as an index.
        let deadline = &deadlines.due[windowed_post.deadline as usize];
        let sectors = &deadline
            .partitions
            .get(&partition)
            .ok_or(Error::<T>::InvalidPartition)?
            .sectors;
        for sector_number in sectors {
            // Sectors stored in the Storage Provider struct should be consistently stored, without breaking invariants.
            let sector_info = &sp.sectors[sector_number];
            let comm_r = Commitment::<CommR>::from_cid_bytes(&sector_info.sealed_cid)
                .expect("CommR to be validated on pre-commit");
            let _ = replicas
                .try_insert(
                    *sector_number,
                    PublicReplicaInfo {
                        comm_r: comm_r.raw(),
                    },
                )
                .map_err(|_| Error::<T>::TooManyReplicas)?;
        }
    }

    log::debug!(target: LOG_TARGET, "submit_windowed_post: index {:?} challenge {:?}, replicas: {:?}",
        current_deadline.idx,
        current_deadline.challenge,
        replicas
    );

    let entropy = owner.encode();
    // The `chain_commit_epoch` should be `current_deadline.challenge` as per:
    //
    // These issues that were filed against the original implementation:
    // * https://github.com/filecoin-project/specs-actors/issues/1094
    // * https://github.com/filecoin-project/specs-actors/issues/1376
    //
    // The Go actors have this note:
    // https://github.com/filecoin-project/specs-actors/blob/985cd0fa04578e262d68e0ef196f17df6f2434f2/actors/builtin/miner/miner_actor.go#L329-L332
    //
    // The fact that both Go and Rust actor implementations use the deadline challenge:
    // * https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/tests/miner_actor_test_wpost.rs#L99-L492
    // * https://github.com/filecoin-project/specs-actors/blob/985cd0fa04578e262d68e0ef196f17df6f2434f2/actors/test/commit_post_test.go#L204-L215
    // * https://github.com/filecoin-project/specs-actors/blob/985cd0fa04578e262d68e0ef196f17df6f2434f2/actors/test/terminate_sectors_scenario_test.go#L117-L128
    //
    // Further supported by the fact that Lotus and Curio (Lotus' replacement) don't use
    // the ChainCommitEpoch variable from the SubmitWindowedPostParams
    // * https://github.com/filecoin-project/lotus/blob/4f70204342ce83671a7a261147a18865f1618967/storage/wdpost/wdpost_run.go#L334-L338
    // * https://github.com/filecoin-project/lotus/blob/4f70204342ce83671a7a261147a18865f1618967/curiosrc/window/compute_do.go#L68-L72
    // * https://github.com/filecoin-project/curio/blob/45373f7fc0431e41f987ad348df7ae6e67beaff9/tasks/window/compute_do.go#L71-L75
    let randomness = get_randomness::<T>(
        DomainSeparationTag::WindowedPoStChallengeSeed,
        current_deadline.challenge,
        &entropy,
    )?;

    let mut proofs = BoundedVec::new();
    for proof in windowed_post.proofs {
        proofs
            .try_push(proof.proof_bytes)
            .map_err(|_| Error::<T>::TooManyProofs)?;
    }

    T::ProofVerification::verify_post(
        sp.info.window_post_proof_type,
        randomness,
        replicas,
        proofs,
    )?;

    log::debug!(target: LOG_TARGET, "submit_windowed_post: proof recorded");

    // Store new storage provider state
    StorageProviders::<T>::set(owner.clone(), Some(sp));
    Pallet::<T>::deposit_event(Event::ValidPoStSubmitted { owner });

    Ok(())
}

/// Check whether the given deadline is valid for PoSt submission.
///
/// Fails if:
/// - The given deadline is not open.
/// - There is and deadline index mismatch.
/// - The block the deadline was committed at is after challenge height.
///
/// Reference:
/// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/lib.rs#L591-L626>
fn validate_deadline<T>(
    current_deadline: &DeadlineInfo<BlockNumberFor<T>>,
    post_params: &SubmitWindowedPoStParams,
) -> Result<(), Error<T>>
where
    T: Config,
{
    // Ensure the deadline is open
    ensure!(current_deadline.is_open(), {
        log::error!(target: LOG_TARGET, "validate_deadline: {current_deadline:?}, deadline isn't open");
        Error::<T>::InvalidDeadlineSubmission
    });

    // Ensure the deadline index matches the one in the post params
    ensure!(post_params.deadline == current_deadline.idx, {
        log::error!(target: LOG_TARGET, "validate_deadline: given index does not match current index {} != {}", post_params.deadline, current_deadline.idx);
        Error::<T>::InvalidDeadlineSubmission
    });

    Ok(())
}
