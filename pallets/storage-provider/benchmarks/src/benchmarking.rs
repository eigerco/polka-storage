#![cfg(feature = "runtime-benchmarks")]

use alloc::{collections::BTreeSet, vec, vec::Vec};
use core::ops::Add;

use codec::{Decode, Encode};
use frame_benchmarking::v2::*;
use frame_support::{
    assert_ok,
    pallet_prelude::{ConstU32, DispatchError, One},
    traits::{Currency, Hooks, ReservableCurrency},
    BoundedVec,
};
use frame_system::{
    pallet_prelude::{BlockNumberFor, OriginFor},
    RawOrigin,
};
use pallet_proofs::Pallet as ProofsPallet;
use pallet_storage_provider::{
    deal::parameters::{OffchainDealDurationBound, OffchainDealParameters},
    error::GeneralPalletError,
    expiration_queue::ExpirationSet,
    fault::{
        DeclareFaultsParams, DeclareFaultsRecoveredParams, FaultDeclaration, RecoveryDeclaration,
    },
    proofs::{PoStProof, SubmitWindowedPoStParams},
    sector::{TerminateSectorsParams, TerminationDeclaration},
    BalanceOf, Pallet as SpPallet, SPDealParameters,
};
use primitives::{
    deals::{ClientDealProposal, DealProposal},
    sector::{ProveCommitSector, SectorNumber, SectorPreCommitInfo},
    MAX_DEALS_PER_SECTOR, MAX_POST_PROOF_BYTES, MAX_SECTORS_PER_CALL, MAX_TERMINATIONS_PER_CALL,
    PEER_ID_MAX_BYTES,
};
use sp_core::Get;
use sp_runtime::{AccountId32, MultiSignature, MultiSigner};

use crate::{
    test_data::{benchmark_data::BenchmarkData, generate_benchmark_account},
    Config, Pallet,
};

type BoundedPeerIdBytes = BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>;

pub type DealProposalOf<T> =
    DealProposal<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

pub type ClientDealProposalOf<T> = ClientDealProposal<
    <T as frame_system::Config>::AccountId,
    pallet_storage_provider::BalanceOf<T>,
    BlockNumberFor<T>,
    MultiSignature,
>;

pub const ALICE: &'static str = "//Alice";
const CLIENT: &'static str = "//Client";
const EXISTENTIAL_DEPOSIT: u32 = 1_000_000_000;

