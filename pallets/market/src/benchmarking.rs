#![cfg(feature = "runtime-benchmarks")]

use codec::Encode;
use frame_benchmarking::v2::*;
use frame_support::{pallet_prelude::ConstU32, sp_runtime::BoundedVec, traits::Currency};
use frame_system::{
    self,
    pallet_prelude::{BlockNumberFor, OriginFor},
    RawOrigin,
};
use pallet_storage_provider::Pallet as SpPallet;
use primitives::{
    commitment::{piece::PaddedPieceSize, CommP, Commitment},
    proofs::RegisteredPoStProof,
    sector::{builder::SectorPreCommitInfoBuilder, ProveCommitSector, SectorPreCommitInfo},
    MAX_LABEL_SIZE, MAX_SECTORS_PER_CALL, PEER_ID_MAX_BYTES,
};
use scale_info::prelude::format;
use sp_core::ed25519;
use sp_io::crypto::{ed25519_generate, ed25519_sign};
use sp_runtime::{traits::IdentifyAccount, AccountId32, MultiSignature, MultiSigner};
use sp_std::{vec, vec::Vec};

use super::*;
#[allow(unused)]
use crate::Pallet as MarketPallet;

type BoundedPeerIdBytes = BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>;
const COLLATERAL: u32 = 10;

fn generate_benchmark_account(name: &'static str) -> (AccountId32, MultiSigner) {
    // Generate a deterministic seed
    let seed = format!("//{}", name);

    let signer: sp_core::ed25519::Public =
        ed25519_generate(0.into(), Some(seed.into_bytes())).into();
    let signer: MultiSigner = signer.into();
    let account_id = signer.clone().into_account();

    (account_id, signer)
}

fn create_ed25519_signature(payload: &[u8], pubkey: MultiSigner) -> MultiSignature {
    let edpubkey = ed25519::Public::try_from(pubkey).unwrap();
    let edsig = ed25519_sign(0.into(), &edpubkey, payload).unwrap();
    edsig.into()
}

fn prepare_proposals<T>(
    n: u32,
    caller: T::AccountId,
    client: T::AccountId,
    client_pair: MultiSigner,
) -> (
    Vec<ClientDealProposal<T::AccountId, BalanceOf<T>, BlockNumberFor<T>, MultiSignature>>,
    u32,
)
where
    T: crate::Config,
{
    let start_block: u32 = 50;
    let end_block: u32 = 100;
    let price_per_block: u32 = 10;
    let cost = (end_block - start_block) * price_per_block * n;

    let mut proposals = vec![];
    for idx in 1..=n {
        // Cast is safe since `n` never goes beyond 255
        let label = vec![idx as u8; MAX_LABEL_SIZE as usize];

        let proposal = DealProposal::<T::AccountId, BalanceOf<T>, BlockNumberFor<T>> {
            piece_cid: Commitment::<CommP>::zero(PaddedPieceSize::MIN)
                .cid()
                .to_bytes()
                .try_into()
                .unwrap(),
            piece_size: 2048,
            client: client.clone(),
            provider: caller.clone(),
            label: BoundedVec::try_from(label).unwrap(),
            start_block: start_block.into(),
            end_block: end_block.into(),
            storage_price_per_block: price_per_block.into(),
            provider_collateral: COLLATERAL.into(),
            state: DealState::<_>::Published,
        };

        let signed_deal_proposal =
            create_ed25519_signature(proposal.encode().as_slice(), client_pair.clone()).into();

        let proposal = ClientDealProposal::<T::AccountId, BalanceOf<T>, BlockNumberFor<T>, _> {
            proposal,
            client_signature: signed_deal_proposal,
        };

        proposals.push(proposal);
    }
    return (proposals, cost);
}

#[frame_benchmarking::v2::benchmarks(
    where
        T: crate::Config<OffchainSignature = sp_runtime::MultiSignature>,
        T: pallet_balances::Config,
        T: pallet_storage_provider::Config<
            PeerId = BoundedPeerIdBytes,
        >,
        T: frame_system::Config<AccountId = AccountId32>
)]
mod benchmarks {
    use itertools::Itertools;

    use super::*;

    const EXISTENTIAL_DEPOSIT: u32 = 1_000_000_000;

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
            Pallet::<T>::add_balance(
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
        Pallet::<T>::add_balance(
            RawOrigin::Signed(caller.clone().into()).into(),
            EXISTENTIAL_DEPOSIT.into(),
        )
        .unwrap();

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            Pallet::<T>::withdraw_balance(
                RawOrigin::Signed(caller.clone()).into(),
                EXISTENTIAL_DEPOSIT.into(),
            )
            .unwrap();
        }

