#![cfg(feature = "runtime-benchmarks")]

use alloc::vec;

use frame_benchmarking::v2::*;
use frame_support::{
    assert_ok,
    pallet_prelude::ConstU32,
    traits::{Currency, OnInitialize},
    BoundedVec,
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use pallet_market::{DealState, Pallet as MarketPallet};
use pallet_storage_provider::{
    pallet::{BalanceOf, Call},
    Pallet as SpPallet,
};
use primitives::{
    commitment::{
        commd::compute_unsealed_sector_commitment,
        piece::{PaddedPieceSize, PieceInfo},
        CommP, Commitment,
    },
    proofs::RegisteredPoStProof,
    sector::{builder::SectorPreCommitInfoBuilder, SectorPreCommitInfo},
    MAX_SECTORS_PER_CALL, PEER_ID_MAX_BYTES,
};
use sp_core::Get;
use sp_runtime::{AccountId32, MultiSignature};

use crate::utils::{
    generate_benchmark_account, sign_proposal, ClientDealProposalOf, DealProposalOf, ALICE, CHARLIE,
};

pub struct Pallet<T: Config>(pallet_storage_provider::Pallet<T>);
pub trait Config:
    pallet_storage_provider::Config
    + pallet_balances::Config
    + pallet_market::Config
    + frame_system::Config
{
}

impl<T: Config> OnInitialize<BlockNumberFor<T>> for Pallet<T> {
    fn on_initialize(n: BlockNumberFor<T>) -> frame_support::weights::Weight {
        pallet_storage_provider::Pallet::<T>::on_initialize(n)
    }
}

type BoundedPeerIdBytes = BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>;

#[benchmarks(
    where
        T: crate::Config<
            PeerId = BoundedPeerIdBytes,
            AccountId = AccountId32,
            OffchainSignature = MultiSignature,
        >,
        BlockNumberFor<T>: From<u64>
)]
mod benchmarks {

    use super::*;

    const EXISTENTIAL_DEPOSIT: u32 = 1_000_000_000;

    #[benchmark]
    fn register_storage_provider() {
        let caller: T::AccountId = whitelisted_caller();
        let peer_id: T::PeerId = b"peer_id".to_vec().try_into().unwrap();
        let window_post_proof_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;

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
    fn pre_commit_sectors() {
        let (sp_id, _) = generate_benchmark_account(CHARLIE);
        let (alice_id, alice_sign) = generate_benchmark_account(ALICE);

        {
            pallet_balances::Pallet::<T>::make_free_balance_be(
                &sp_id,
                (EXISTENTIAL_DEPOSIT * 2).into(),
            );
            pallet_balances::Pallet::<T>::make_free_balance_be(
                &alice_id,
                (EXISTENTIAL_DEPOSIT * 2).into(),
            );
        }

        let peer_id: T::PeerId = ALICE.as_bytes().to_vec().try_into().unwrap();
        let window_post_proof_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;

        assert_ok!(SpPallet::<T>::register_storage_provider(
            RawOrigin::Signed(sp_id.clone()).into(),
            peer_id.clone(),
            window_post_proof_type,
        ));

        assert_ok!(MarketPallet::<T>::add_balance(
            RawOrigin::Signed(sp_id.clone()).into(),
            EXISTENTIAL_DEPOSIT.into()
        ));

        assert_ok!(MarketPallet::<T>::add_balance(
            RawOrigin::Signed(alice_id.clone()).into(),
            EXISTENTIAL_DEPOSIT.into()
        ));

        let piece_commitment = Commitment::<CommP>::from(*b"dummydummydummydummydummydummydu");
        let unsealed_cid = compute_unsealed_sector_commitment(
            window_post_proof_type.sector_size(),
            &[PieceInfo {
                commitment: piece_commitment,
                size: PaddedPieceSize::new(128).unwrap(),
            }],
        )
        .unwrap();

        let min_dur = T::MinDealDuration::get();

        let proposal = DealProposalOf::<T> {
            piece_cid: piece_commitment
                .cid()
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            piece_size: 128,
            client: alice_id,
            provider: sp_id.clone(),
            label: vec![0xb, 0xe, 0xe, 0xf].try_into().unwrap(),
            start_block: 1.into(),
            end_block: min_dur + 1.into(),
            storage_price_per_block: 5u32.into(),
            provider_collateral: 25u32.into(),
            state: DealState::Published,
        };
        let deal = sign_proposal::<T>(alice_sign, proposal);
        assert_ok!(MarketPallet::<T>::publish_storage_deals(
            RawOrigin::Signed(sp_id.clone()).into(),
            vec![deal].try_into().unwrap(),
        ));

        let sector = SectorPreCommitInfoBuilder::default()
            .raw_unsealed_cid(unsealed_cid.cid().to_bytes().try_into().unwrap())
            .deals(vec![0])
            .build();
        let sectors: BoundedVec<
            SectorPreCommitInfo<BlockNumberFor<T>>,
            ConstU32<MAX_SECTORS_PER_CALL>,
        > = vec![sector].try_into().unwrap();

        frame_system::Pallet::<T>::reset_events();

        #[extrinsic_call]
        _(RawOrigin::Signed(sp_id.clone()), sectors.clone());

        let state = SpPallet::<T>::storage_providers(sp_id).unwrap();
        let balance: BalanceOf<T> = 1_u32.into();
        assert_eq!(state.pre_commit_deposits, balance);
    }

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
