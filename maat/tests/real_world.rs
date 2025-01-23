use std::{collections::BTreeSet, env, path::Path, sync::Arc, time::Duration};

use cid::Cid;
use codec::Encode;
use libp2p::PeerId;
use maat::*;
use polka_storage_proofs::{porep, post};
use polka_storage_provider_common::{deadline::Deadline, sector::UnsealedSector};
use primitives::{
    commitment::{CommP, Commitment},
    sector::{SectorNumber, SectorSize},
};
use storagext::{
    clients::ProofsClientExt,
    multipair::MultiPairSigner,
    runtime::runtime_types::pallet_market::pallet::DealState,
    types::{market::DealProposal, proofs::VerifyingKey, storage_provider::{FaultDeclaration, RecoveryDeclaration}},
    MarketClientExt, PolkaStorageConfig, StorageProviderClientExt, SystemClientExt,
};
use subxt::ext::sp_core::sr25519::Pair as Sr25519Pair;
use tempfile::tempdir;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use zombienet_sdk::NetworkConfigExt;

/// Network's collator name. Used for logs and so on.
const COLLATOR_NAME: &str = "collator";

async fn register_storage_provider<Keypair>(client: &storagext::Client, charlie: &Keypair)
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    let peer_id = PeerId::random();

    let result = client
        .register_storage_provider(
            charlie,
            peer_id,
            primitives::proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
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
        assert_eq!(event.info.sector_size, SectorSize::_2KiB);
        assert_eq!(
            event.info.window_post_proof_type,
            primitives::proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1
        );
        assert_eq!(
            event.info.window_post_partition_sectors,
            primitives::proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1
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
    assert_eq!(retrieved_peer_id, peer_id.to_bytes());
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

async fn set_porep_verifying_key<Keypair>(
    client: &storagext::Client,
    charlie: &Keypair,
    vk: VerifyingKey,
) where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    let result = client
        .set_porep_verifying_key(charlie, vk, true)
        .await
        .unwrap()
        .unwrap();

    for event in result
        .events
        .find::<storagext::runtime::proofs::events::PoRepVerifyingKeyChanged>()
    {
        let event = event.unwrap();
        assert_eq!(event.who, charlie.account_id().clone().into());
    }
}

async fn set_post_verifying_key<Keypair>(
    client: &storagext::Client,
    charlie: &Keypair,
    vk: VerifyingKey,
) where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    let result = client
        .set_post_verifying_key(charlie, vk, true)
        .await
        .unwrap()
        .unwrap();

    for event in result
        .events
        .find::<storagext::runtime::proofs::events::PoStVerifyingKeyChanged>()
    {
        let event = event.unwrap();
        assert_eq!(event.who, charlie.account_id().clone().into());
    }
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
        assert_eq!(event.successful.0[0].amount, 24_000_000_000);
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
    deal_proposal: DealProposal,
) -> u64
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    // Valid piece cid of `examples/test-data-big.car`.
    // Calculated with executing `polka-storage-provider-client proofs commp examples/test-data-big.car`.

    let deal_result = client
        .publish_storage_deals(charlie, alice, vec![deal_proposal], true)
        .await
        .unwrap()
        .unwrap();

    for event in deal_result
        .events
        .find::<storagext::runtime::market::events::DealsPublished>()
    {
        let event = event.unwrap();
        tracing::debug!(?event);

        assert_eq!(event.provider, charlie.account_id().clone().into());
        assert_eq!(event.deals.0.len(), 1);
        assert_eq!(event.deals.0[0].client, alice.account_id().clone().into());
        assert_eq!(event.deals.0[0].deal_id, 0); // first deal ever

        return event.deals.0[0].deal_id;
    }

    unreachable!();
}


