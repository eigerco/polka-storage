#![cfg(feature = "runtime-benchmarks")]

mod accounts;
mod deal_proposals;

use alloc::{vec, vec::Vec};

use accounts::{generate_benchmark_account, ALICE, STORAGE_PROVIDER};
use deal_proposals::{sign_proposal, ClientDealProposalOf, DealProposalOf};
use frame_benchmarking::v2::*;
use frame_support::{assert_ok, pallet_prelude::ConstU32, traits::Currency, BoundedVec};
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
    sector::builder::SectorPreCommitInfoBuilder,
    MAX_SECTORS_PER_CALL, PEER_ID_MAX_BYTES,
};
use sp_core::Get;
use sp_runtime::{AccountId32, MultiSignature, MultiSigner};

use crate::{Config, Pallet};
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

    use primitives::{commitment::CommD, sector::SectorPreCommitInfo, MAX_LABEL_SIZE};

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

    fn create_test_commitment() -> (Commitment<CommP>, Commitment<CommD>) {
        let piece_commitment = Commitment::<CommP>::from(*b"dummydummydummydummydummydummydu");
        let unsealed_cid = compute_unsealed_sector_commitment(
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1.sector_size(),
            &[PieceInfo {
                commitment: piece_commitment,
                size: PaddedPieceSize::new(2048).unwrap(),
            }],
        )
        .unwrap();
        (piece_commitment, unsealed_cid)
    }

    struct TestProposal<T: pallet_market::Config> {
        proposal: ClientDealProposalOf<T>,
        sector_pre_commit: SectorPreCommitInfo<BlockNumberFor<T>>,
    }

    fn create_test_proposal<T>(
        id: u8,
        provider: <T as frame_system::Config>::AccountId,
        client: (<T as frame_system::Config>::AccountId, MultiSigner),
    ) -> TestProposal<T> where T: crate::Config, <<<T as frame_system::Config>::Block as sp_runtime::traits::Block>::Header as sp_runtime::traits::Header>::Number: From<u64>{
        let (piece_commitment, unsealed_cid) = create_test_commitment();

        let min_dur = T::MinDealDuration::get();

        let label = vec![id as u8; MAX_LABEL_SIZE as usize];

        let start_block = 400.into();
        let end_block = start_block + min_dur + 1.into();

        let proposal = DealProposalOf::<T> {
            piece_cid: piece_commitment
                .cid()
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            piece_size: 2048,
            client: client.0,
            provider,
            label: BoundedVec::try_from(label).unwrap(),
            start_block,
            end_block,
            storage_price_per_block: 5u32.into(),
            provider_collateral: 25u32.into(),
            state: DealState::Published,
        };

        let proposal = sign_proposal::<T>(client.1, proposal);

        let sector_pre_commit = SectorPreCommitInfoBuilder::<BlockNumberFor<T>>::default()
            .expiration(460.into())
            .raw_unsealed_cid(unsealed_cid.cid().to_bytes().try_into().unwrap())
            .sector_number(
                (id as u32)
                    .try_into()
                    .expect("n only goes up to 32 so this should be ok"),
            )
            .deals(vec![id as u64])
            .build();

        TestProposal {
            proposal,
            sector_pre_commit,
        }
    }

    fn create_account_with_balance<T>(
        name: &'static str,
        balance: u32,
    ) -> (AccountId32, MultiSigner)
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
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
        ));
        account.0
    }

    #[benchmark]
    fn pre_commit_sectors(n: Linear<1, MAX_SECTORS_PER_CALL>) {
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

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