#[benchmarks(
    where
        T: crate::Config<
            PeerId = BoundedPeerIdBytes,
            AccountId = AccountId32,
            OffchainSignature = MultiSignature,
        >,
    BalanceOf<T>: From<u32> + Encode,
    u64: TryFrom<BalanceOf<T>>,
)]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn register_storage_provider() {
        let data = BenchmarkData::<T>::load();
        let provider = data.storage_provider();
        let caller = provider.account_id;
        let peer_id = provider.peer_id;
        let window_post_proof_type = data.post_type;

        #[block]
        {
            assert_ok_sp(SpPallet::<T>::register_storage_provider(
                RawOrigin::Signed(caller.clone()).into(),
                peer_id.clone(),
                window_post_proof_type,
            ));
        }

        let state = SpPallet::<T>::storage_providers(caller).unwrap();
        assert_eq!(state.info.peer_id, peer_id);
        assert_eq!(state.info.window_post_proof_type, window_post_proof_type);
    }

    #[benchmark]
    fn add_balance() {
        let caller: T::AccountId = whitelisted_caller();

        #[block]
        {
            #[allow(deprecated)]
            SpPallet::<T>::add_balance(
                RawOrigin::Signed(caller.clone()).into(),
                EXISTENTIAL_DEPOSIT.into(),
            )
            .unwrap();
        }

        // add_balance is a no-op
    }

    #[benchmark]
    fn withdraw_balance() {
        let caller: T::AccountId = whitelisted_caller();

        #[block]
        {
            #[allow(deprecated)]
            SpPallet::<T>::withdraw_balance(
                RawOrigin::Signed(caller.clone()).into(),
                EXISTENTIAL_DEPOSIT.into(),
            )
            .unwrap();
        }

        // withdraw_balance is a no-op
    }

    /// `n`: number of submitted deals
    #[benchmark]
    fn publish_storage_deals(n: Linear<1, MAX_DEALS_PER_SECTOR>) {
        let data = BenchmarkData::<T>::load();
        let sp = data.storage_provider();
        setup_account_balance::<T>(sp.account_id.clone());
        // Register the caller as a storage provider
        pallet_storage_provider::Pallet::<T>::register_storage_provider(
            RawOrigin::Signed(sp.account_id.clone()).into(),
            sp.peer_id,
            data.post_type,
        )
        .unwrap();

        // Setup a client account
        let client = generate_benchmark_account::<T>(CLIENT);
        setup_account_balance::<T>(client.0.clone());

        let proposals = data.deal_proposals(&client, n);
        let cost: BalanceOf<T> = proposals
            .iter()
            .map(|p| p.proposal.total_storage_fee().unwrap())
            .sum::<u128>()
            .try_into()
            .unwrap_or_else(|_| panic!("failed to convert proposal fees to balance"));
        let collaterals: BalanceOf<T> = proposals
            .iter()
            .map(|p| p.proposal.provider_collateral().unwrap())
            .sum::<u128>()
            .try_into()
            .unwrap_or_else(|_| panic!("failed to convert collaterals to balance"));

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            SpPallet::<T>::publish_storage_deals(
                RawOrigin::Signed(sp.account_id.clone()).into(),
                proposals,
            )
            .unwrap();
        }

        let free_balance = T::Currency::free_balance(&sp.account_id);
        let locked_balance = T::Currency::reserved_balance(&sp.account_id);
        assert_eq!(
            free_balance,
            BalanceOf::<T>::from(2 * EXISTENTIAL_DEPOSIT) - collaterals
        );
        assert_eq!(locked_balance, collaterals);

        let free_balance = T::Currency::free_balance(&client.0);
        let locked_balance = T::Currency::reserved_balance(&client.0);
        assert_eq!(
            free_balance,
            BalanceOf::<T>::from(2 * EXISTENTIAL_DEPOSIT) - cost
        );
        assert_eq!(locked_balance, cost.into());
    }

    /// `n`: number of submitted deals
    #[benchmark]
    fn settle_deal_payments(n: Linear<1, MAX_DEALS_PER_SECTOR>) {
        let data = BenchmarkData::<T>::load();
        let sp = data.storage_provider();
        setup_account_balance::<T>(sp.account_id.clone());
        // Register the caller as a storage provider
        pallet_storage_provider::Pallet::<T>::register_storage_provider(
            RawOrigin::Signed(sp.account_id.clone()).into(),
            sp.peer_id,
            data.post_type,
        )
        .unwrap();

        // Setup a client account
        let client = generate_benchmark_account::<T>(CLIENT);
        setup_account_balance::<T>(client.0.clone());

        let proposals = data.deal_proposals(&client, n);
        let cost: u32 = proposals
            .iter()
            .map(|p| p.proposal.total_storage_fee().unwrap())
            .sum::<u128>()
            .try_into()
            .unwrap();

        let storage_provider: OriginFor<T> = RawOrigin::Signed(sp.account_id.clone()).into();
        SpPallet::<T>::publish_storage_deals(storage_provider.clone(), proposals.clone()).unwrap();

        // Run to pre-commit period
        run_to_block::<T>(data.timeline.pre_commit_sectors().0);

        let pre_commit_infos = data.pre_commit_sectors(n);
        SpPallet::<T>::pre_commit_sectors(storage_provider.clone(), pre_commit_infos.clone())
            .unwrap();

        assert_ok!(ProofsPallet::<T>::set_porep_verifying_key(
            RawOrigin::Root.into(),
            data.seal_proof,
            data.porep_verifying_key.to_vec(),
        ));

        // Run to after pre-commit delay
        run_to_block::<T>(data.timeline.prove_commit_sectors().0);

        let proofs = data.prove_commit_sectors(n);
        SpPallet::<T>::prove_commit_sectors(storage_provider.clone(), proofs).unwrap();

        let expiration_block = pre_commit_infos
            .iter()
            .map(|info| info.expiration)
            .max()
            .unwrap();
        run_to_block::<T>(
            expiration_block
                .try_into()
                .unwrap_or_else(|_| panic!("failed to convert expiration block to block number")),
        );

        let deal_ids = pre_commit_infos
            .iter()
            .flat_map(|info| info.deal_ids.iter())
            .copied()
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            SpPallet::<T>::settle_deal_payments(storage_provider.clone(), deal_ids).unwrap();
        }

        assert_eq!(
            SpPallet::<T>::free(&sp.account_id).unwrap(),
            (2 * EXISTENTIAL_DEPOSIT + cost).into()
        );
        assert_eq!(SpPallet::<T>::locked(&sp.account_id).unwrap(), 0u32.into());

        assert_eq!(
            SpPallet::<T>::free(&client.0).unwrap(),
            (2 * EXISTENTIAL_DEPOSIT - cost).into()
        );
        assert_eq!(SpPallet::<T>::locked(&client.0).unwrap(), 0u32.into());
    }

    /// `n` == 1: Publish
    /// `n` == 2: Publish & Replace
    #[benchmark]
    fn publish_deal_parameters(n: Linear<1, 2>) {
        let data = BenchmarkData::<T>::load();
        let sp = data.storage_provider();
        setup_account_balance::<T>(sp.account_id.clone());
        // Register the caller as a storage provider
        pallet_storage_provider::Pallet::<T>::register_storage_provider(
            RawOrigin::Signed(sp.account_id.clone()).into(),
            sp.peer_id,
            data.post_type,
        )
        .unwrap();

        let offchain_deal_parameters: OffchainDealParameters<BalanceOf<T>, BlockNumberFor<T>> =
            OffchainDealParameters {
                minimum_price_per_block: 1u32.into(),
                deal_duration: OffchainDealDurationBound {
                    lower: Some(60u32.into()),
                    upper: Some(100u32.into()),
                },
            };
        let storage_provider: OriginFor<T> = RawOrigin::Signed(sp.account_id.clone()).into();

        if n == 2 {
            SpPallet::<T>::publish_deal_parameters(
                storage_provider.clone(),
                offchain_deal_parameters.clone(),
            )
            .unwrap();
        }

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            SpPallet::<T>::publish_deal_parameters(
                storage_provider.clone(),
                offchain_deal_parameters.clone(),
            )
            .unwrap();
        }
        let deal_parameters = offchain_deal_parameters
            .clone()
            .validate(T::MinDealDuration::get(), T::MaxDealDuration::get())
            .expect("Seamless conversion");
        assert_eq!(
            SPDealParameters::<T>::get(&sp.account_id),
            Some(deal_parameters)
        );
    }

    #[benchmark]
    fn remove_deal_parameters() {
        let data = BenchmarkData::<T>::load();
        let sp = data.storage_provider();
        setup_account_balance::<T>(sp.account_id.clone());
        // Register the caller as a storage provider
        pallet_storage_provider::Pallet::<T>::register_storage_provider(
            RawOrigin::Signed(sp.account_id.clone()).into(),
            sp.peer_id,
            data.post_type,
        )
        .unwrap();

        let deal_parameters: OffchainDealParameters<BalanceOf<T>, BlockNumberFor<T>> =
            OffchainDealParameters {
                minimum_price_per_block: 1u32.into(),
                deal_duration: OffchainDealDurationBound {
                    lower: Some(60u32.into()),
                    upper: Some(100u32.into()),
                },
            };
        let storage_provider: OriginFor<T> = RawOrigin::Signed(sp.account_id.clone()).into();

        SpPallet::<T>::publish_deal_parameters(storage_provider.clone(), deal_parameters).unwrap();

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            SpPallet::<T>::remove_deal_parameters(storage_provider.clone()).unwrap();
        }

        assert_eq!(SPDealParameters::<T>::get(&sp.account_id), None);
    }

    impl_benchmark_test_suite! {
        Pallet,
        crate::test::new_test_ext(),
        crate::mock::Test,
    }

    #[benchmark]
    fn pre_commit_sectors() {
        let (sp_id, _, sectors) = prepare_pre_commit_sectors::<T>(1);

        #[block]
        {
            assert_ok_sp(SpPallet::<T>::pre_commit_sectors(
                RawOrigin::Signed(sp_id.clone()).into(),
                sectors,
            ));
        }

        check_pre_commit_sectors::<T>(1, sp_id);
    }

    #[benchmark]
    fn prove_commit_sectors() {
        let (sp_id, prove_sectors, total_fee) = prepare_prove_commit_sectors::<T>(1);

        #[block]
        {
            assert_ok_sp(SpPallet::<T>::prove_commit_sectors(
                RawOrigin::Signed(sp_id.clone()).into(),
                prove_sectors.clone(),
            ));
        }

        check_prove_commit_sectors::<T>(sp_id, prove_sectors, total_fee);
    }

    /// `n`: number of submitted faulty sectors
    // TODO(@Jinxit,#827,15/04/2025): Use `n: Linear<1, DECLARATIONS_MAX * MAX_TERMINATIONS_PER_CALL>`
    // when we have more proven sectors to use.
    #[benchmark]
    fn declare_faults() {
        let (sp_id, faults) = prepare_declare_faults::<T>(1);

        #[block]
        {
            assert_ok_sp(SpPallet::<T>::declare_faults(
                RawOrigin::Signed(sp_id.clone()).into(),
                faults.clone(),
            ));
        }

        check_declare_faults::<T>(faults);
    }

    /// `n`: number of submitted faulty sectors
    // TODO(@Jinxit,#827,16/04/2025): Use `n: Linear<1, DECLARATIONS_MAX * MAX_TERMINATIONS_PER_CALL>`
    // when we have more proven sectors to use.
    #[benchmark]
    fn declare_faults_recovered() {
        let (sp_id, faults) = prepare_declare_faults_recovered::<T>(1);

        #[block]
        {
            assert_ok_sp(SpPallet::<T>::declare_faults_recovered(
                RawOrigin::Signed(sp_id.clone()).into(),
                faults.clone(),
            ));
        }

        check_declare_faults_recovered::<T>(faults);
    }

    /// `n`: number of submitted faulty sectors
    // TODO(@Jinxit,#827,16/04/2025): Use `n: Linear<1, DECLARATIONS_MAX * MAX_TERMINATIONS_PER_CALL>`
    // when we have more proven sectors to use.
    #[benchmark]
    fn terminate_sectors() {
        let (sp_id, sectors) = prepare_terminate_sectors::<T>(1);

        #[block]
        {
            assert_ok_sp(SpPallet::<T>::terminate_sectors(
                RawOrigin::Signed(sp_id.clone()).into(),
                sectors.clone(),
            ));
        }

        check_terminate_sectors::<T>(sectors);
    }

    /// `n`: number of submitted proofs
    // TODO(@Jinxit,#827,16/04/2025): Use `n: Linear<1, MAX_POST_PROOFS_PER_BLOCK>`
    // when we have more proofs to use.
    #[benchmark]
    fn submit_windowed_post() {
        let (sp_id, windowed_post) = prepare_submit_windowed_post::<T>(1);

        #[block]
        {
            assert_ok_sp(SpPallet::<T>::submit_windowed_post(
                RawOrigin::Signed(sp_id.clone()).into(),
                windowed_post.clone(),
            ));
        }

        check_submit_windowed_post::<T>(windowed_post);
    }

    impl_benchmark_test_suite!(Pallet, crate::test::new_test_ext(), crate::mock::Test);
}

