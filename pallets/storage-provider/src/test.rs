use codec::Encode;
use frame_support::{
    assert_noop, assert_ok,
    sp_runtime::{bounded_vec, BoundedVec},
    traits::ConstU32,
};
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_market::{BalanceOf, ClientDealProposal, DealProposal, DealState};
use primitives_proofs::{RegisteredPoStProof, RegisteredSealProof};
use sp_core::Pair;
use sp_runtime::MultiSignature;

use crate::{
    mock::*,
    pallet::{Error, Event, StorageProviders},
    proofs::{PoStProof, SubmitWindowedPoStParams},
    sector::{ProveCommitSector, SectorPreCommitInfo},
    storage_provider::StorageProviderInfo,
};

#[test]
fn initial_state() {
    new_test_ext().execute_with(|| {
        assert!(!StorageProviders::<Test>::contains_key(account(ALICE)));
        assert!(!StorageProviders::<Test>::contains_key(account(BOB)));
    })
}

/// Tests if storage provider registration is successful.
#[test]
fn register_sp() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
        let expected_sector_size = window_post_type.sector_size();
        let expected_partition_sectors = window_post_type.window_post_partitions_sector();
        let expected_sp_info = StorageProviderInfo::new(peer_id.clone(), window_post_type);
        // Register BOB as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(account(BOB)),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(account(BOB)));
        // `unwrap()` should be safe because of the above check.
        let sp_bob = StorageProviders::<Test>::get(account(BOB)).unwrap();
        // Check that storage provider information is correct.
        assert_eq!(sp_bob.info.peer_id, peer_id);
        assert_eq!(sp_bob.info.window_post_proof_type, window_post_type);
        assert_eq!(sp_bob.info.sector_size, expected_sector_size);
        assert_eq!(
            sp_bob.info.window_post_partition_sectors,
            expected_partition_sectors
        );
        // Check that pre commit sectors are empty.
        assert!(sp_bob.pre_committed_sectors.is_empty());
        // Check that no pre commit deposit is made
        assert_eq!(sp_bob.pre_commit_deposits, 0);
        // Check that sectors are empty.
        assert!(sp_bob.sectors.is_empty());
        // Check that the event triggered
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::StorageProviderRegistered {
                    owner: account(BOB),
                    info: expected_sp_info,
                },
            )]
        );
    })
}

/// Check that double registration fails
#[test]
fn double_register_sp() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
        // Register BOB as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(account(BOB)),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(account(BOB)));
        // Try to register BOB again. Should fail
        assert_noop!(
            StorageProvider::register_storage_provider(
                RuntimeOrigin::signed(account(BOB)),
                peer_id.clone(),
                window_post_type,
            ),
            Error::<Test>::StorageProviderExists
        );
    });
}

#[test]
fn pre_commit_sector() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
        // Register ALICE as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(account(ALICE)),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(account(ALICE)));
        // Check that the event triggered
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::StorageProviderRegistered {
                    owner: account(ALICE),
                    info: StorageProviderInfo::new(peer_id, window_post_type),
                },
            )]
        );
        let sector = SectorPreCommitInfo {
            seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
            sector_number: 1,
            sealed_cid: cid_of("sealed_cid")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            deal_ids: bounded_vec![0, 1],
            expiration: YEARS,
            unsealed_cid: cid_of("unsealed_cid")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
        };
        // Check starting balance
        assert_eq!(Balances::free_balance(account(ALICE)), 100);
        // Run pre commit extrinsic
        StorageProvider::pre_commit_sector(RuntimeOrigin::signed(account(ALICE)), sector.clone())
            .expect("Pre commit failed");
        // Check that the event triggered
        assert_eq!(
            events(),
            [
                RuntimeEvent::Balances(pallet_balances::Event::<Test>::Reserved {
                    who: account(ALICE),
                    amount: 1
                },),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorPreCommitted {
                    owner: account(ALICE),
                    sector: sector.clone(),
                })
            ]
        );
        let sp_alice = StorageProviders::<Test>::get(account(ALICE))
            .expect("SP Alice should be present because of the pre-check");

        assert!(sp_alice.sectors.is_empty()); // not yet proven
        assert!(!sp_alice.pre_committed_sectors.is_empty());
        assert_eq!(sp_alice.pre_commit_deposits, 1);
        assert_eq!(Balances::free_balance(account(ALICE)), 99);
    });
}

