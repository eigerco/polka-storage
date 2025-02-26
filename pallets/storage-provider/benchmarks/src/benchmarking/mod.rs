#![cfg(feature = "runtime-benchmarks")]

mod accounts;
mod deal_proposals;

use alloc::{vec, vec::Vec};
use core::cmp::min;
use alloc::str::FromStr;

use accounts::{generate_benchmark_account, ALICE, STORAGE_PROVIDER};
use cid::Cid;
use deal_proposals::{sign_proposal, ClientDealProposalOf, DealProposalOf};
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, pallet_prelude::ConstU32, traits::Currency, BoundedVec};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use pallet_market::{DealState, Pallet as MarketPallet};
use pallet_proofs::Pallet as ProofsPallet;
use pallet_storage_provider::{
    pallet::{BalanceOf, Call},
    Pallet as SpPallet,
};
use primitives::{
    commitment::{
        commd::compute_unsealed_sector_commitment,
        piece::{PaddedPieceSize, PieceInfo},
        CommD, CommP, Commitment,
    },
    proofs::{RegisteredPoStProof, RegisteredSealProof},
    sector::{
        builder::SectorPreCommitInfoBuilder, ProveCommitSector, SectorNumber, SectorPreCommitInfo,
    },
    MAX_LABEL_SIZE, MAX_SECTORS_PER_CALL, PEER_ID_MAX_BYTES,
};
use sp_core::Get;
use sp_runtime::{AccountId32, MultiSignature, MultiSigner};

use crate::{Config, Pallet};
type BoundedPeerIdBytes = BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>;

const EXISTENTIAL_DEPOSIT: u32 = 1_000_000_000;

struct BenchSector {
    porep: &'static [u8],
}

// If these files don't exist, please run `just generate-bench-proofs`.
const BENCH_SECTORS: &'static [BenchSector] = &[
    BenchSector {
        porep: include_bytes!("../../../../../target/bench/proofs/0.sector.proof.porep.scale"),
    },
    BenchSector {
        porep: include_bytes!("../../../../../target/bench/proofs/1.sector.proof.porep.scale"),
    },
];

