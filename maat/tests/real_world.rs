use std::{collections::BTreeSet, time::Duration};

use maat::*;
use primitives_proofs::SectorSize;
use storagext::{
    runtime::runtime_types::{
        bounded_collections::bounded_vec::BoundedVec,
        pallet_market::pallet::DealState,
        pallet_storage_provider::{proofs::SubmitWindowedPoStParams, sector::ProveCommitResult},
    },
    types::{
        market::DealProposal,
        storage_provider::{
            FaultDeclaration, ProveCommitSector, RecoveryDeclaration, SectorPreCommitInfo,
        },
    },
    IntoBoundedByteVec, MarketClientExt, PolkaStorageConfig, StorageProviderClientExt,
    SystemClientExt,
};
use subxt::{ext::sp_core::sr25519::Pair as Sr25519Pair, Config};
use zombienet_sdk::NetworkConfigExt;

/// Network's collator name. Used for logs and so on.
const COLLATOR_NAME: &str = "collator";

const STRATEGIC_SLEEP: Duration = Duration::from_secs(6);

/// Strategic sleep is a band-aid for [subxt#1668](https://github.com/paritytech/subxt/issues/1668).
///
/// The proper fix is to way for an event that actually reflects the success status of the operation.
/// A possible fix is shown in [polkadot-sdk#4883](https://github.com/paritytech/polkadot-sdk/pull/4883/files#diff-275b35fb5cb16898b64ab5ba6b7da61a5107b3941aee88303cc298e735acbaa7R131-R151),
/// however, it is not very ergonomic.
async fn strategic_sleep() {
    tracing::warn!(
        "sleeping for {:?}, for more information, see https://github.com/paritytech/subxt/issues/1668",
        STRATEGIC_SLEEP
    );
    // tokio::time::sleep(STRATEGIC_SLEEP).await;
}

async fn register_storage_provider<Keypair>(client: &storagext::Client, charlie: &Keypair)
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    // Register Charlie as a Storage Provider
    let peer_id = "dummy_peer_id".to_string();
    let peer_id_bytes = peer_id.as_bytes();

    let result = client
        .register_storage_provider(
            charlie,
            peer_id.clone(),
            primitives_proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
            true,
        )
        .await
        .unwrap()
        .unwrap();

    for event in result
        .events
        .find::<storagext::runtime::storage_provider::events::StorageProviderRegistered>()
    {
        let event = event.unwrap();

        assert_eq!(event.owner, charlie.account_id().clone().into());
        assert_eq!(event.proving_period_start, 63);
        assert_eq!(event.info.peer_id.0, peer_id.clone().into_bytes());
        assert_eq!(event.info.sector_size, SectorSize::_2KiB);
        assert_eq!(
            event.info.window_post_proof_type,
            primitives_proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1
        );
        assert_eq!(
            event.info.window_post_partition_sectors,
            primitives_proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1
                .window_post_partitions_sector()
        );
    }

    let retrieved_peer_info = client
        .retrieve_storage_provider(&subxt::utils::AccountId32::from(
            charlie.account_id().clone(),
        ))
        .await
        .unwrap()
        // this last unwrap ensures there's something there
        .unwrap()
        .info;
    let retrieved_peer_id = retrieved_peer_info.peer_id.0.as_slice();

    assert_eq!(retrieved_peer_id, peer_id_bytes);
}

async fn add_balance<Keypair>(client: &storagext::Client, account: &Keypair, balance: u128)
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    client
        .add_balance(account, balance, true)
        .await
        .unwrap()
        .unwrap();

    let balance_entry = client
        .retrieve_balance(account.account_id().clone())
        .await
        .unwrap()
        .unwrap();

    assert_eq!(balance_entry.free, balance);
    assert_eq!(balance_entry.locked, 0);
}

