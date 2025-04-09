use core::fmt::Debug;

use frame_benchmarking::v2::*;
use frame_support::{pallet_prelude::ConstU32, sp_runtime::BoundedVec, traits::Currency};
use frame_system::{
    self,
    pallet_prelude::{BlockNumberFor, OriginFor},
    RawOrigin,
};
use pallet_market::{
    deal_parameters::{OffchainDealDurationBound, OffchainDealParameters},
    BalanceTable, Pallet as MarketPallet, SPDealParameters,
};
use pallet_proofs::Pallet as ProofsPallet;
use pallet_storage_provider::Pallet as SpPallet;
use primitives::{
    configs::BalanceOf,
    test_data::{generate_benchmark_account, BenchmarkData},
    MAX_DEALS_PER_SECTOR, PEER_ID_MAX_BYTES,
};
use sp_core::{Encode, Get};
use sp_runtime::AccountId32;
use sp_std::{iter::Sum, vec::Vec};

use crate::{Config, Pallet};

type BoundedPeerIdBytes = BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>;
const CLIENT: &'static str = "//Client";
const EXISTENTIAL_DEPOSIT: u32 = 1_000_000_000;

#[benchmarks(
    where
        T: crate::Config<
            PeerId = BoundedPeerIdBytes,
            AccountId = AccountId32,
            OffchainSignature = sp_runtime::MultiSignature,
        >,
        BlockNumberFor<T>: From<u64>,
        BalanceOf<T>: Sum + From<u32> + Encode,
        <T as pallet_market::Config>::RuntimeEvent: Debug,
)]
mod benchmarks {

    use frame_support::assert_ok;

    use super::*;