#[test]
fn pre_commit_sector_fails_when_precommited_twice() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
        // Register ALICE as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(account(ALICE)),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(account(ALICE)));
        // Check that the event triggered
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::StorageProviderRegistered {
                    owner: account(ALICE),
                    info: StorageProviderInfo::new(peer_id, window_post_type),
                },
            )]
        );
        let sector = SectorPreCommitInfo {
            seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
            sector_number: 1,
            sealed_cid: cid_of("sealed_cid")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            deal_ids: bounded_vec![0, 1],
            expiration: YEARS,
            unsealed_cid: cid_of("unsealed_cid")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
        };
        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(account(ALICE)),
            sector.clone()
        ));
        // Run same extrinsic, this should fail
        assert_noop!(
            StorageProvider::pre_commit_sector(
                RuntimeOrigin::signed(account(ALICE)),
                sector.clone()
            ),
            Error::<Test>::SectorNumberAlreadyUsed,
        );
    });
}

#[test]
fn prove_commit_sector() {
    new_test_ext().execute_with(|| {
        let _ = Market::add_balance(RuntimeOrigin::signed(account(ALICE)), 60);
        let _ = Market::add_balance(RuntimeOrigin::signed(account(BOB)), 70);
        let deal_proposal = DealProposalBuilder::default()
            .client(BOB)
            .provider(ALICE)
            .signed(BOB);
        assert_ok!(Market::publish_storage_deals(
            RuntimeOrigin::signed(account(ALICE)),
            bounded_vec![deal_proposal],
        ));
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
        let sector_number = 1;
        // Register ALICE as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(account(ALICE)),
            peer_id.clone(),
            window_post_type,
        ));
        assert!(StorageProviders::<Test>::contains_key(account(ALICE)));
        let sector = SectorPreCommitInfo {
            seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
            sector_number,
            sealed_cid: cid_of("sealed_cid")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            deal_ids: bounded_vec![0],
            expiration: YEARS,
            unsealed_cid: cid_of("unsealed_cid")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
        };
        // Run pre commit extrinsic
        assert_ok!(StorageProvider::pre_commit_sector(
            RuntimeOrigin::signed(account(ALICE)),
            sector.clone()
        ));
        // check that the deposit has been reserved.
        assert_eq!(Balances::free_balance(account(ALICE)), 39);
        // flush the events
        events();
        // Test prove commits
        let sector = ProveCommitSector {
            sector_number,
            proof: cid_of("prove_commit")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
        };
        assert_ok!(StorageProvider::prove_commit_sector(
            RuntimeOrigin::signed(account(ALICE)),
            sector
        ));
        assert_eq!(
            events(),
            [
                RuntimeEvent::Market(pallet_market::Event::DealActivated {
                    deal_id: 0,
                    client: account(BOB),
                    provider: account(ALICE)
                }),
                RuntimeEvent::StorageProvider(Event::<Test>::SectorProven {
                    owner: account(ALICE),
                    sector_number: sector_number
                })
            ]
        );
        // check that the funds are still locked
        assert_eq!(Balances::free_balance(account(ALICE)), 39);
        let sp_state = StorageProviders::<Test>::get(account(ALICE))
            .expect("Should be able to get ALICE info");
        // check that the sector has been activated
        assert!(!sp_state.sectors.is_empty());
        assert!(sp_state.sectors.contains_key(&sector_number));
    });
}