fn prepare_pre_commit_sectors<T>(
    n: u32,
) -> (
    AccountId32,
    BoundedVec<ClientDealProposalOf<T>, T::MaxDeals>,
    BoundedVec<SectorPreCommitInfo<BlockNumberFor<T>>, ConstU32<{ MAX_SECTORS_PER_CALL }>>,
)
where
    T: crate::Config<
        PeerId = BoundedPeerIdBytes,
        AccountId = AccountId32,
        OffchainSignature = MultiSignature,
    >,
    BalanceOf<T>: From<u32> + Encode,
{
    let data = BenchmarkData::<T>::load();
    let alice = create_account_with_balance::<T>(ALICE, EXISTENTIAL_DEPOSIT * 2);
    let sp = data.storage_provider();

    pallet_balances::Pallet::<T>::make_free_balance_be(
        &sp.account_id,
        (EXISTENTIAL_DEPOSIT * 2).into(),
    );

    run_to_block::<T>(data.timeline.register_storage_provider().0);

    assert_ok_sp(SpPallet::<T>::register_storage_provider(
        RawOrigin::Signed(sp.account_id.clone()).into(),
        sp.peer_id.clone(),
        data.post_type,
    ));

    let proposals = data.deal_proposals(&alice, n);

    run_to_block::<T>(data.timeline.publish_storage_deals().0);

    assert_ok!(SpPallet::<T>::publish_storage_deals(
        RawOrigin::Signed(sp.account_id.clone()).into(),
        proposals.clone(),
    ));

    // Run to pre-commit period
    run_to_block::<T>(data.timeline.pre_commit_sectors().0);

    let sectors = data.pre_commit_sectors(n);
    (sp.account_id, proposals, sectors)
}