async fn declare_recoveries<Keypair>(client: &storagext::Client, charlie: &Keypair)
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    let recovery_declarations = vec![RecoveryDeclaration {
        deadline: 0,
        partition: 0,
        sectors: BTreeSet::from_iter([1.into()].into_iter()),
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
        sectors: BTreeSet::from_iter([1.into()].into_iter()),
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


#[tokio::test]
async fn real_world_use_case() {
    setup_logging();

    let workspace_root = env::var("CARGO_MANIFEST_DIR").unwrap();
    let data_file_path = Path::new(&workspace_root)
        .join("..")
        .join("examples/test-data-big.car");
    tracing::info!("loading example file from {:?}", data_file_path);

    let temp_dir = tempdir().unwrap();
    let unsealed_sector_path = temp_dir.path().join("unsealed_sector");
    let cache_dir_path = temp_dir.path().join("cache_dir");
    let sealed_sector_path = temp_dir.path().join("sealed_sector");
    let porep_parameters_path = temp_dir.path().join("porep_params");
    let post_parameters_path = temp_dir.path().join("post_params");
    let mut porep_parameters_file = std::fs::File::create(porep_parameters_path.clone()).unwrap();
    let mut post_parameters_file = std::fs::File::create(post_parameters_path.clone()).unwrap();

    tracing::info!("generating PoRep parameters...");
    // NOTE: it can take 1-2 minutes on slower machines. can be cached someday, but I think it's good enough for now.
    let seal_proof = primitives::proofs::RegisteredSealProof::StackedDRG2KiBV1P1;

    let porep_parameters = porep::generate_random_groth16_parameters(seal_proof).unwrap();
    porep_parameters.write(&mut porep_parameters_file).unwrap();
    // We need to read it again, as Proof Generating machine requires it in this form and that's the API of bellperson.
    let porep_mapped_parameters = porep::load_groth16_parameters(porep_parameters_path).unwrap();

    tracing::info!("generating PoSt parameters...");
    let post_proof = primitives::proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
    let post_parameters = post::generate_random_groth16_parameters(post_proof).unwrap();
    post_parameters.write(&mut post_parameters_file).unwrap();
    // We need to read it again, as Proof Generating machine requires it in this form and that's the API of bellperson.
    let post_mapped_parameters = Arc::new(post::load_groth16_parameters(post_parameters_path).unwrap());

    let network = local_testnet_config().spawn_native().await.unwrap();
    tracing::debug!("base dir: {:?}", network.base_dir());
    let collator = network.get_node(COLLATOR_NAME).unwrap();
    let client = Arc::new(
        storagext::Client::new(collator.ws_uri(), 5, Duration::from_secs(10))
            .await
            .unwrap(),
    );

    let alice_kp = pair_signer_from_str::<Sr25519Pair>("//Alice");
    let charlie_kp = pair_signer_from_str::<Sr25519Pair>("//Charlie");

    register_storage_provider(&client, &charlie_kp).await;
    // Set PoRep VerifyingKey extrinsic only accepts scale-encoded bytes of Verifying Key in substrate form.
    let porep_vk =
        polka_storage_proofs::VerifyingKey::<bls12_381::Bls12>::try_from(porep_parameters.vk)
            .unwrap();
    let porep_vk_scale = Encode::encode(&porep_vk);
    set_porep_verifying_key(
        &client,
        &charlie_kp,
        VerifyingKey::from_raw_bytes(porep_vk_scale),
    )
    .await;

    let post_vk =
        polka_storage_proofs::VerifyingKey::<bls12_381::Bls12>::try_from(post_parameters.vk)
            .unwrap();
    let post_vk_scale = Encode::encode(&post_vk);
    set_post_verifying_key(
        &client,
        &charlie_kp,
        VerifyingKey::from_raw_bytes(post_vk_scale),
    )
    .await;

    // Add balance to Charlie
    let balance = 12_500_000_000;
    tracing::debug!("adding {} balance to charlie", balance);
    add_balance(&client, &charlie_kp, balance).await;

    // Add balance to Alice
    let balance = 25_000_000_000;
    tracing::debug!("adding {} balance to alice", balance);
    add_balance(&client, &alice_kp, balance).await;

    // Valid piece cid of `examples/test-data-big.car`.
    // Calculated with executing `polka-storage-provider-client proofs commp examples/test-data-big.car`.
    let piece_cid =
        Cid::try_from("baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii").unwrap();
    let commp = Commitment::<CommP>::from_cid(&piece_cid).unwrap();
    let sector_end_block = 165;

    // Publish a storage deal
    let deal = DealProposal {
        piece_cid,
        piece_size: 2048,
        client: alice_kp.account_id().clone(),
        provider: charlie_kp.account_id().clone(),
        label: "My lovely big data".to_string(),
        start_block: 85,
        end_block: sector_end_block,
        storage_price_per_block: 300_000_000,
        provider_collateral: 12_500_000_000,
        state: DealState::Published,
    };
    let deal_id = publish_storage_deals(&client, &charlie_kp, &alice_kp, deal.clone()).await;

    let sector_number = SectorNumber::new(1).unwrap();
    let mut sector = UnsealedSector::create(seal_proof, sector_number, unsealed_sector_path)
        .await
        .unwrap();

    sector
        .add_piece(deal_id, deal, data_file_path, commp)
        .await
        .unwrap();
    let multi_pair = MultiPairSigner::new(Some(charlie_kp.signer().clone()), None, None).unwrap();
    let sector = sector
        .pre_commit(
            client.clone(),
            &multi_pair,
            cache_dir_path,
            sealed_sector_path,
        )
        .await
        .unwrap();
    let sector = sector
        .prove_commit(
            client.clone(),
            &multi_pair,
            Arc::new(porep_mapped_parameters),
            Arc::new(Semaphore::new(1)),
            CancellationToken::new(),
        )
        .await
        .unwrap();

    // in a network with no sectors, the 1st sector is always assigned to the 1st deadline (index: 0).
    let deadline = Deadline::new(0, post_proof);
    let sector_storage = |_sector_number| Some(sector.clone());
    deadline
        .submit_windowed_post(
            client.clone(),
            &multi_pair,
            post_mapped_parameters.clone(),
            sector_storage,
        )
        .await
        .unwrap();

    // Waiting for the next deadline so we can record the next deadline of index 0 as faulty/recovered.
    let next_deadline = Deadline::new(1, post_proof);
    let next_deadline_info = next_deadline.get_info(client.clone(), &multi_pair).await.unwrap();
    client.wait_for_height(next_deadline_info.start, true).await.unwrap();

    declare_faults(&client, &charlie_kp).await;
    declare_recoveries(&client, &charlie_kp).await;

    deadline
    .submit_windowed_post(
        client.clone(),
        &multi_pair,
        post_mapped_parameters,
        sector_storage,
    )
    .await
    .unwrap();

    client.wait_for_height(sector_end_block, true).await.unwrap();
    settle_deal_payments(&client, &charlie_kp, &alice_kp).await;
}