async fn settle_deal_payments<Keypair>(
    client: &storagext::Client,
    charlie: &Keypair,
    alice: &Keypair,
) where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    let settle_result = client
        .settle_deal_payments(charlie, vec![0], true)
        .await
        .unwrap()
        .unwrap();

    for event in settle_result
        .events
        .find::<storagext::runtime::market::events::DealsSettled>()
    {
        let event = event.unwrap();
        assert!(event.unsuccessful.0.is_empty());
        assert_eq!(event.successful.0[0].deal_id, 0);
        assert_eq!(event.successful.0[0].amount, 25_000_000_000);
        assert_eq!(
            event.successful.0[0].provider,
            charlie.account_id().clone().into()
        );
        assert_eq!(
            event.successful.0[0].client,
            alice.account_id().clone().into()
        );
    }
}
async fn publish_storage_deals<Keypair>(
    client: &storagext::Client,
    charlie: &Keypair,
    alice: &Keypair,
) where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    // Publish a storage deal
    let husky_storage_deal = DealProposal {
        piece_cid: cid::Cid::try_from(
            "baga6ea4seaqgi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5i",
        )
        .expect("valid CID"),
        piece_size: 2048,
        client: alice.account_id().clone(),
        provider: charlie.account_id().clone(),
        label: "My lovely Husky (husky.jpg)".to_owned(),
        start_block: 65,
        end_block: 115,
        storage_price_per_block: 500000000,
        provider_collateral: 12500000000,
        state: DealState::Published,
    };

    let deal_result = client
        .publish_storage_deals(charlie, alice, vec![husky_storage_deal], true)
        .await
        .unwrap()
        .unwrap();

    for event in deal_result
        .events
        .find::<storagext::runtime::market::events::DealPublished>()
    {
        let event = event.unwrap();
        tracing::debug!(?event);

        assert_eq!(event.client, alice.account_id().clone().into());
        assert_eq!(event.provider, charlie.account_id().clone().into());
        assert_eq!(event.deal_id, 0); // first deal ever
    }
}

async fn pre_commit_sectors<Keypair>(client: &storagext::Client, charlie: &Keypair)
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    // Unsealed sector commitment
    let unsealed_cid =
        cid::Cid::try_from("baga6ea4seaqgi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5lnnv4wi5i")
            .expect("valid CID");

    // Sealed sector commitment.
    let sealed_cid =
        cid::Cid::try_from("bagboea4b5abcahzgmrmzan2urtn5qobkffrkaxwbc7iesqt6o7wgwr4hrwget7n4")
            .expect("valid CID");

    let sectors_pre_commit_info = vec![SectorPreCommitInfo {
        seal_proof: primitives_proofs::RegisteredSealProof::StackedDRG2KiBV1P1,
        sector_number: 1,
        sealed_cid,
        deal_ids: vec![0],
        expiration: 165,
        unsealed_cid,
        // TODO: This height depends on the block of the randomness fetched from
        // the network when sealing a sector.
        seal_randomness_height: 0,
    }];

    let result = client
        .pre_commit_sectors(charlie, sectors_pre_commit_info.clone(), true)
        .await
        .unwrap()
        .unwrap();

    for event in result
        .events
        .find::<storagext::runtime::storage_provider::events::SectorsPreCommitted>()
    {
        let event = event.unwrap();
        tracing::debug!(?event);

        assert_eq!(event.owner, charlie.account_id().clone().into());
        assert_eq!(event.sectors.0, sectors_pre_commit_info);
    }
}

async fn prove_commit_sectors<Keypair>(client: &storagext::Client, charlie: &Keypair)
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    let expected_results = vec![ProveCommitResult {
        sector_number: 1,
        partition_number: 0,
        deadline_idx: 0,
    }];

    let result = client
        .prove_commit_sectors(
            charlie,
            vec![ProveCommitSector {
                sector_number: 1,
                proof: vec![0u8; 4],
            }
            .into()],
            true,
        )
        .await
        .unwrap()
        .unwrap();

    for event in result
        .events
        .find::<storagext::runtime::storage_provider::events::SectorsProven>()
    {
        let event = event.unwrap();
        tracing::debug!(?event);

        assert_eq!(event.owner, charlie.account_id().clone().into());
        assert_eq!(event.sectors.0, expected_results);
    }
}