const BENCH_PARAMS_POREP_VK: &'static [u8] =
    include_bytes!("../../../../../target/bench/params/8MiB.porep.vk.scale");

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
    fn pre_commit_sectors(n: Linear<1, 2>) {
        let alice = create_account_with_balance::<T>(ALICE, EXISTENTIAL_DEPOSIT * 2);
        let sp_id =
            create_and_register_storage_provider::<T>(STORAGE_PROVIDER, EXISTENTIAL_DEPOSIT * 2);

        assert_ok!(MarketPallet::<T>::add_balance(
            RawOrigin::Signed(sp_id.clone()).into(),
            EXISTENTIAL_DEPOSIT.into()
        ));

        assert_ok!(MarketPallet::<T>::add_balance(
            RawOrigin::Signed(alice.0.clone()).into(),
            EXISTENTIAL_DEPOSIT.into()
        ));

        let mut proposals = Vec::new();
        let mut sectors = Vec::new();

        for idx in 0..n {
            let TestProposal::<T> {
                proposal,
                sector_pre_commit,
                ..
            } = create_test_proposal(idx as u8, sp_id.clone(), alice.clone());

            proposals.push(proposal);
            sectors.push(sector_pre_commit);
        }

        assert_ok!(MarketPallet::<T>::publish_storage_deals(
            RawOrigin::Signed(sp_id.clone()).into(),
            proposals.try_into().unwrap(),
        ));

        let sectors: BoundedVec<_, ConstU32<{ MAX_SECTORS_PER_CALL }>> =
            sectors.try_into().unwrap();

        #[extrinsic_call]
        _(RawOrigin::Signed(sp_id.clone()), sectors);

        let state = SpPallet::<T>::storage_providers(sp_id).unwrap();
        let balance: BalanceOf<T> = (n * 1_u32).into();
        assert_eq!(state.pre_commit_deposits, balance);
    }

    #[benchmark]
    fn prove_commit_sectors(n: Linear<1, /* TODO: MAX_SECTORS_PER_CALL */ 2>) {
        let (sp_id, prove_sectors, total_fee) = prepare_prove_commit_sectors::<T>(n);

        #[extrinsic_call]
        _(RawOrigin::Signed(sp_id.clone()), prove_sectors.clone());

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
        // always assigns first deadline and first partition, probably will fail when we change deadline calculation algo.
        let deadline = &state.deadlines.due[0];
        let assigned_partition = &deadline.partitions[&0];
        //assert_eq!(
        //    assigned_partition.sectors.len(),
        //    1,
        //    "n == {n}, state: {state:#?}"
        //);
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
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
    let alice = create_account_with_balance::<T>(ALICE, EXISTENTIAL_DEPOSIT * 2);
    let sp_id =
        create_and_register_storage_provider::<T>(STORAGE_PROVIDER, EXISTENTIAL_DEPOSIT * 2);

    assert_ok!(MarketPallet::<T>::add_balance(
        RawOrigin::Signed(sp_id.clone()).into(),
        EXISTENTIAL_DEPOSIT.into()
    ));

    assert_ok!(MarketPallet::<T>::add_balance(
        RawOrigin::Signed(alice.0.clone()).into(),
        EXISTENTIAL_DEPOSIT.into()
    ));

    let mut proposals = Vec::new();
    let mut sectors_pre_commits = Vec::new();
    let mut prove_sectors = Vec::new();
    let mut total_fee = 0u64;
    for idx in 0..n {
        let TestProposal::<T> {
            proposal,
            sector_pre_commit,
            prove_commit_sector,
        } = create_test_proposal(idx as u8, sp_id.clone(), alice.clone());

        //total_fee += u64::try_from(proposal.proposal.storage_price_per_block).unwrap()
        //    * u64::try_from(proposal.proposal.end_block - proposal.proposal.start_block).unwrap();
        total_fee += u64::try_from(proposal.proposal.provider_collateral)
            .unwrap_or_else(|_| panic!("failed to convert provider_collateral to u64"));
        proposals.push(proposal);
        sectors_pre_commits.push(sector_pre_commit);
        prove_sectors.push(prove_commit_sector);
    }

    assert_ok!(MarketPallet::<T>::publish_storage_deals(
        RawOrigin::Signed(sp_id.clone()).into(),
        proposals.try_into().unwrap(),
    ));

    // Run to 1 to get VRF randomness
    run_to_block::<T>(1);

    let sectors_pre_commits: BoundedVec<_, ConstU32<{ MAX_SECTORS_PER_CALL }>> =
        sectors_pre_commits.try_into().unwrap();

    assert_ok!(SpPallet::<T>::pre_commit_sectors(
        RawOrigin::Signed(sp_id.clone()).into(),
        sectors_pre_commits
    ));

    // Run to 5 to enter pre-commit period
    run_to_block::<T>(5);

    let prove_sectors: BoundedVec<_, ConstU32<{ MAX_SECTORS_PER_CALL }>> =
        prove_sectors.try_into().unwrap();

    assert_ok!(ProofsPallet::<T>::set_porep_verifying_key(
        RawOrigin::Signed(sp_id.clone()).into(),
        RegisteredSealProof::StackedDRG8MiBV1,
        BENCH_PARAMS_POREP_VK.to_vec(),
    ));

    (sp_id, prove_sectors, total_fee as u32)
}

fn create_test_commitment() -> (Commitment<CommP>, Commitment<CommD>) {
    let piece_commitment = Commitment::<CommP>::from_cid(
        &Cid::from_str("baga6ea4seaqhx2sxpfc2f3k2o75m3acskihug7me3g4coyw6adjqnd6ioszfqay").unwrap(),
    )
    .unwrap();
    let unsealed_cid = Commitment::<CommD>::from_cid(
        &Cid::from_str("baga6ea4seaqjzzj3jpjqkmnzwjueswah6a7wbiahq6arqknv57znraeoiwm4kgq").unwrap(),
    )
    .unwrap();
    (piece_commitment, unsealed_cid)
}