    fn setup_account_balance<T>(account: T::AccountId)
    where
        T: crate::Config,
        T: pallet_balances::Config,
    {
        pallet_balances::Pallet::<T>::make_free_balance_be(
            &account,
            (EXISTENTIAL_DEPOSIT * 2).into(),
        );

        // Add some balance so we can withdraw it
        MarketPallet::<T>::add_balance(
            RawOrigin::Signed(account.clone()).into(),
            EXISTENTIAL_DEPOSIT.into(),
        )
        .unwrap();

        assert_eq!(
            MarketPallet::<T>::free(&account).unwrap(),
            EXISTENTIAL_DEPOSIT.into()
        );
        assert_eq!(MarketPallet::<T>::locked(&account).unwrap(), 0u32.into());
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
                crate::Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());
                frame_system::Pallet::<T>::on_finalize(frame_system::Pallet::<T>::block_number());
            }

            frame_system::Pallet::<T>::set_block_number(
                frame_system::Pallet::<T>::block_number() + 1u32.into(),
            );
            frame_system::Pallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
            crate::Pallet::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
            pallet_storage_provider::Pallet::<T>::on_initialize(
                frame_system::Pallet::<T>::block_number(),
            );
        }
    }

    #[benchmark]
    fn add_balance() {
        let caller = whitelisted_caller();

        // `make_free_balance_be` returns an imbalance that gets automatically dropped here
        // if not consumed by other functions, that imbalance updates the total issuance on drop
        // as such, it should be dropped ASAP so that when a transfer occurs the issuance is "valid"
        // otherwise, an underflow occurs during the transfer
        // https://github.com/paritytech/polkadot-sdk/blob/721f6d97613b0ece9c8414e8ec8ba31d2f67d40c/substrate/frame/balances/src/impl_currency.rs#L328-L345
        // https://github.com/paritytech/polkadot-sdk/blob/721f6d97613b0ece9c8414e8ec8ba31d2f67d40c/substrate/frame/balances/src/impl_currency.rs#L222-L231
        // https://github.com/paritytech/polkadot-sdk/blob/6eca7647dc99dd0e78aacb740ba931e99e6ba71f/substrate/frame/support/src/traits/tokens/fungible/regular.rs#L317-L339
        // https://github.com/paritytech/polkadot-sdk/blob/721f6d97613b0ece9c8414e8ec8ba31d2f67d40c/substrate/frame/balances/src/impl_fungible.rs#L104-L151
        pallet_balances::Pallet::<T>::make_free_balance_be(
            &caller,
            // Must add more than the amount that will be added to the market balance
            // otherwise the account may not have enough to pay fees
            (EXISTENTIAL_DEPOSIT * 2).into(),
        );

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            MarketPallet::<T>::add_balance(
                RawOrigin::Signed(caller.clone()).into(),
                EXISTENTIAL_DEPOSIT.into(),
            )
            .unwrap();
        }

        let balance_entry = BalanceTable::<T>::get(&caller);
        assert_eq!(balance_entry.free, EXISTENTIAL_DEPOSIT.into());
        assert_eq!(balance_entry.locked, 0u32.into());
    }

    #[benchmark]
    fn withdraw_balance() {
        let caller: T::AccountId = whitelisted_caller();
        pallet_balances::Pallet::<T>::make_free_balance_be(
            &caller,
            (EXISTENTIAL_DEPOSIT * 2).into(),
        );
        // Add some balance so we can withdraw it
        MarketPallet::<T>::add_balance(
            RawOrigin::Signed(caller.clone().into()).into(),
            EXISTENTIAL_DEPOSIT.into(),
        )
        .unwrap();

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            MarketPallet::<T>::withdraw_balance(
                RawOrigin::Signed(caller.clone()).into(),
                EXISTENTIAL_DEPOSIT.into(),
            )
            .unwrap();
        }

        let balance_entry = BalanceTable::<T>::get(&caller);
        assert_eq!(balance_entry.free, 0u32.into());
        assert_eq!(balance_entry.locked, 0u32.into());
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
        let cost: u32 = proposals
            .iter()
            .map(|p| p.proposal.total_storage_fee().unwrap())
            .sum::<u128>()
            .try_into()
            .unwrap();
        let collaterals = proposals
            .iter()
            .map(|p| p.proposal.provider_collateral)
            .sum();

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            MarketPallet::<T>::publish_storage_deals(
                RawOrigin::Signed(sp.account_id.clone()).into(),
                proposals,
            )
            .unwrap();
        }

        let balance_entry = BalanceTable::<T>::get(&sp.account_id);
        assert_eq!(
            balance_entry.free,
            BalanceOf::<T>::from(EXISTENTIAL_DEPOSIT) - collaterals
        );
        assert_eq!(balance_entry.locked, collaterals);

        let balance_entry = BalanceTable::<T>::get(&client.0);
        assert_eq!(balance_entry.free, (EXISTENTIAL_DEPOSIT - cost).into());
        assert_eq!(balance_entry.locked, cost.into());
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
        MarketPallet::<T>::publish_storage_deals(storage_provider.clone(), proposals.clone())
            .unwrap();

        // Run to 5 to enter pre-commit period
        run_to_block::<T>(5);

        let pre_commit_infos = data.pre_commit_sectors(n);
        SpPallet::<T>::pre_commit_sectors(storage_provider.clone(), pre_commit_infos.clone())
            .unwrap();

        assert_ok!(ProofsPallet::<T>::set_porep_verifying_key(
            RawOrigin::Signed(sp.account_id.clone()).into(),
            data.seal_proof,
            data.verifying_key.to_vec(),
        ));

        run_to_block::<T>(15);

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
            MarketPallet::<T>::settle_deal_payments(storage_provider.clone(), deal_ids).unwrap();
        }

        assert_eq!(
            MarketPallet::<T>::free(&sp.account_id).unwrap(),
            (EXISTENTIAL_DEPOSIT + cost).into()
        );
        assert_eq!(
            MarketPallet::<T>::locked(&sp.account_id).unwrap(),
            0u32.into()
        );

        assert_eq!(
            MarketPallet::<T>::free(&client.0).unwrap(),
            (EXISTENTIAL_DEPOSIT - cost).into()
        );
        assert_eq!(MarketPallet::<T>::locked(&client.0).unwrap(), 0u32.into());
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
            MarketPallet::<T>::publish_deal_parameters(
                storage_provider.clone(),
                offchain_deal_parameters.clone(),
            )
            .unwrap();
        }

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            MarketPallet::<T>::publish_deal_parameters(
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

        MarketPallet::<T>::publish_deal_parameters(storage_provider.clone(), deal_parameters)
            .unwrap();

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            MarketPallet::<T>::remove_deal_parameters(storage_provider.clone()).unwrap();
        }

        assert_eq!(SPDealParameters::<T>::get(&sp.account_id), None);
    }

    impl_benchmark_test_suite! {
        Pallet,
        crate::test::new_test_ext(),
        crate::mock::Test,
    }
}
