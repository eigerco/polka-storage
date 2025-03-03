#![cfg(feature = "runtime-benchmarks")]

use alloc::vec;

use frame_benchmarking::v2::*;
use frame_support::{
    assert_ok,
    pallet_prelude::{ConstU32, One},
    traits::{Currency, OnFinalize, OnInitialize},
    BoundedVec,
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use pallet_market::Pallet as MarketPallet;
use pallet_proofs::Pallet as ProofsPallet;
use pallet_storage_provider::{
    pallet::{BalanceOf, Call},
    Pallet as SpPallet,
};
use primitives::{
    proofs::RegisteredPoStProof,
    sector::{ProveCommitSector, SectorNumber, SectorPreCommitInfo, SectorSize},
    MAX_SECTORS_PER_CALL, PEER_ID_MAX_BYTES,
};
use sp_runtime::{AccountId32, MultiSignature, MultiSigner};

use crate::{
    accounts::{generate_benchmark_account, ALICE},
    data::{get_benchmark_data, StorageProvider},
    Config, Pallet,
};
type BoundedPeerIdBytes = BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>;

const EXISTENTIAL_DEPOSIT: u32 = 1_000_000_000;

const BENCHMARK_SECTOR_SIZE: SectorSize = SectorSize::_8MiB;

#[benchmarks(
    where
        T: crate::Config<
            PeerId = BoundedPeerIdBytes,
            AccountId = AccountId32,
            OffchainSignature = MultiSignature,
        >,
    BlockNumberFor<T>: From<u64> + Into<u64>,
    u64: TryFrom<pallet_market::BalanceOf<T>>,
)]
mod benchmarks {

    use super::*;

    #[benchmark]
    fn register_storage_provider() {
        let caller: T::AccountId = whitelisted_caller();
        let peer_id: T::PeerId = b"peer_id".to_vec().try_into().unwrap();
        let window_post_proof_type = RegisteredPoStProof::StackedDRGWindow8MiBV1;

        #[extrinsic_call]
        _(
            RawOrigin::Signed(caller.clone()),
            peer_id.clone(),
            window_post_proof_type,
        );

        let state = SpPallet::<T>::storage_providers(caller).unwrap();
        assert_eq!(state.info.peer_id, peer_id);
        assert_eq!(state.info.window_post_proof_type, window_post_proof_type);
    }

    #[benchmark]
    fn pre_commit_sectors(n: Linear<1, MAX_SECTORS_PER_CALL>) {
        let (sp_id, sectors) = prepare_pre_commit_sectors::<T>(n);

        #[extrinsic_call]
        _(RawOrigin::Signed(sp_id.clone()), sectors);

        check_pre_commit_sectors::<T>(n, sp_id);
    }