#[test]
fn submit_windowed_post() {
    new_test_ext().execute_with(|| {
        let peer_id = "storage_provider_1".as_bytes().to_vec();
        let peer_id = BoundedVec::try_from(peer_id).unwrap();
        let window_post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
        // Register ALICE as a storage provider.
        assert_ok!(StorageProvider::register_storage_provider(
            RuntimeOrigin::signed(account(ALICE)),
            peer_id.clone(),
            window_post_type,
        ));
        // Flush events
        events();
        // Build window post proof
        let windowed_post = SubmitWindowedPoStParams {
            deadline: 1,
            index: 1,
            proof: PoStProof {
                post_proof: window_post_type,
                proof_bytes: bounded_vec![0x1, 0x2, 0x3],
            },
            chain_commit_block: System::block_number() - 1,
        };
        // Run extrinsic and assert that the result is `Ok`
        assert_ok!(StorageProvider::submit_windowed_post(
            RuntimeOrigin::signed(account(ALICE)),
            windowed_post,
        ));
        // Check that expected events were emitted
        assert_eq!(
            events(),
            [RuntimeEvent::StorageProvider(
                Event::<Test>::ValidPoStSubmitted {
                    owner: account(ALICE)
                }
            )]
        );
    });
}

/// Builder to simplify writing complex tests of [`DealProposal`].
/// Exclusively uses [`Test`] for simplification purposes.
struct DealProposalBuilder {
    piece_cid: BoundedVec<u8, ConstU32<128>>,
    piece_size: u64,
    client: AccountIdOf<Test>,
    provider: AccountIdOf<Test>,
    label: BoundedVec<u8, ConstU32<128>>,
    start_block: u64,
    end_block: u64,
    storage_price_per_block: u64,
    provider_collateral: u64,
    state: DealState<u64>,
}

impl Default for DealProposalBuilder {
    fn default() -> Self {
        Self {
            piece_cid: cid_of("polka-storage-data")
                .to_bytes()
                .try_into()
                .expect("hash is always 32 bytes"),
            piece_size: 18,
            client: account(BOB),
            provider: account(ALICE),
            label: bounded_vec![0xb, 0xe, 0xe, 0xf],
            start_block: 100,
            end_block: 110,
            storage_price_per_block: 5,
            provider_collateral: 25,
            state: DealState::Published,
        }
    }
}

impl DealProposalBuilder {
    pub fn client(mut self, client: &'static str) -> Self {
        self.client = account(client);
        self
    }

    pub fn provider(mut self, provider: &'static str) -> Self {
        self.provider = account(provider);
        self
    }

    pub fn unsigned(self) -> DealProposalOf<Test> {
        DealProposalOf::<Test> {
            piece_cid: self.piece_cid,
            piece_size: self.piece_size,
            client: self.client,
            provider: self.provider,
            label: self.label,
            start_block: self.start_block,
            end_block: self.end_block,
            storage_price_per_block: self.storage_price_per_block,
            provider_collateral: self.provider_collateral,
            state: self.state,
        }
    }

    pub fn signed(self, by: &'static str) -> ClientDealProposalOf<Test> {
        let built = self.unsigned();
        let signed = sign_proposal(by, built);
        signed
    }
}

type DealProposalOf<T> =
    DealProposal<<T as frame_system::Config>::AccountId, BalanceOf<T>, BlockNumberFor<T>>;

type ClientDealProposalOf<T> = ClientDealProposal<
    <T as frame_system::Config>::AccountId,
    BalanceOf<T>,
    BlockNumberFor<T>,
    MultiSignature,
>;

fn sign(pair: &sp_core::sr25519::Pair, bytes: &[u8]) -> MultiSignature {
    MultiSignature::Sr25519(pair.sign(bytes))
}

fn sign_proposal(client: &str, proposal: DealProposalOf<Test>) -> ClientDealProposalOf<Test> {
    let alice_pair = key_pair(client);
    let client_signature = sign(&alice_pair, &Encode::encode(&proposal));
    ClientDealProposal {
        proposal,
        client_signature,
    }
}
