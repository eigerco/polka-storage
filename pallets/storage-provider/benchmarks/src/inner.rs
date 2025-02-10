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
        T: pallet_storage_provider::Config<
            PeerId = BoundedPeerIdBytes,
            AccountId = AccountId32
        > + pallet_market::Config<
            AccountId = AccountId32,
            OffchainSignature = MultiSignature,
        > + pallet_balances::Config
        + frame_system::Config,
        <<<T as frame_system::Config>::Block as sp_runtime::traits::Block>::Header as sp_runtime::traits::Header>::Number: From<u64>
)]
mod benchmarks {

    use super::*;

    impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
