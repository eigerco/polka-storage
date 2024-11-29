use std::collections::BTreeSet;

use cid::Cid;
use maat::*;
use primitives::sector::SectorSize;
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
        assert_eq!(event.proving_period_start, 83);
        assert_eq!(event.info.peer_id.0, peer_id.clone().into_bytes());
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
        .find::<storagext::runtime::market::events::DealsPublished>()
    {
        let event = event.unwrap();
        tracing::debug!(?event);

        assert_eq!(event.provider, charlie.account_id().clone().into());
        assert_eq!(event.deals.0.len(), 1);
        assert_eq!(event.deals.0[0].client, alice.account_id().clone().into());
        assert_eq!(event.deals.0[0].deal_id, 0); // first deal ever
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

    // The unsealed_cid, randomness above and some other things were used to
    // generate this sealed_cid.
    let sealed_cid =
        Cid::try_from("bagboea4b5abcaqolcsygu5o756srf7l4pzzagml5r3wa3o6ahoo5vixummsev6rf").unwrap();

    let sectors_pre_commit_info = vec![SectorPreCommitInfo {
        seal_proof: primitives::proofs::RegisteredSealProof::StackedDRG2KiBV1P1,
        sector_number: 1.into(),
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
        sector_number: 1.into(),
        partition_number: 0,
        deadline_idx: 0,
    }];

    // The proof depends on the block number to which the precommit was added.
    // Because we can't know exactly which block will that be. We prepared some
    // proofs for probable heights.
    //
    // Proofs were generated by using `polka-storage-provider-client proofs porep` command.
    let proof = match pre_commit_block_number {
        21 => "a0fc39bb0ac6986d56126fb445a7fa38cca95969db1320caf7bb7c0ad7f9d11f02050c9157669ceb95d44015d7da741aa32f2456eb312d76b863652c6f16a7c3805bb5a25368c59ec4b257394936113e3b93ff2b67211819d0452363b4d37f0416c6b08cadfb22edb20eea6898829631da5523c1fb98804dc5645e9b4c75dc6152f0019517863040463fe351de9c630c8367cc5b9ce8257dd7f6c784152a29199620d697b8fabae05463a6b70ce4bbdc1b16a8d8805951a4cc1ea36c14ba406f",
        22 => "9250a154bc2d75b4c7349f5a6f3da85d5bae71ced091953cf195a8305808e8b87c81d8598dc4b8f1fc9f51a74020267882dd8a0c56aa70f8323fbc1689b09172c2cb7e1782c39fdfcba66a8498b945d33d432e461da06cb23960d5757f8638c3090e97a69e4d66121ef32eeb104d76386ece0bb492258e83e503c5e648c21895d50e458467aebf50c7a9a504ff0d84adb1935a72b82dcadb9826c1f82f5f04094608b2983b48663139697c5583166a02b513609e5755c6613e53ee41f1e45210",
        23 => "a7af1f176e978b97760e6fd703505d885cba6b7dfd60a1f03aecdb52ca3ead6f9b576f8e4b0d4e1bad46a5ee36da3106abd16d6c121d5bfe5bb1a3bbad9f06bfdbcac85caa866179986c81383e9947243a91e83c897d21b4fb0f0c90a5a6dacb04c83ae4bd57595a8b052208e44fadbac7670e17eb470cef0724f838e1c2617f0f0333877398193510bb132520bbe64e964df36bb6e7c2d7e2bd688ad4033d9d010e74be448c1a4b26e83dbf2d3a1235e954e1d9ff9645b8f7948112c960521c",
        24 => "924a59f396c18c26641266366f54ca708c82913b1c91d72285627da22e06c1948aef7895e5d4128d442b20d0e43c91aca40ac8ce0af73f58b3de13a691af1036ed4b477925bad88983cd6e3f685d1d0c43262a3927d3c47380b712a2a1523a4909156fdfee9c3f7e6100cfab9a12936634c09001e5a738975b3109209e3c17d2f0af0d147093ca43481940ec5112431fa84eb003b4d812748cba47bf6632a783c98d684ca297eb3ca67a854f474a12118db57356d319ec2d4be6884f82f2b924",
        25 => "a86c1a73703450e67fef89c5df975ebd79add9a5ecad76f00f3836dea3cd779a3b2206d75ead4044d9d873e470079a408b3bdefdb86a7fc69c57d76f2ef7a4ffb235ca0451d2b5521419bf7cda32916bc10b49c7c0d5d22ade8d33824ec19662112b38bc6dad836a96c5e15ceb7012b920778f984b0425fc442375bcd4c1edcf6def3090b052ece59396763f1e2410b8981c729a63eaa303165ab6adf3fd46cc866790116a5121c71d4c2f6a95e3f94ec4a0229f3fb3aa8667bebfa8579c1a4f",
        26 => "84ee71add7f7fe6f8e0e8da5730400e50e58025cb96fa1e62afca58d46d29c0da7315a6f21c46b05210bd0cf83f6310e9775f3ed2b8586c96c9438dfb801b141b6decb1be026874ebdd24b1ba68ed434f0e89ef654297135d8a27f5eabaab2a911d7015ddd33d8f55aa11e77ddb81afe83a0f06b8421bdc1de659f5c73e033083499a021b858b3725afe967dfd40a7a6a04c6e44112d15d2b8e7a2048c8cdbbcb5d60f6116a0263a5aa7cf9fa67ff736609379973ecbc5300f1b30bc527eaba8",
        27 => "8219438b09845020fc56367326c63e66c1b720c740ff544ac8a3f9d0f726056d443f95fbaf9b9ea70da81ddddc946ad4a28cd8b7653d827a823c0b55932e78318ff5e64c92b00723bd402a83071ebae4ffcca785e0b3846895585ac6be50b75d0470b5783caf883a58a8db37bea3d79a3f8860d9b48010ec79db1a8e0ff7628e09a6fa29cf1ab570f470bcf71db62f579413b5e630a34215d5cf7e2d309fdddd911f1f8f658282439ea764f641d8120aaf0c5c97577f7fead1dc2f9029563aa5",
        28 => "9536835a80b7b6c0f94437a3f89e53034c2fbe778bebdb9fca0bb2a39a72164547da62737874c140ccc26eddf4ae691a8a686228c555eb2a836213518fc94797e9c0187bda5099754283285a3966d99a5a8bf253fe0a9b503c49fbf6ac3d8947143068e8dcde7f945b20c33bac57cddfb2b8305e1e78208db11699860328cb15f44e8c5ebca8f68e3c6a46d45f09f24799d8292a132d6566b2c71abd716c3805171cf8d8a3e670c5b295f8fd37080e2b9fde077daefa3d502ffacdafeeab42c8",
        29 => "8949e95f5d9e4145911ce1d09dda93d640e7677970a008af46f79eef4ff678cdf433aa8bb0675bb1d09b200648b46b9c816bd67cd7be6d9492a5502583e6053f87c2a495583146e81f4564ea730474a1555dd7ab5c0969352514817b7a04fd02079e67233bb1b3735997ccf037f73c7cb6635a027c89d1a6618e1fb79759cfe2fbdc42feb6c0b8def03a3e601362bf97a69c7e120b6e7d4ab11eb064bb94f9364edfae01fcc25ffdcde2b059fc1ced270be3954505557a70b3b02c46f1c030ca",
        30 => "9412416b5fb1bccc10df03ac866245ae9689eae2d7859193a4b423780f1e7ac0c0fa4fa523ce85330229b13d5d691538976678f716579cda00bdab1e127134c5ab86afd73080dda4eff06bbf97cc59a8ca2185a123fe097e0488ae1dc37ada26079442de0044295f77b252a5fad0588d95157cf35055540b5980830b214038628a8f815b0777c9b848703693890fd393a7bc740d76aca92a0306587bf796399f39bd0061b9dc7044973e7aabc4fc46c4fb5525e9188040027df26554c15e2530",
        _ => {
            panic!("Proof not generated for {pre_commit_block_number}");
        }
    };
    let proof = hex::decode(proof).unwrap().to_vec();

    let result = client
        .prove_commit_sectors(
            charlie,
            vec![ProveCommitSector {
                sector_number: 1.into(),
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
                            primitives::proofs::RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
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