fn check_pre_commit_sectors<T>(n: u32, sp_id: AccountId32)
where
    T: crate::Config<AccountId = AccountId32>,
{
    let state = SpPallet::<T>::storage_providers(sp_id).unwrap();
    let balance: BalanceOf<T> = n.into();
    assert_eq!(state.pre_commit_deposits, balance);
}

fn prepare_prove_commit_sectors<T>(
    n: u32,
) -> (
    AccountId32,
    BoundedVec<ProveCommitSector, ConstU32<MAX_SECTORS_PER_CALL>>,
    u32,
)
where
    T: crate::Config<
        PeerId = BoundedPeerIdBytes,
        AccountId = AccountId32,
        OffchainSignature = MultiSignature,
    >,
    BlockNumberFor<T>: Add,
{
    let data = BenchmarkData::<T>::load();
    let (sp_id, proposals, sector_pre_commits) = prepare_pre_commit_sectors::<T>(n);

    let prove_sectors = data.prove_commit_sectors(n);
    let total_collateral: u64 = proposals
        .iter()
        .map(|p| {
            u64::try_from(p.proposal.provider_collateral().unwrap())
                .unwrap_or_else(|_| panic!("failed to convert provider_collateral to u64"))
        })
        .sum();

    assert_ok_sp(SpPallet::<T>::pre_commit_sectors(
        RawOrigin::Signed(sp_id.clone()).into(),
        sector_pre_commits,
    ));

    assert_ok!(ProofsPallet::<T>::set_porep_verifying_key(
        RawOrigin::Root.into(),
        data.seal_proof,
        data.porep_verifying_key.to_vec(),
    ));

    run_to_block::<T>(data.timeline.prove_commit_sectors().0);

    (sp_id, prove_sectors, total_collateral as u32)
}