        let balance_entry = BalanceTable::<T>::get(&caller);
        assert_eq!(balance_entry.free, 0u32.into());
        assert_eq!(balance_entry.locked, 0u32.into());
    }

    // When running `cargo test` the first get's called,
    // for the proper benchmarks we get the second
    #[cfg(test)]
    const MAX_DEALS: u32 = 32;
    #[cfg(not(test))]
    const MAX_DEALS: u32 = 128;

    /// `n`: number of submitted deals
    #[benchmark]
    fn publish_storage_deals(n: Linear<1, MAX_DEALS>) {
        let caller: T::AccountId = whitelisted_caller();
        setup_account_balance::<T>(caller.clone());
        // Register the caller as a storage provider
        pallet_storage_provider::Pallet::<T>::register_storage_provider(
            RawOrigin::Signed(caller.clone()).into(),
            BoundedVec::try_from("placeholder".as_bytes().to_vec()).unwrap(),
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
        )
        .unwrap();

        // Setup a client account
        let (client, client_pair): (T::AccountId, _) = generate_benchmark_account("client");
        setup_account_balance::<T>(client.clone());

        let (proposals, cost) =
            prepare_proposals::<T>(n, caller.clone(), client.clone(), client_pair);
        let proposals = BoundedVec::try_from(proposals).unwrap();

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            Pallet::<T>::publish_storage_deals(RawOrigin::Signed(caller.clone()).into(), proposals)
                .unwrap();
        }

        let balance_entry = BalanceTable::<T>::get(&caller);
        assert_eq!(
            balance_entry.free,
            (EXISTENTIAL_DEPOSIT - (COLLATERAL * n)).into()
        );
        assert_eq!(balance_entry.locked, (COLLATERAL * n).into());

        let balance_entry = BalanceTable::<T>::get(&client);
        assert_eq!(balance_entry.free, (EXISTENTIAL_DEPOSIT - cost).into());
        assert_eq!(balance_entry.locked, cost.into());
    }

    /// `n`: number of submitted deals
    #[benchmark]
    fn settle_deal_payments(n: Linear<1, MAX_DEALS>) {
        let caller: T::AccountId = whitelisted_caller();
        setup_account_balance::<T>(caller.clone());
        // Register the caller as a storage provider
        pallet_storage_provider::Pallet::<T>::register_storage_provider(
            RawOrigin::Signed(caller.clone()).into(),
            BoundedVec::try_from("placeholder".as_bytes().to_vec()).unwrap(),
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
        )
        .unwrap();

        // Setup a client account
        let (client, client_pair): (T::AccountId, _) = generate_benchmark_account("client");
        setup_account_balance::<T>(client.clone());

        let (proposals, cost) =
            prepare_proposals::<T>(n, caller.clone(), client.clone(), client_pair);
        let proposals = BoundedVec::try_from(proposals).unwrap();

        let storage_provider: OriginFor<T> = RawOrigin::Signed(caller.clone()).into();
        Pallet::<T>::publish_storage_deals(storage_provider.clone(), proposals.clone()).unwrap();

        // Run to 1 to get VRF randomness
        run_to_block::<T>(1);

        proposals
            .into_iter()
            .enumerate()
            // Create SectorPreCommitInfo from the proposals
            .map(|(idx, p)| {
                SectorPreCommitInfoBuilder::<BlockNumberFor<T>>::default()
                    .deals(vec![idx as u64])
                    .sector_number(
                        (idx as u32)
                            .try_into()
                            .expect("n only goes up to 32 so this should be ok"),
                    )
                    .raw_unsealed_cid(p.proposal.piece_cid.clone())
                    .build()
            })
            // Chunk into `MAX_DEALS` groups, required because of the cfg flag
            // meaning that sometimes MAX_DEALS > MAX_SECTORS_PER_CALL
            .chunks(MAX_SECTORS_PER_CALL as usize)
            // Convert IntoChunks to Iterator
            .into_iter()
            // Convert the chunks into Vecs
            .map(Vec::from_iter)
            // Convert each chunk into a BoundedVec
            .map(|sectors_for_call| {
                BoundedVec::<
                    SectorPreCommitInfo<BlockNumberFor<T>>,
                    ConstU32<MAX_SECTORS_PER_CALL>
                >::try_from(sectors_for_call).unwrap()
            })
            // Submit each "chunk"
            .for_each(|pre_commit_infos| {
                SpPallet::<T>::pre_commit_sectors(storage_provider.clone(), pre_commit_infos)
                    .unwrap();
            });

        // Run to 11 to enter pre-commit period
        run_to_block::<T>(11);

        let proofs = {
            // can't use bounded_vec![] in benchmarks
            let mut proofs = BoundedVec::new();
            // Empty proof is considered valid under the DummyProofVerifier
            proofs.try_push(BoundedVec::new()).unwrap();
            proofs
        };

        // Similar pattern to before
        (0..n)
            // Create ProveCommitSectors from the `n` "index"
            .map(|n| ProveCommitSector {
                sector_number: n.try_into().unwrap(),
                proofs: proofs.clone(),
            })
            // Chunk into MAX_SECTORS_PER_CALL due to the cfg
            .chunks(MAX_SECTORS_PER_CALL as usize)
            // Convert IntoChunks to Iterator
            .into_iter()
            // Convert the chunks into Vecs
            .map(Vec::from_iter)
            // Convert the Vecs into BoundedVecs
            .map(|prove_commit_sectors| {
                BoundedVec::<_, ConstU32<MAX_SECTORS_PER_CALL>>::try_from(prove_commit_sectors)
                    .unwrap()
            })
            // Submit them
            .for_each(|prove_commit_sectors| {
                SpPallet::<T>::prove_commit_sectors(storage_provider.clone(), prove_commit_sectors)
                    .unwrap();
            });

        run_to_block::<T>(101);

        // We know that DealIds are incrementing integers, making this a "valid approach",
        // it is kinda cheating but this way we don't need to care for the events
        let deal_ids = BoundedVec::try_from(Vec::from_iter(0..(n as u64))).unwrap();

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            Pallet::<T>::settle_deal_payments(storage_provider.clone(), deal_ids).unwrap();
        }

        assert_eq!(
            MarketPallet::<T>::free(&caller).unwrap(),
            (EXISTENTIAL_DEPOSIT + cost).into()
        );
        assert_eq!(MarketPallet::<T>::locked(&caller).unwrap(), 0u32.into());

        assert_eq!(
            MarketPallet::<T>::free(&client).unwrap(),
            (EXISTENTIAL_DEPOSIT - cost).into()
        );
        assert_eq!(MarketPallet::<T>::locked(&client).unwrap(), 0u32.into());
    }

    /// `n` == 1: Publish
    /// `n` == 2: Publish & Replace
    #[benchmark]
    fn publish_deal_parameters(n: Linear<1, 2>) {
        let caller: T::AccountId = whitelisted_caller();
        // Register the caller as a storage provider
        pallet_storage_provider::Pallet::<T>::register_storage_provider(
            RawOrigin::Signed(caller.clone()).into(),
            BoundedVec::try_from("placeholder".as_bytes().to_vec()).unwrap(),
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
        )
        .unwrap();

        let deal_parameters: DealParameters<BalanceOf<T>, BlockNumberFor<T>> = DealParameters {
            minimum_price_per_block: 1u32.into(),
            deal_duration: DealDurationBound {
                lower: Some(1u32.into()),
                upper: Some(10u32.into()),
            },
        };
        let storage_provider: OriginFor<T> = RawOrigin::Signed(caller.clone()).into();

        if n == 2 {
            Pallet::<T>::publish_deal_parameters(storage_provider.clone(), deal_parameters.clone())
                .unwrap();
        }

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            Pallet::<T>::publish_deal_parameters(storage_provider.clone(), deal_parameters.clone())
                .unwrap();
        }
        assert_eq!(SPDealParameters::<T>::get(&caller), Some(deal_parameters));
    }

    /// `n` == 1: Remove with nothing there
    /// `n` == 2: Insert and remove
    #[benchmark]
    fn remove_deal_parameters(n: Linear<1, 2>) {
        let caller: T::AccountId = whitelisted_caller();
        // Register the caller as a storage provider
        pallet_storage_provider::Pallet::<T>::register_storage_provider(
            RawOrigin::Signed(caller.clone()).into(),
            BoundedVec::try_from("placeholder".as_bytes().to_vec()).unwrap(),
            RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
        )
        .unwrap();

        let deal_parameters: DealParameters<BalanceOf<T>, BlockNumberFor<T>> = DealParameters {
            minimum_price_per_block: 1u32.into(),
            deal_duration: DealDurationBound {
                lower: Some(1u32.into()),
                upper: Some(10u32.into()),
            },
        };
        let storage_provider: OriginFor<T> = RawOrigin::Signed(caller.clone()).into();

        if n == 2 {
            Pallet::<T>::publish_deal_parameters(storage_provider.clone(), deal_parameters)
                .unwrap();
        }

        // #[extrinsic_call] requires type shenanigans, using #[block] is MUCH simpler
        #[block]
        {
            Pallet::<T>::remove_deal_parameters(storage_provider.clone()).unwrap();
        }

        assert_eq!(SPDealParameters::<T>::get(&caller), None);
    }

    impl_benchmark_test_suite! {
        MarketPallet,
        crate::mock::new_test_ext(),
        crate::mock::Test,
    }
}