    #[benchmark]
    fn prove_commit_sectors(n: Linear<1, MAX_SECTORS_PER_CALL>) {
        let (sp_id, prove_sectors, total_fee) = prepare_prove_commit_sectors::<T>(n);

        #[extrinsic_call]
        _(RawOrigin::Signed(sp_id.clone()), prove_sectors.clone());

        check_prove_commit_sectors::<T>(sp_id, prove_sectors, total_fee, n);
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}

fn prepare_pre_commit_sectors<T>(
    n: u32,
) -> (
    AccountId32,
    BoundedVec<SectorPreCommitInfo<BlockNumberFor<T>>, ConstU32<{ MAX_SECTORS_PER_CALL }>>,
)
where
    T: crate::Config<
        PeerId = BoundedPeerIdBytes,
        AccountId = AccountId32,
        OffchainSignature = MultiSignature,
    >,
    BlockNumberFor<T>: From<u64> + Into<u64>,
{
    let data = get_benchmark_data::<T>(BENCHMARK_SECTOR_SIZE);
    let alice = create_account_with_balance::<T>(ALICE, EXISTENTIAL_DEPOSIT * 2);
    let sp = data.provider();
    create_and_register_storage_provider_with_balance::<T>(&sp, EXISTENTIAL_DEPOSIT * 2);

    assert_ok!(MarketPallet::<T>::add_balance(
        RawOrigin::Signed(sp.account_id.clone()).into(),
        EXISTENTIAL_DEPOSIT.into()
    ));

    assert_ok!(MarketPallet::<T>::add_balance(
        RawOrigin::Signed(alice.0.clone()).into(),
        EXISTENTIAL_DEPOSIT.into()
    ));

    let proposals = data.deal_proposals(&alice, n);

    assert_ok!(MarketPallet::<T>::publish_storage_deals(
        RawOrigin::Signed(sp.account_id.clone()).into(),
        proposals,
    ));

    let sectors = data.pre_commit_sectors(n);
    (sp.account_id, sectors)
}

fn check_pre_commit_sectors<T>(n: u32, sp_id: AccountId32)
where
    T: crate::Config<AccountId = AccountId32>,
    BlockNumberFor<T>: From<u64> + Into<u64>,
{
    let state = SpPallet::<T>::storage_providers(sp_id).unwrap();
    let balance: BalanceOf<T> = (n * 1_u32).into();
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
    BlockNumberFor<T>: From<u64> + Into<u64>,
    u64: TryFrom<pallet_market::BalanceOf<T>>,
{
    let data = get_benchmark_data::<T>(BENCHMARK_SECTOR_SIZE);
    let alice = create_account_with_balance::<T>(ALICE, EXISTENTIAL_DEPOSIT * 2);
    let sp = data.provider();
    create_and_register_storage_provider_with_balance::<T>(&sp, EXISTENTIAL_DEPOSIT * 2);

    assert_ok!(MarketPallet::<T>::add_balance(
        RawOrigin::Signed(sp.account_id.clone()).into(),
        EXISTENTIAL_DEPOSIT.into()
    ));

    assert_ok!(MarketPallet::<T>::add_balance(
        RawOrigin::Signed(alice.0.clone()).into(),
        EXISTENTIAL_DEPOSIT.into()
    ));

    let proposals = data.deal_proposals(&alice, n);
    let sectors_pre_commits = data.pre_commit_sectors(n);
    let prove_sectors = data.prove_commit_sectors(n);
    let total_collateral: u64 = proposals
        .iter()
        .map(|p| {
            u64::try_from(p.proposal.provider_collateral)
                .unwrap_or_else(|_| panic!("failed to convert provider_collateral to u64"))
        })
        .sum();

    assert_ok!(MarketPallet::<T>::publish_storage_deals(
        RawOrigin::Signed(sp.account_id.clone()).into(),
        proposals,
    ));

    // Run to 1 to get VRF randomness

    // Run to 5 to enter pre-commit period
    run_to_block::<T>(5.into());

    assert_ok!(SpPallet::<T>::pre_commit_sectors(
        RawOrigin::Signed(sp.account_id.clone()).into(),
        sectors_pre_commits
    ));

    assert_ok!(ProofsPallet::<T>::set_porep_verifying_key(
        RawOrigin::Signed(sp.account_id.clone()).into(),
        data.seal_proof,
        data.verifying_key.to_vec(),
    ));

    run_to_block::<T>(15.into());

    (sp.account_id, prove_sectors, total_collateral as u32)
}

fn check_prove_commit_sectors<T: crate::Config>(
    sp_id: T::AccountId,
    prove_sectors: BoundedVec<ProveCommitSector, ConstU32<MAX_SECTORS_PER_CALL>>,
    total_fee: u32,
    n: u32,
) where
    T: crate::Config,
{
    assert_eq!(
        MarketPallet::<T>::free(&sp_id),
        Some((EXISTENTIAL_DEPOSIT - total_fee).into()),
        "n == {n}"
    );

    let state = SpPallet::<T>::storage_providers(sp_id).unwrap();
    let balance: BalanceOf<T> = 0_u32.into();
    assert_eq!(state.pre_commit_deposits, balance, "n == {n}");

    // check that the sector has been activated
    assert!(!state.sectors.is_empty());
    for sector in prove_sectors {
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
    assert_eq!(sectors.len(), n as usize, "n == {n}, state: {state:#?}");
    assert!((0..n).all(|i| {
        let sector_number = SectorNumber::new(i).unwrap();
        sectors.contains(&&sector_number)
    }));
}

/// Run until a particular block.
///
/// Stolen from https://github.com/paritytech/polkadot-sdk/blob/1bc6ca606438a65c927f14be3f36634ca0e58e8f/substrate/frame/identity/src/benchmarking.rs#L48
fn run_to_block<T: Config>(n: frame_system::pallet_prelude::BlockNumberFor<T>) {
    while frame_system::Pallet::<T>::block_number() < n {
        //crate::Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());
        frame_system::Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());
        frame_system::Pallet::<T>::set_block_number(
            frame_system::Pallet::<T>::block_number() + One::one(),
        );
        frame_system::Pallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
        crate::Pallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
    }
}

fn create_account_with_balance<T>(name: &'static str, balance: u32) -> (AccountId32, MultiSigner)
where
    T: crate::Config<AccountId = AccountId32>,
{
    let account = generate_benchmark_account(name);
    pallet_balances::Pallet::<T>::make_free_balance_be(&account.0, balance.into());
    account
}

fn create_and_register_storage_provider_with_balance<T>(provider: &StorageProvider<T>, balance: u32)
where
    T: crate::Config<AccountId = AccountId32, PeerId = BoundedPeerIdBytes>,
{
    pallet_balances::Pallet::<T>::make_free_balance_be(&provider.account_id, balance.into());

    assert_ok!(SpPallet::<T>::register_storage_provider(
        RawOrigin::Signed(provider.account_id.clone()).into(),
        provider.peer_id.clone(),
        RegisteredPoStProof::StackedDRGWindow8MiBV1,
    ));
}
