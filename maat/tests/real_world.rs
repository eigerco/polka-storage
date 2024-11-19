use std::collections::BTreeSet;

use cid::Cid;
use maat::*;
use primitives_proofs::SectorSize;
use storagext::{
    clients::ProofsClientExt,
    runtime::runtime_types::{
        bounded_collections::bounded_vec::BoundedVec,
        pallet_market::pallet::DealState,
        pallet_storage_provider::{proofs::SubmitWindowedPoStParams, sector::ProveCommitResult},
    },
    types::{
        market::DealProposal,
        proofs::VerifyingKey,
        storage_provider::{
            FaultDeclaration, ProveCommitSector, RecoveryDeclaration, SectorPreCommitInfo,
        },
    },
    IntoBoundedByteVec, MarketClientExt, PolkaStorageConfig, StorageProviderClientExt,
    SystemClientExt,
};
use subxt::ext::sp_core::sr25519::Pair as Sr25519Pair;
use zombienet_sdk::NetworkConfigExt;

/// Network's collator name. Used for logs and so on.
const COLLATOR_NAME: &str = "collator";

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
        assert_eq!(event.proving_period_start, 83);
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

async fn set_porep_verifying_key<Keypair>(client: &storagext::Client, charlie: &Keypair)
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    // This is hex encoded `2KiB.porep.vk.scale` file generated in advance by
    // calling `polka-storage-provider-client proofs porep-params`. It is part
    // of the sealer that was used to prove the sector in this test.
    let default_porep_verifying_key = "a0997984403d866cc63bad0c82b4ee5aa317c8c56f614fbd2372f40c143a51e8f6f6de28a414ee8b2e2cf7cd4b598fd78a0112fe5986c92843f4d8ff630b478cd92da824a71b43cbfe4419f3360068302a70b4d52cbc08d54674846929e5012e80474584986b4ba26c30bfdaed8809310811b6ac220d8ffdd9f73f76ccd2c46f373bd7cdf3cc5f6f3912af64aadf48eb1988a00b919f8eb660aaaf63d4730fa1fdd75b094e4bfd6419f820942f9844e2fe0b409b136121457aff228c358b1a90b1d99e9af5664b90230a77f18b0493cb6478db2b432e1ff9af19a95de5233f56c7847fb50aa23e830024b8e641dadefe0f24ea407db3efa0c64f87e880a6428e867c522d504072241818c6cebeee2d0bf7b8db6b8b2720158d5ba5e0336dddf3a7c00a7ae8ad8daaa2ea6ebb1cc675d1091865701aa222cc2fa5759e3f6844671d5ade538979b20dc4c9ae706d23f8ec99576d8dc2f63668e64b35d2d47217a794043b75642038428ee5e85ef09d89d059d773523f4d78c1564875b4f24f7e3512b622f101909296f2afe2f12998dce3058dcc618a0804dec71bab1f486db49b232e5a847c8160a59bba1ec4bb3dc77b0000002897e5512a849178954aef5dfa987ea48a558bbfdc2cde7fd7175a3756e61a6991239b8e3e84e15c123963da74b61feec79952ab4869f5e90d75e4c98e9c9a84cefeb1a3ca58454facb0016b1c67b4323a85a90b6318ff70a9ff3c5540a82088249804645bb7eb1db2a5faefa80bcbd2fec6d2aa4d0f1a44209ba3246aedbb41f098b4e41d6ad4e1054d83a2d460dbd240989c6939e1e81c7e46d336e33a2f7daa9298ebf36793102139d9834361452fb64b66bf40767d7f59083dd4f745a34b39b5e7705315190ce7800a026da47cae95f17f32f2efda66351f6078f2ae54e1e5efaae55dc6b482ec2c7fb9a4bfe6351484ead5ac342b0c8fc492989b1ac89a2cb76b68d7784c45913495950697a1a765c521310c0ca835b30db3da23df48de39af45a9a999056ba8a06c5be7525ff9164b539a4257aaac7c871355f1eb2dd36bca74b52fef03d226b50eb568c960b324a89595282a1c0184a0d8e93fdc7bf22813d76cd12f9eb4609b69c0efccbcb2f7c80c227d7523ecd424386b1f4fb6245589bdb10cfeba577415d7948bd89b7e4a6902630a84cd1ea39b1f53bb304f0150172a88037e766fbaf1f78129799ee816b42250f9e32937e5a367153e3ecb6c8598368ad1cad5936de4ed2253cb45dbb868f8f6274d666fb766bf55022d4b66c6a6bdc11fdf538440afdd1ac28001dfd09ea392df1d03be1f8fe96188c169e1cbe0946a19c96e870a0790e60139e2bdb28a1c1985f7800742e60ad92691dc05554303bbc310f275721587a06b7e6ff04f7efad7ffd1bbc0ea62d160c098b296efb4781b09655cb955d47501f7dd56622fd3696347cd47ed338e2a1f36dc1766a99c934ce699245a67efc3ba74b2bbb5e3b76cf5276e25ecca37080057078fb922386547b5bc66ed3c759b5e99b2111e82d998c583835f0f3fbf09b8f4cd8e05c2b99c2880e473ad139b6265c7ba71b821fd74f3244371bd3a99f82f5619927636e9460d77901100e2ea6c85deacdcd106938bc60f7f90e6eb01ddd4ebd04f17d25beb166b2cd22f47761b5774e4e431260248b018a1cb425348b760538df749d8b38edb7b17552ca10c840d3bc9c9f5f9ac904889dd2ff74dbbedca88e504ce79eaef25c7f8830565e1e7fbb2b98c6d4ca28c9f9670b47be12d15e4a1068e0c5fe08abc177a35c5b4f452a8b91e31db79fafab166a1273b50b8c840e1157bf2a2a2babd2442e3da36303e5b6f7a8c9520c79e582733dc0012b22ae635a41b1e64fb9966ad68d24eaac78ba68699a8211fa3a844ad49dc9f90296095707c2fcf837934f9388e49f3e4980d9ec6286b3d15bd50c52d94d8ae21abcec258fdfafa21a3dfeb80c5ebeac0c3c35f89a3075f24b02515ff0a0e06773cd33d77c7ff10eedb8fc161c4660dd2bee6ef84febfa5208e8fa946a98e591e4edceaf4e8b90443c9aeb127943d20a57b36e4c50dcd95964d7922c90e14c92c09be83c8aa249cd28c7ce2f262b615ec8818e1feaacc76a6726124e79557a345a54f06987c1b4a7f00e291c6d6ec335c82c1e5013a0a8efd96739ed57cbbe830935b1e2c2e94bedf7d7ef14c84ce286745be66888544032dff68552c753e9f82b4c0667854af92fcb0a393fc1423d98a85e74950bee6c457e69e18c178c03d38c1827f92e62cde50dae52af4c37235b0c3ae3c96742d4698b4a2dabc6ef222c18c98870d73d3d3aa241a6bf274c978b3fd3cd5a92907db1be5a5223fb7041206b310b141864f82ae8a0adae378075f09f6249d4280506c44a2e4b7a21b147015174a830bd8356d15bf9272d53d0b343bacff0cd7a3d6a21cb66238041924593ab382fd9487d01a482e9a99da6e452fd84584a0f7fa132500c01eb744ebb0f24b4feb4a9b8d05cf7bb63eafcf9ba879ab1c268328433f703b17092dec7c468702add270238d1113ececb42bc1060fa3d3403c275ad65be19fb7d06cf8549ef010ad83b2bd60f443dd678c4562d54ef28da00bc18a2df6e0bc95dacf2151ec900c4fa08b64ddc46fa9a1c806f9de02d3f361311a2a019c67743b6df5280cb76a6591d8b924aba1d71680114e331f8a335566a7bdb40f25f8518669661d8c6208780f0375180fc538a7d97e4dad41020c62d99672d04e6bc496e8f821e474f1d16339d729267e600903994ad84de162aa08dc3386b9c83f474511d1a704b4978459057d61edfcc0282afd84e903ab4fac2136260ce53b82ef92a84617894d95cbd3782929cd6bab10de4faa243d79f280106ff7da7bef91147f0f72050838e4d9acf1e0c7ebdee31ddb9185c7538ef39a70a01e451857bea2f0b7502206c6ece16407ce2147f98cfa582d7e2f32295d0f073dae27b0d83055308c19597bc9d6490aeb7a6a598cc97ab3a79c46e1e21534f7ccbaaa206aea60dcfbaedeba526fcdc212b5a3f2a3322ccb91e303e1067eb522dc38d651ef46ff69ced4b6ebfdaeff8c6bfb723e9eb33251b2cdd3224e07d84ce477e36c610af13fa303ccaa4d320f935cfdcd42165a2cc034bc86080cd251d35c5e461202351fa1eaf2a460b1f4243983ff23616aeb973fb30848d43d3f15aefd851e9d04a562f4d04ee1b31164aae6f66797cc951c5c7693797bbc7a68fa365b4bc87ac69aba1083a2b96ded24d8dc4e7b67c747cb2a4e7057d3a37eea5785680a460d00026886cec2dfd4df0533b5a13cac6d2ea7ac29";
    let default_porep_verifying_key = VerifyingKey::from_hex(default_porep_verifying_key).unwrap();

    let result = client
        .set_porep_verifying_key(charlie, default_porep_verifying_key, true)
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
) where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    // Valid piece cid of `examples/test-data-big.car`.
    // Calculated with executing `polka-storage-provider-client proofs commp examples/test-data-big.car`.
    let piece_cid =
        Cid::try_from("baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii").unwrap();
    let label = "My lovely big data".to_string();

    // Publish a storage deal
    let husky_storage_deal = DealProposal {
        piece_cid,
        piece_size: 2048,
        client: alice.account_id().clone(),
        provider: charlie.account_id().clone(),
        label,
        start_block: 85,
        end_block: 165,
        storage_price_per_block: 300_000_000,
        provider_collateral: 12_500_000_000,
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

async fn pre_commit_sector<Keypair>(client: &storagext::Client, charlie: &Keypair) -> u64
where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    // The unsealed cid is calculated from all of the pieces in the sector. In
    // our case the unsealed cid is same as the piece commitment. That is
    // because the piece takes the whole sector. If we would have multiple
    // pieces or the piece would be smaller, in that case the commd would be
    // different.
    let unsealed_cid =
        Cid::try_from("baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii").unwrap();

    // This is the height at which we get the randomness to derive the sealed
    // cid. Usually that is done by the sealing pipline, but here we have both
    // hardcoded. This height is passed to the pallet so that the chain can get
    // the randomness for itself, when checking the proof in later stage.
    let seal_randomness_height = 20;

    // The randomness received at <seal_randomness_height> used to derive the
    // sealed cid: [162, 13, 84, 200, 249, 99, 34, 176, 119, 98, 24, 201, 104,
    // 246, 249, 160, 8, 202, 132, 1, 205, 231, 49, 145, 195, 28, 231, 104, 45,
    // 13, 151, 107]

    // The unsealed_cid, randomness above and some other things were used to
    // generate this sealed_cid.
    let sealed_cid =
        Cid::try_from("bagboea4b5abcah2xpzls5hsuyngd5onktkgxfdsltfgroxk722oi72ipbo6jiwig").unwrap();

    let sectors_pre_commit_info = vec![SectorPreCommitInfo {
        seal_proof: primitives_proofs::RegisteredSealProof::StackedDRG2KiBV1P1,
        sector_number: 1.try_into().unwrap(),
        sealed_cid,
        deal_ids: vec![0],
        expiration: 195,
        unsealed_cid,
        seal_randomness_height,
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

    result.height
}

async fn prove_commit_sector<Keypair>(
    client: &storagext::Client,
    charlie: &Keypair,
    pre_commit_block_number: u64,
) where
    Keypair: subxt::tx::Signer<PolkaStorageConfig>,
{
    let expected_results = vec![ProveCommitResult {
        sector_number: 1.try_into().unwrap(),
        partition_number: 0,
        deadline_idx: 0,
    }];

    // The proof depends on the block number to which the precommit was added.
    // Because we can't know exactly which block will that be. We prepared some
    // proofs for probable heights.
    //
    // Proofs were generated by using `polka-storage-provider-client proofs porep` command.
    let proof = match pre_commit_block_number {
        21 => "ac7b0de077236494913f6c5a0af70b1a5ee7d2c423be0a3555a964a9afde64ce712b23f8ed2d3a6e30e9a0323a9e3c5a9171ec98fcc2cd0ab8110e0e1aa764b627015342eb252d90d2b05816a6e5456876d71fb7e8fb066cceb8bbab8fb1fb1c17c42bf359939a3064a28d78c0258f3b2b7cb6813ab7adaf259d2b0a48c8a5b29b8417f8bea3e56ab14d4999e077f22fa9536d9311ca05ab786de590685effc313be15f52438cdb09fe96ba9917c1c201914edd17b1c9c20f618fa872c7f5845",
        22 => "a329f2b90dfd87d1d5e9c30e1cfd298491d0334c2307259d4b4b4a261246ae1f46c0120b1af1201415e43facb1c0943d9087a80922b1362f045b715d089678f54c76deee4d12c71935b6ee01ef04c770175f6471e81296793941ed75d7c60b92005cc877c255d3c0c78d530b42e04b1e1b3349a5a912bffeca9a06d229077fb8f6c08e4b3947e5d8f370b35d8bfbeb65b2da6ecdadc5b0addf6186232f5f559f6cf1fa9f93813da4360a9b1554e13b49ed58a4ea91f8399701e94ebc26bfea12",
        23 => "985a6b3cd7564ad2a80f086aa2d8f332654c41773571babf24e66584ecfe2a76d5100146c4808767fbb9b50d51de962fa1db255430fb9faa5d0011c5b8b7ea0d5904fc6f75c8cb20efb266d2af867f02a30a78bc2f026f956f7ad71ba25c9e3414aa566f98749e7c9424b8b250d63b33a29972ec6eea1d22cf0e9d2662905c4042f236cc741f07b7611392d17d3cfd0fa0202cc9fcd3137433e66ad73649ee0bfb1b97ffdb1bd5ab66704ac30ab7ea769eb3e8be12401724f9ee5f740cbcd0e8",
        24 => "b0f777807d02c16794e6dc8ca2a27aa97aa88f7590bee9c6f8f5da388ed4a7d8adbd2f5c0cfbfeb23aaf8e4b9de47b65a9ac7c52a89c97d280afcda2fc4459a2f87d74501d6008bc87a3f40f760811edeb48d29fc4113867bea569ae5b1485e9112e77a74fec5789542fbaea69fec79152ae6b7dbcf4520ef98df426afa11c5afda21b09dc00f7153fdc5760fcd2c52586eea468ff43a6ceeeb64cc62f15371bbdaee7b7c72ac6277587b3a32638ffae19b9c7af9a80299dbe956e520119be8a",
        25 => "87c8128bc80baf631102b8f4b60b98fd80d2ea1b6804c2c399ee313aa709147663c8a51ad5c2d7c80005272792e1e223a96d8675ccfe8b5ed1097e32a77bef8f0c1b6b61a5fa77ef9faebeb566561267273a7a4b9f73634b968ca1315b85dc630880a04eae5cc20482752f20ee8d388b429e31adc4c519547fef6d2eff88aad9d6e8a1110e6c7462cc5ab9cbc84f3d4ba9654e9e34bf83960dd3cb73684ca42db1471f643aa38e3b98d3a587b523139b4b9ba1b22004af6a443226c440d8255f",
        26 => "8558c970b0ae521c8af16b76f8ac85aa484670cbada153fffc7341515946b19083d8bb7fa03cbc63031c70770511a170abc5f0f1b3083e82108271e3d2b63d8751997696804893caddd1fbfd5f4d5fbf66f0f8df34b18b1dcb0787d70efddde713b9b331fc669d775b7a9ccd07b4fbd9418af78368989fcd503446ca47cb35631cdc4f64ca56d9715cd918d74b4ca01f81b1ea2b4868cc7f7736afd314b8eaeb6f04d9dd996fc181210ece0f3c34ea8588eb1f53f734345309bb800fdc31256d",
        27 => "8d978855236bdaf82c44ba1f0b8ec6451c8c8beda6324f7ff85e15b86be8bdfcb5790d5a8902114f187f9e8c59afe0f891a01fea8b25c583f1f4759da3240982565d99a5d63297d6a084e29b26333577e0a1d37d6a817ca5ae0ab5a18629057115b9c5601db47d262fb97946488119e527099dcfda0c0e598dc03d1875ce2cefc96260805627670605f66c461f66b04688ce66bb0dab8eec4f123fc0c50340eb4c48dbfbd70a82db65661541c8273dc39ddebdc64a5c59e708fcbb870a5711b4",
        28 => "90c459ab1cb5300faa8bf75653e455a04e8dea8e5d320e33b0e338272a4800d97995ea1985a3e21f791c15158de38b5499506bc5d4edccb0164b878842ac31fb6e412709f31d77a7bb61227c13d648326cfed86ca5c468bb0d9033a9e3c48fb5082cdb36a5e63f94341ed3b855846b1ca5970334847911b7fcc9ca7dcba4596a060854e7fb8c5dcd3d7abf8e480333658b87641e13444ac740dfd7724f5696790faee4c7c9fe2cd1568fb5d6386bc70e82078030d07857ab80086b494f8aa6cc",
        29 => "b5dd7ae4c6a3bcf093afed88a3a23ea1783f9ee85cbd93d24d725580017c7f502e75bcca06ad866e576b3f7fd312ff5bb24909a79f99f416873b2c154eda37646229e5062e5fcba4f996a5d86ef36064cfd76f2cd7c09a6d67541852fe2b6f351620f18bf43c59d6b4c34a6a6c62103383bfb6fe374773cd64eea21980fc33b7cd29ab521902f0cf822240622936a341afc85c0928e6b035f0b44f470d87e8974bc86356abf4326ee2d0914a2633c53fcb3ab84f439a4615f4621c883d974a72",
        _ => {
            panic!("Proof not generated for {pre_commit_block_number}");
        }
    };
    let proof = hex::decode(proof).unwrap().to_vec();

    let result = client
        .prove_commit_sectors(
            charlie,
            vec![ProveCommitSector {
                sector_number: 1.try_into().unwrap(),
                proof,
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
        sectors: BTreeSet::from_iter([1.try_into().unwrap()].into_iter()),
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
        sectors: BTreeSet::from_iter([1.try_into().unwrap()].into_iter()),
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
    set_porep_verifying_key(&client, &charlie_kp).await;

    // Add balance to Charlie
    let balance = 12_500_000_000;
    tracing::debug!("adding {} balance to charlie", balance);
    add_balance(&client, &charlie_kp, balance).await;

    // Add balance to Alice
    let balance = 25_000_000_000;
    tracing::debug!("adding {} balance to alice", balance);
    add_balance(&client, &alice_kp, balance).await;

    publish_storage_deals(&client, &charlie_kp, &alice_kp).await;

    let pre_commit_block_number = pre_commit_sector(&client, &charlie_kp).await;
    client.wait_for_height(40, true).await.unwrap();

    prove_commit_sector(&client, &charlie_kp, pre_commit_block_number).await;

    client.wait_for_height(83, true).await.unwrap();
    submit_windowed_post(&client, &charlie_kp).await;

    client.wait_for_height(103, true).await.unwrap();
    declare_faults(&client, &charlie_kp).await;

    declare_recoveries(&client, &charlie_kp).await;

    client.wait_for_height(143, true).await.unwrap();
    submit_windowed_post(&client, &charlie_kp).await;

    client.wait_for_height(165, true).await.unwrap();
    settle_deal_payments(&client, &charlie_kp, &alice_kp).await;
}