fn check_prove_commit_sectors<T>(
    sp_id: T::AccountId,
    prove_sectors: BoundedVec<ProveCommitSector, ConstU32<MAX_SECTORS_PER_CALL>>,
    total_fee: u32,
) where
    T: crate::Config<
        PeerId = BoundedPeerIdBytes,
        AccountId = AccountId32,
        OffchainSignature = MultiSignature,
    >,
    BlockNumberFor<T>: Add,
    u64: TryFrom<BalanceOf<T>>,
{
    assert_eq!(
        SpPallet::<T>::free(&sp_id),
        Some((2 * EXISTENTIAL_DEPOSIT - total_fee).into()),
    );

    let state = SpPallet::<T>::storage_providers(sp_id).unwrap();
    let balance: BalanceOf<T> = 0_u32.into();
    assert_eq!(state.pre_commit_deposits, balance);

    // check that the sector has been activated
    assert!(!state.sectors.is_empty());
    for sector in &prove_sectors {
        assert!(state.sectors.contains_key(&sector.sector_number));
    }
    // always assigns first deadline and first partition, probably will fail when we change deadline
    // calculation algo.
    let mut sectors = vec![];
    for deadline in &state.deadlines.due {
        for (_, partition) in &deadline.partitions {
            for sector in &partition.sectors {
                sectors.push(sector);
            }
        }
    }
    assert_eq!(sectors.len(), prove_sectors.len(), "state: {state:#?}");
    assert!(prove_sectors.iter().all(|sector| {
        let sector_number = sector.sector_number;
        sectors.contains(&&sector_number)
    }));
}