async fn submit_windowed_post<Keypair>(client: &storagext::Client, charlie: &Keypair)
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    let windowed_post_result = client
        .submit_windowed_post(
            charlie,
            SubmitWindowedPoStParams {
                deadline: 0,
                partitions: BoundedVec(vec![0]),
                proof:
                    storagext::runtime::runtime_types::pallet_storage_provider::proofs::PoStProof {
                        post_proof:
                            primitives_proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
                        proof_bytes: "beef".to_string().into_bounded_byte_vec(),
                    },
            },
            true,
        )
        .await
        .unwrap()
        .unwrap();

    for event in windowed_post_result
        .events
        .find::<storagext::runtime::storage_provider::events::ValidPoStSubmitted>()
    {
        let event = event.unwrap();

        assert_eq!(event.owner, charlie.account_id().clone().into());
    }
}

async fn declare_recoveries<Keypair>(client: &storagext::Client, charlie: &Keypair)
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    let recovery_declarations = vec![RecoveryDeclaration {
        deadline: 0,
        partition: 0,
        sectors: BTreeSet::from_iter([1u64].into_iter()),
    }];
    let faults_recovered_result = client
        .declare_faults_recovered(charlie, recovery_declarations.clone(), true)
        .await
        .unwrap()
        .unwrap();

    for event in faults_recovered_result
        .events
        .find::<storagext::runtime::storage_provider::events::FaultsRecovered>()
    {
        let event = event.unwrap();
        assert_eq!(event.owner, charlie.account_id().clone().into());
        assert_eq!(event.recoveries.0, recovery_declarations);
    }
}

async fn declare_faults<Keypair>(client: &storagext::Client, charlie: &Keypair)
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    let fault_declarations = vec![FaultDeclaration {
        deadline: 0,
        partition: 0,
        sectors: BTreeSet::from_iter([1u64].into_iter()),
    }];
    let fault_declaration_result = client
        .declare_faults(charlie, fault_declarations.clone(), true)
        .await
        .unwrap()
        .unwrap();

    for event in fault_declaration_result
        .events
        .find::<storagext::runtime::storage_provider::events::FaultsDeclared>()
    {
        let event = event.unwrap();
        assert_eq!(event.owner, charlie.account_id().clone().into());
        assert_eq!(event.faults.0, fault_declarations);
    }
}

/// This test was adapted from a bash script and is timing sensitive.
/// While it works right now, it still needs some work to better test the parachain,
/// like reading the sector deadlines and so on.
///
// TODO(@jmg-duarte,#381,17/09/2024): Remove the timing dependencies
#[tokio::test]
async fn real_world_use_case() {
    setup_logging();
    let network = local_testnet_config().spawn_native().await.unwrap();

    tracing::debug!("base dir: {:?}", network.base_dir());

    let collator = network.get_node(COLLATOR_NAME).unwrap();
    let client =
        storagext::Client::from(collator.wait_client::<PolkaStorageConfig>().await.unwrap());

    let alice_kp = pair_signer_from_str::<Sr25519Pair>("//Alice");
    let charlie_kp = pair_signer_from_str::<Sr25519Pair>("//Charlie");

    register_storage_provider(&client, &charlie_kp).await;

    // Add balance to Charlie
    let balance = 12_500_000_000;
    tracing::debug!("adding {} balance to charlie", balance);
    add_balance(&client, &charlie_kp, balance).await;

    strategic_sleep().await;

    // Add balance to Alice
    let balance = 25_000_000_000;
    tracing::debug!("adding {} balance to alice", balance);
    add_balance(&client, &alice_kp, balance).await;

    publish_storage_deals(&client, &charlie_kp, &alice_kp).await;

    pre_commit_sectors(&client, &charlie_kp).await;
    client.wait_for_height(40, true).await.unwrap();

    prove_commit_sectors(&client, &charlie_kp).await;

    // These ones wait for a specific block so the strategic sleep shouldn't be needed
    client.wait_for_height(63, true).await.unwrap();
    submit_windowed_post(&client, &charlie_kp).await;

    client.wait_for_height(83, true).await.unwrap();
    declare_faults(&client, &charlie_kp).await;

    declare_recoveries(&client, &charlie_kp).await;

    client.wait_for_height(103, true).await.unwrap();
    submit_windowed_post(&client, &charlie_kp).await;

    client.wait_for_height(115, true).await.unwrap();
    settle_deal_payments(&client, &charlie_kp, &alice_kp).await;
}