/// Run until a particular block.
///
/// Stolen't from: <https://github.com/paritytech/polkadot-sdk/blob/7df94a469e02e1d553bd4050b0e91870d6a4c31b/substrate/frame/lottery/src/mock.rs#L87-L98>
pub fn run_to_block<T>(n: u32)
where
    T: crate::Config,
    T: pallet_storage_provider::Config,
{
    use frame_support::traits::Hooks;

    while frame_system::Pallet::<T>::block_number() < n.into() {
        if frame_system::Pallet::<T>::block_number() > 1u32.into() {
            pallet_storage_provider::Pallet::<T>::on_finalize(
                frame_system::Pallet::<T>::block_number(),
            );
            //crate::Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());
            frame_system::Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());
        }

        frame_system::Pallet::<T>::set_block_number(
            frame_system::Pallet::<T>::block_number() + 1u32.into(),
        );
        frame_system::Pallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
        //crate::Pallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
        pallet_storage_provider::Pallet::<T>::on_initialize(
            frame_system::Pallet::<T>::block_number(),
        );
    }
}

struct TestProposal<T: pallet_market::Config> {
    proposal: ClientDealProposalOf<T>,
    sector_pre_commit: SectorPreCommitInfo<BlockNumberFor<T>>,
    prove_commit_sector: ProveCommitSector,
}

fn create_test_proposal<T>(
        id: u8,
        provider: <T as frame_system::Config>::AccountId,
        client: (<T as frame_system::Config>::AccountId, MultiSigner),
) -> TestProposal<T> where T: crate::Config, <<<T as frame_system::Config>::Block as sp_runtime::traits::Block>::Header as sp_runtime::traits::Header>::Number: From<u64>{
    let (piece_commitment, unsealed_cid) = create_test_commitment();

    let min_dur = T::MinDealDuration::get();

    let label = vec![id as u8; MAX_LABEL_SIZE as usize];

    let proposal = DealProposalOf::<T> {
        piece_cid: piece_commitment
            .cid()
            .to_bytes()
            .try_into()
            .expect("hash is always 32 bytes"),
        piece_size: *PaddedPieceSize::from_arbitrary_size(185459),
        client: client.0,
        provider,
        label: BoundedVec::try_from(label).unwrap(),
        start_block: 100.into(),
        end_block: min_dur + 100.into(),
        storage_price_per_block: 5u32.into(),
        provider_collateral: 25u32.into(),
        state: DealState::Published,
    };

    let proposal = sign_proposal::<T>(client.1, proposal);

    let sector_pre_commit = SectorPreCommitInfoBuilder::<BlockNumberFor<T>>::default()
        .raw_unsealed_cid(unsealed_cid.cid().to_bytes().try_into().unwrap())
        .sector_number(
            (id as u32)
                .try_into()
                .expect("n only goes up to 32 so this should be ok"),
        )
        .deals(vec![id as u64])
        .seal_proof(RegisteredSealProof::StackedDRG8MiBV1)
        .expiration(min(
            T::SectorMaximumLifetime::get(),
            T::MaxSectorExpiration::get(),
        ))
        .seal_randomness_height(1.into())
        .build();

    let prove_commit_sector = ProveCommitSector {
        sector_number: SectorNumber::new(id as u32).unwrap(),
        proofs: BoundedVec::try_from(vec![BoundedVec::try_from(
            BENCH_SECTORS[id as usize].porep.to_vec(),
        )
        .unwrap()])
        .unwrap(),
    };

    TestProposal {
        proposal,
        sector_pre_commit,
        prove_commit_sector,
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

fn create_and_register_storage_provider<T>(name: &'static str, balance: u32) -> AccountId32
where
    T: crate::Config<AccountId = AccountId32, PeerId = BoundedPeerIdBytes>,
{
    let account = create_account_with_balance::<T>(name, balance);

    let peer_id: T::PeerId = name.as_bytes().to_vec().try_into().unwrap();
    assert_ok!(SpPallet::<T>::register_storage_provider(
        RawOrigin::Signed(account.0.clone()).into(),
        peer_id.clone(),
        RegisteredPoStProof::StackedDRGWindow8MiBV1,
    ));
    account.0
}