fn prepare_declare_faults<T>(n: u32) -> (AccountId32, DeclareFaultsParams)
where
    T: crate::Config<
        PeerId = BoundedPeerIdBytes,
        AccountId = AccountId32,
        OffchainSignature = MultiSignature,
    >,
    BlockNumberFor<T>: Add,
    u64: TryFrom<BalanceOf<T>>,
{
    let (sp_id, prove_sectors, _) = prepare_prove_commit_sectors::<T>(n);

    assert_ok_sp(SpPallet::<T>::prove_commit_sectors(
        RawOrigin::Signed(sp_id.clone()).into(),
        prove_sectors.clone(),
    ));

    let faults = prove_sectors
        .chunks(MAX_TERMINATIONS_PER_CALL as usize)
        .into_iter()
        .enumerate()
        .map(|(i, sectors)| FaultDeclaration {
            deadline: i as u64,
            partition: i as u32,
            sectors: sectors
                .into_iter()
                .map(|s| s.sector_number)
                .collect::<BTreeSet<_>>()
                .try_into()
                .unwrap(),
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    (sp_id, DeclareFaultsParams { faults })
}

fn check_declare_faults<T>(faults: DeclareFaultsParams)
where
    T: crate::Config<AccountId = AccountId32>,
{
    let data = BenchmarkData::<T>::load();
    let sp = data.storage_provider();

    let state = SpPallet::<T>::storage_providers(sp.account_id.clone()).unwrap();

    let faults_sectors = faults
        .faults
        .iter()
        .flat_map(|f| f.sectors.iter())
        .collect::<Vec<_>>();
    let deadline_sectors = state
        .deadlines
        .due
        .iter()
        .flat_map(|dl| dl.partitions.iter())
        .flat_map(|(_, p)| p.faults.iter())
        .collect::<Vec<_>>();

    assert_eq!(faults_sectors, deadline_sectors);
}

fn prepare_declare_faults_recovered<T>(n: u32) -> (AccountId32, DeclareFaultsRecoveredParams)
where
    T: crate::Config<
        PeerId = BoundedPeerIdBytes,
        AccountId = AccountId32,
        OffchainSignature = MultiSignature,
    >,
    BlockNumberFor<T>: Add,
    u64: TryFrom<BalanceOf<T>>,
{
    let (sp_id, faults) = prepare_declare_faults::<T>(n);

    assert_ok_sp(SpPallet::<T>::declare_faults(
        RawOrigin::Signed(sp_id.clone()).into(),
        faults.clone(),
    ));

    let recoveries = faults
        .faults
        .into_iter()
        .map(|fault| RecoveryDeclaration {
            deadline: fault.deadline,
            partition: fault.partition,
            sectors: fault.sectors,
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    (sp_id, DeclareFaultsRecoveredParams { recoveries })
}

fn check_declare_faults_recovered<T>(recoveries: DeclareFaultsRecoveredParams)
where
    T: crate::Config<AccountId = AccountId32>,
{
    let data = BenchmarkData::<T>::load();
    let sp = data.storage_provider();

    let state = SpPallet::<T>::storage_providers(sp.account_id.clone()).unwrap();

    let recoveries_sectors = recoveries
        .recoveries
        .iter()
        .flat_map(|f| f.sectors.iter())
        .collect::<Vec<_>>();
    let deadline_sectors = state
        .deadlines
        .due
        .iter()
        .flat_map(|dl| dl.partitions.iter())
        .flat_map(|(_, p)| p.recoveries.iter())
        .collect::<Vec<_>>();

    assert_eq!(recoveries_sectors, deadline_sectors);
}

fn prepare_terminate_sectors<T>(n: u32) -> (AccountId32, TerminateSectorsParams)
where
    T: crate::Config<
        PeerId = BoundedPeerIdBytes,
        AccountId = AccountId32,
        OffchainSignature = MultiSignature,
    >,
    BlockNumberFor<T>: Add,
    u64: TryFrom<BalanceOf<T>>,
{
    let (sp_id, prove_sectors, _) = prepare_prove_commit_sectors::<T>(n);

    assert_ok_sp(SpPallet::<T>::prove_commit_sectors(
        RawOrigin::Signed(sp_id.clone()).into(),
        prove_sectors.clone(),
    ));

    let terminations = prove_sectors
        .chunks(MAX_TERMINATIONS_PER_CALL as usize)
        .into_iter()
        .enumerate()
        .map(|(i, sectors)| TerminationDeclaration {
            deadline: i as u64,
            partition: i as u32,
            sectors: sectors
                .into_iter()
                .map(|s| s.sector_number)
                .collect::<BTreeSet<_>>()
                .try_into()
                .unwrap(),
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();

    (sp_id, TerminateSectorsParams { terminations })
}

fn check_terminate_sectors<T>(terminations: TerminateSectorsParams)
where
    T: crate::Config<AccountId = AccountId32>,
{
    let data = BenchmarkData::<T>::load();
    let sp = data.storage_provider();

    let state = SpPallet::<T>::storage_providers(sp.account_id.clone()).unwrap();

    let terminated_sectors = terminations
        .terminations
        .iter()
        .flat_map(|f| f.sectors.iter())
        .collect::<Vec<_>>();
    let deadline_sectors = state
        .deadlines
        .due
        .iter()
        .flat_map(|dl| dl.partitions.iter())
        .flat_map(|(_, p)| p.terminated.iter())
        .collect::<Vec<_>>();

    assert_eq!(terminated_sectors, deadline_sectors);

    let expirations: Vec<ExpirationSet> = state
        .deadlines
        .due
        .iter()
        .flat_map(|dl| dl.partitions.iter())
        .flat_map(|(_, p)| p.expirations.map.values())
        .cloned()
        .collect();
    assert_eq!(expirations, Vec::new())
}

fn prepare_submit_windowed_post<T>(n: u32) -> (AccountId32, SubmitWindowedPoStParams)
where
    T: crate::Config<
        PeerId = BoundedPeerIdBytes,
        AccountId = AccountId32,
        OffchainSignature = MultiSignature,
    >,
    BlockNumberFor<T>: Add,
    u64: TryFrom<BalanceOf<T>>,
{
    let data = BenchmarkData::<T>::load();
    let (sp_id, prove_sectors, _) = prepare_prove_commit_sectors::<T>(n);

    assert_ok_sp(SpPallet::<T>::prove_commit_sectors(
        RawOrigin::Signed(sp_id.clone()).into(),
        prove_sectors.clone(),
    ));

    assert_ok!(ProofsPallet::<T>::set_post_verifying_key(
        RawOrigin::Root.into(),
        data.post_type,
        data.post_verifying_key.to_vec(),
    ));

    let sector = data.sectors.get(0).unwrap();
    let proofs = sector
        .post_proof
        .chunks_exact(MAX_POST_PROOF_BYTES as usize)
        .map(|chunk| PoStProof {
            post_proof: data.post_type,
            proof_bytes: chunk.to_vec().try_into().unwrap(),
        })
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();

    run_to_block::<T>(data.timeline.submit_windowed_post().0);

    (
        sp_id,
        SubmitWindowedPoStParams {
            deadline: data.timeline.deadline_index().into(),
            partitions: vec![0].try_into().unwrap(),
            proofs,
        },
    )
}

fn check_submit_windowed_post<T>(windowed_post: SubmitWindowedPoStParams)
where
    T: crate::Config<AccountId = AccountId32>,
{
    let data = BenchmarkData::<T>::load();
    let sp = data.storage_provider();
    let state = SpPallet::<T>::storage_providers(sp.account_id).unwrap();

    let partitions_posted: Vec<u32> = state
        .deadlines
        .due
        .iter()
        .flat_map(|dl| dl.partitions_posted.iter().copied())
        .collect();

    let unproven: Vec<SectorNumber> = state
        .deadlines
        .due
        .iter()
        .flat_map(|dl| {
            dl.partitions
                .iter()
                .flat_map(|(_, p)| p.unproven.iter().copied())
        })
        .collect();

    assert_eq!(partitions_posted, windowed_post.partitions.to_vec());
    assert_eq!(unproven, vec![]);
}

/// Run until a particular block.
///
/// Stolen from https://github.com/paritytech/polkadot-sdk/blob/1bc6ca606438a65c927f14be3f36634ca0e58e8f/substrate/frame/identity/src/benchmarking.rs#L48
fn run_to_block<T: Config>(n: frame_system::pallet_prelude::BlockNumberFor<T>) {
    while frame_system::Pallet::<T>::block_number() < n {
        crate::Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());
        frame_system::Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());

        frame_system::Pallet::<T>::set_block_number(
            frame_system::Pallet::<T>::block_number() + One::one(),
        );

        frame_system::Pallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
        ProofsPallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
        crate::Pallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
    }
}

fn create_account_with_balance<T>(name: &'static str, balance: u32) -> (AccountId32, MultiSigner)
where
    T: crate::Config<AccountId = AccountId32>,
{
    let account = generate_benchmark_account::<T>(name);
    pallet_balances::Pallet::<T>::make_free_balance_be(&account.0, balance.into());
    account
}

fn setup_account_balance<T>(account: T::AccountId)
where
    T: crate::Config,
    T: pallet_balances::Config,
{
    pallet_balances::Pallet::<T>::make_free_balance_be(&account, (EXISTENTIAL_DEPOSIT * 2).into());
}

/// Asserts that the result was Ok, and additionally decodes the error if it was a GeneralPalletError
/// to get the inner error as a user-friendly string.
#[track_caller]
fn assert_ok_sp(res: Result<(), DispatchError>) {
    if let Err(sp_runtime::DispatchError::Module(module)) = &res {
        if let Some("GeneralPalletError") = module.message {
            let variant = GeneralPalletError::decode(&mut [module.error[1]].as_slice()).unwrap();
            panic!("{variant:#?}");
        }
    }
    assert_ok!(res);
}
