use codec::Decode;
use frame_support::{assert_noop, assert_ok};
use hex::FromHex;
use polka_storage_proofs::{Bls12, Proof, VerifyingKey};
use primitives_proofs::{RegisteredSealProof, SectorNumber};
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;

use crate::{
    mock::*,
    porep::{Commitment, Ticket},
    Error,
};

const TEST_SEED: [u8; 16] = [
    0x59, 0x62, 0xbe, 0x5d, 0x76, 0x3d, 0x31, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc, 0xe5,
];

#[test]
fn it_works_for_default_value() {
    new_test_ext().execute_with(|| {
        assert_ok!(ProofsModule::do_something(RuntimeOrigin::signed(1), 42));
    });
}

#[test]
fn ceil_log2_computation_same_as_filecoin() {
    for n in 0..10001 {
        let n_u64 = n as u64;
        let n_buckets_fc = (n_u64 as f64).log2().ceil() as u64;
        let n_buckets_we = crate::graphs::bucket::ceil_log2(n_u64);
        assert_eq!(n_buckets_fc, n_buckets_we);
    }
}

// Temporary. Will be frequently adapted in the follow up PRs.
// Default, because we are currently generating a fixed proof.
fn default_test_setup() -> (RegisteredSealProof, SectorNumber, Ticket, Commitment) {
    let seal_proof = RegisteredSealProof::StackedDRG2KiBV1P1;
    let sector = SectorNumber::from(77u64);
    let ticket = [12u8; 32];
    let seed = [13u8; 32];
    (seal_proof, sector, ticket, seed)
}

// Temporary. Will be frequently adapted in the follow up PRs.
// Default, because we are currently generating a fixed proof.
fn default_comm_r() -> Commitment {
    let hex_str = "70e82695a03f9dbd96e568fef583283266a2048766e2c7abe78dcd9896996d68";
    <[u8; 32]>::from_hex(hex_str).unwrap()
}

// Temporary. Will be frequently adapted in the follow up PRs.
// Default, because we are currently generating a fixed proof.
fn default_comm_d() -> Commitment {
    let hex_str = "129c7562bb0c189544f5dccd365feaec2141eab458097a5ca8429c109d154421";
    <[u8; 32]>::from_hex(hex_str).unwrap()
}

// Temporary. Will be frequently adapted in the follow up PRs.
// Default, because we are currently generating a fixed proof.
fn default_proof() -> Proof<Bls12> {
    let hex_str = "82562826f01227d97399677f612e134acfe13d70e7fc46bae5ea94d19d9f291f533d06f94b6e9b0e18b856750d4270cc90d5eab93eb8d8dd73bcef50de0e009644f74ca2222bc9732ccd5f6417dfc8f9508e7ecea6294b11f0d47a2974f2a0a40eaeac501915626f9c6a964be1995b1f0f21e306f8513595bf68debb9a59efe50c8719fd55c09f1a20178aa55895cebab7b9c36508d08405d47b5862df9bc0c84ba764a9ae1730ed1458aff0744c75fbc88cd7382861d046554c1b8ea8408fc8";
    let proof = Vec::from(hex::decode(hex_str).unwrap());
    Proof::<Bls12>::decode(&mut proof.as_slice()).unwrap()
}

// Temporary. Will be frequently adapted in the follow up PRs.
// Default, because we are currently generating a fixed proof.
fn default_verifyingkey() -> VerifyingKey<Bls12> {
    let hex_str = "17efa7d92aa49d0afb02515c26e3a8122f383ddee6e0b12a8d23e01e19b82ef3b779972d7afca8cae127f68781df0ea6150705f4893beb7c69cc515031b2ae98de9ea7a28c6b102abb3cf91cd2b10eee5e6b8d9a3f9db46ba16eb3bae38790d300373b573c6a4559513b67a65f653abedb6179b07d193966c4cb376639af218f77210014736fa59c23829f69f962c3d10b33326c0f983cce9ede68abf6e8356deeaffdcff60a1987032c3e9cbd0a2ff16c2570d50ba8aa3f9d08b98a8d1e4b150853e3965125dbd967d204ef100fa96cd6c27442587d39315e56e72301eaa4a538d99a7e711d0d224e17cf00d6e710930bcc49da93357f4f8c9801e30ad0cec7ab09e68b411ac8711da8c61adffccd86069e7c0ccdfc0d0467b3fd1b34a77494073dbf95b43f493e1d87d0331c98f9b6995169fc0e6781328245e9d04f9a0532390b8ee88c5ad01a238c2042564a926117a3a3de585fe04e2bb2432650e4b252eb8bcb84fd393bc95da565f7f76ea0c07808763a938c4e13adfdbc47f500ad2d173c6e343ad421e24b7831a0c7a2f1293587b8e14b4697a8581f09bac122f2eecde57c5ed41a30cd15c8c694309d07801740ddf6fdba4b9bc43ea4d056bf365bb2d60719195c551f74360a16dca2778e9a90eee25c2038cec02f63a70258cb100ed592b6cab8bdfd0d0780546cba8ff73ce54bded23ef0667e3150eacd814e06fd8eff7a93d5b046e865ab9dffa1320d02e907333af7e2022fcb91710d8bc62380665a8c8e73bdb3a43d1d429c3e9fea253abdd81b32597a3d88c1d659adb4fe0fbbc19b7e130d818dafbe04b6cbeaa613e29ba3d5201d1608a3f9dbdc6c4f3e22d444e16662bd19c1e4d35a18fc407d07a28fe01d0d14510527a8a2a395391ddf5d57a405b5cbbdd0ed5f2d7264b69fadbbc806621a36c58168ceffb75feae511d289ee2b6be36a3bf0dfa52ddbe943ab36d0ab82d865724b644452834eb80a2db2ed6b781702ce8c1ddfd30b950949067e497e58c9cb78a005655e6f7c2fb842c0396ffd00dce46221b181d64f492bd8a6b8e8abc16aac28ba3d131f316ed901e0bdf1f3ae61bca5a9bfa02eee6ebc83a410ef577f3a44c9f7480775282cbf0187851cf4f823ecb9908ccf929cd31c038479837f71fca4cdace3cba1ffec30ab59f81e942399da863bce930ce183a0c9afb3067e7d98b64f665af5ca335f4300000028171514e8dbbbba9095b76c492bdb7d04d0dd91bd6f6fa916166b38037dc95fa6a0033e1907bc1afc4467e26f681ea00917d8787b52da92b61988144cb87e87840650b8ff6979cbc3571bd47fa6b634b7ce19c59569f7009019b81b692fb3c346115f4412f7b5d6348fe32705e038b56b21755de506923feccf1df704fd403b647726686d9b7130f1c038720d64a37337023228bdd7c89be4b80cd58a33f6448098db02dda66ae940e2b85ba43d4c56e6217b55087c3a922658b7183b7a805d90073b5df9330d493855054bce38ff69215afe4882538c1d8526ec465cd016f3eba6b19fa92da131307daa77548824c1aa1366c152f0e77b779b2642d6c14b48831d81cab043e3ab9b3ec4b03a0d028431755f3cd1abac58bb20253ebab72e71bd15c8083a9cc65ac5c6b400e200e60ccdac3c6d5a198d833990a5a7d78fec0edf99df7fa492aeb07f40b907c62e79b35610c61b7fccf8019231988db540fb21f8f53f9c2d52e1b33f23e4529d2e6f3b19e1c1a1b16c6fd2d7f0036e02d028ad5d1100b217ed7a9c6f3dfc2f3865c6e4db335805c917acbec6dfeddc99234dc84c7fa2ba5dd99e78bf52582e63c707c7bc10cbd23ba617b7bfcb80c5ed04b7f7d3e7fef43aea48912c3ef1cadad3fa6e3707faedf26ac80f129c8212ba492e6a170fc13172962552457d55af99bd534cd5df9fd4bd74bf1811d0e8f3b0c4365fb28d730ba178501f8514fbc9b5bdc90a580f1287d71f16a00826d8000b71ccdd5a6aa8c3f6cbfa4206fb2b928227a661197fe1222b3658efe30c97af6f52626b53074e1d10af3cd818cff8bff3cd93a4e6d68e36e46573eeb9f1c1c940757f7a2af55ad5c01b25cc372fa639c46e144b120b2d7f258ebaf0b2c21ca8fc4e32b028915c7cdb1d3aedd289db53d8eb6dba131b51e3d1ed75e95ec0aebb72fdc5e1e9076af263fd580e665d325a164f93dac6356002d0270e05e6891ce1e1b51ab6fa9c56cb8f7cf52db655d6720a9bfb47fc06fde8900710d42c9c01b89165b840bcc5c7b8fe8620f6a2e51fc147243a88a814a8fe9a53bee145ae5b848b26afdbea106f7bd350fe9c7e747d8328cae1cca32672cd26ac947afb775e223ddd1560de6091f87b42fd6fe8cfbe0a2ddfafcbde05341439616eb36369b7dfafce61e43cdb7119d240780b79dc37619e93967e39e5f58b2796aa2601587ef457f234d2310b8739bd0b95249dccb969f6dd3e2327efab81ff004651004c84b300621c985164b56f953a79e60630fccbe17ce5690518154cbb735b31cee6e27d2b9ad4daa0feb2167050669af66fdefa1871db9111bdd48fb6b0621f28f82166d48ff50d1b040da582ff8817106d5b6a835e9853be703af4809fdf5f9e75653827cd2e7643a24c69cfcbdc069ca40e842d46d171e5089ffe1c436fd1be4e86565ad464ba8e3a203a74175f54b07be5a5133cd43e480b525f2604c6d4242c2e1bdc458d01d01949efcef38864dda0e681ddec3ee5a7187653b61bb3189665956f8fbfd70932818391bb1b844f1740547899e02ef9a4059d423904a1ce684768c2f9af57f5ba369f72a992ab210799ccaab48179bbbf7ac915b4be002178dacc3956c72cbc6b1205484f3cba8d4eb8ea9899a738652cdd25251cb12a6c131fcc0672ed1a6f748bd5a866a58eceade080ae38a8aeee850c03f129fda133e22ff21b9b548dc789e26048e41fb07fdf7194cb44503e7f837ae13682a14a23831d65352a667bb3a802701263a04dde867a31560628e517714d6ee4b6b3412046e9028b05c4c730886f496218a616f325b53c96fe83c65e190bcd7692b6f21c63175a334536b155fb7dcd13f830d4bfb0e4ebf84dda50ce76a36b2eba5029ab2e08b11bf08f088a34042c5fa557d18ca3bb7cf8a136ae782ee03cddfc9b10404bc3a6cfd680a41040c9b5102666e6de163956e59f1d4c70ca0d2ff94f0d3f482ac44d3fda04fa50232789f2736392cfde1d30ea0454f843f5c2d01a75b52a14063162b3410428fa2a153e453576670a9b4b7c7c613944ed873a42d22e3ceecc91f16e663e642814977a261fc8c1eb1e2ed84be6f5f9e7605514d468388c8d74f0e607b4da27823b09b360612837964c6684017a0d40bb258ac48dccdfa7bc65997cf622243c9449f003e8814b31f48b60b3bbd1e54b9bc2ae4da6beeb3741e848d5243d49f84ce5ac2625b7cc4b02fa08c11c14aa401a97a40387563f44393a3daa2ef0bb4e68e9b73ae65f20aa01f26a44f69414e670ba7ae8c60c764434ac0d75ae2b61ba99cf8c079343e808275db02422f8f72ccc17a781757a4ca0bf6a73820ea01f92a4ab12428cd465e35afbcd7814367abfb9df7403a88e289e2569eec04ffc50afd610a6230796da85bc3b6cb3819f20791005a60bc15e5b1ee8716dea4fcb109f7c1223105a152e212d3d1504b79e3e09207e3d35613eb24171e92cf1670ca6c1aac359e1463f379531a9a00fa5b8cbad3d53a312f8cec0c0adebe2592dfda5cbfea28eebbb0547ecef4057c4f1091bf1350d0a563d2bf5372f5f281cdceb93968a27490e37b483b81c0de14a1d958c23587245f5370c80091b2702191f596689fbcccf4b75d3b5a2f4fd5e5872522953d21cbb0434da5f3549df4a026e270cff360f89fcf9db7f6b7a38b42fe3e222e9ebc1b45fec4fb20a68e1f92bce9308d7c0edf90be5b50476fac3619216e78685a9a03ccef030c240ff1ee3339e577135314a81fd6d299bfe892a59b53d691466c612670ba702474349efbcc12175e8890be7c55dddb65396484791de51698f1cc162774c3455a4555904afa4a3861a2b345751157502edc0660ac11500673fdba4c9cc21112a35f4ca25099123a5b7f0adb79a539fbe8a8ea01c36c9aa4166dddb38d106eaff0fdebb9b5d43576e01d4694086669f5e6a502e55f27f02c8d2a60beea06ccf2646e51873a12238624998260d500bfc094115fcf1a1ac0ceaf9c936087da39426dbb948da1ede187c6be993d778436bf8e242c68225b2e110cbb3c9a6e00899bee9f077186b8cd372ad53b778ec3edc70dd015be9fe9cf67c7b2b2f58d937971235f881357359d56673daa73d4f0b415fcd7970002b8d856a2594082e804b891bbc1d13b9bd7946d364d18699a9cf4d5a6aafd59cdd75ac17bb9572fdaf0f1fba68a2a006c150a368856a70df59548521c69fe8a9df2e2207ed3a7d24c1e9942c3ed24c4416a97d7ffd915088e8176b2e9197c9c9d87c0f81a8809b79c098846396d1cc7c17664e4274477f4f3c4e136618307a1a2b9946f7af4c29f4f2134d7f7cb3bc7ea69b8d3436e3bcda18dc95bcf1d437b1e43382beaaa6a203db2d6aaa9d733c21f2c5cc3efadb0b149e0dc8d4d84630858561e2c9acda6d23bc697b882bcd7018f534963bee10eb6e331fd5daf1d7831bebe7745f490c0f477b0800da9d5b28affc69701a0ea3aebb791836917a2d7764cbe2a627d2d3413d6f55cb193d7a9c041fadb1b9302e31745a05f9cff5c95447f6fd124b48fff58176988420974e36751f0aa03dc4540e6531a8008b58407e92f7f8b6bf0bab84e3ec0d9f9bdb15bd4737bc89a878797670c7e8a276a9f8b9d7b7614b47efa48f38e56c583109a6b896c97ec36eae6942b42812fbb7cfe35eec7efa10197dba80a7eb8d778335052977d1a8ea5ca32e3c97d90a8018dd2bef342548ac675c324ad3d905cb1d81caec203721ea2fab89dc66a55da83c3be3250e68cb998c7ac3f62f1ab1b3456fe66bc8d3392240e1eeb5ab3e19073f84e94ab1a1d7e66bc004f43d1037c6dc33b5001f04badf01bb9bc036b51621b64e13f21ddac189e02c8e2505cc16edbd29d81ff47fe3636facf3b9dfde16f0d10657db7c60a855aa5766e7d95633741961ea229e3b80b7cd3773fd08cd0b02213d601fd9be47381d7aebe6ff47fa6f858866a6cf8a5dbf2f014e5cd46213febac3bae8195b2b53f3d0b27c1fb118d4129ce4d2470cc06e4d3fe489003b81fc13314534ccfdd2f4fb219a6a4af1ca7c4077b23fd5aa5dbaf424f2050eed0c36585170486f253913fb45c38b3d4c9a2cc019dde38c7023f5776b348e54cfb9874513515ddbcb654f6573485938ab02538e8ea60c45fdbf49da322c02ad1cafc31dbc1b66a8b6e9e2da62061bcff40809843ba07b010243ad1be6a244fdda0ee0f2a43551c7f96619d9a78b6d9421adedd9d46db4eec50e52fc1833b849681baf78c5a585063db2fa147fa730220e0562c80a18d2871c076951fc859d0807dd61cdb1c39c319b85dd2349b8ca39fa708732b0f2fbd0c353fcaf9212e232790439c80e4fe6b040ba87f67ee54d2c0355799cc63b2c90555db1be1a8a55463ca5d6ad76c59e288445cf0b33e65f85710a02da9b1adb5280065d915b1e7725d6614f871cbbd690b747e34920c8d7a45a75cc00c323f7020c2466d952af9082ef02ce47fd0dc8874761fd32a9a2a2bf60dd71a47301b85742a1f6a415e5a111493e5692df5de8e9105d4239be95a4f6b70630e685a1e448838e151cab6106948ab34fc3e62b0c69ce1e721298bb83199cd26060042e8bab6efbd20e4a4521e5a6014c3025f0d0e10e9fef14c4f4a1120198e69619faa883bdaf4c8c7da96c2e2435349f9fd0c259f57dc6399bf03b476613ba7bdb245f8915a2871f3890f3530d1c81768850e5088a3d0775e4ad932b423e8cce2d1cb0b281c639d8d362d71e2201173989e39a98ae86b1e70975f07f92cfcdcd7b6fc0d06a7680872e8d5459117307b2dd87abf6b4634ef276a56ed691038ae36d08cba80df60240366c6074aae0b9033bf5b8a0a93b800ca9c7d81119175158582db5c43444274686a32755930d7e1c32a922adc87d6c83f0069606c2502f68ddab9269e5ed173df94b2421b77600d8dd7ab1508908d17d83ed7e41b90f2e6849b693bd9ffc29f9a7892cc6d2c2dfd51be5dd7b6dbb6407104d9590aba14d961338ac13a51914dc921fe142630b45c14fe2b9438870ac16148cc9901c36fabd63892b111b55fa4dc76e837bf7142430a7eea9787786da79fb1652558816deb2021010caeac859e37429b559518c7e7bc8cce7b0517a5bfd56eac5488b0923502f89e9af260148872205fffe6102e2284f7cd03bf2650b84285ae8cc94bb3934f8eeba660ccfe4d9b5b0737699c1fce9a5b9c049ef72404bdffdac5c2a143fc0155528216c87ef6777fc8175bda3f388411dcf3a5655c0d16da58aac36db1213d3260bb70c627a3e73e221dfca0467713ebdbfa66e4313fa18217a3319fb2a34ae9b4e3d4b70b6dd1b94fd5f7245362815920933135d4db988742321760dbcb90ba8d0c7c48c1322e004155f6eaa9f2d34e754c305899d1c5c366cc3825ab5ecb4c537637d709fb6e723e0611f";
    let vkey = Vec::from(hex::decode(hex_str).unwrap());
    VerifyingKey::<Bls12>::decode(&mut vkey.as_slice()).unwrap()
}

#[test]
fn verification_invalid_verifyingkey() {
    new_test_ext().execute_with(|| {
        let (seal_proof, sector, ticket, seed) = default_test_setup();
        let comm_r = default_comm_r();
        let comm_d = default_comm_d();
        let proof = default_proof();

        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let vkey = VerifyingKey::<Bls12>::random(&mut rng);

        let mut vkey_bytes = vec![0u8; vkey.serialised_bytes()];
        vkey.into_bytes(&mut vkey_bytes.as_mut_slice()).unwrap();
        let mut proof_bytes = vec![0u8; Proof::<Bls12>::serialised_bytes()];
        proof.into_bytes(&mut proof_bytes.as_mut_slice()).unwrap();

        assert_noop!(
            ProofsModule::verify_porep(
                RuntimeOrigin::signed(1),
                seal_proof,
                comm_r,
                comm_d,
                sector,
                ticket,
                seed,
                vkey_bytes,
                proof_bytes,
            ),
            Error::<Test>::InvalidVerifyingKey,
        );
    });
}

#[test]
fn verification_succeeds() {
    new_test_ext().execute_with(|| {
        let (seal_proof, sector, ticket, seed) = default_test_setup();
        let comm_r = default_comm_r();
        let comm_d = default_comm_d();
        let proof = default_proof();
        let vkey = default_verifyingkey();

        let mut vkey_bytes = vec![0u8; vkey.serialised_bytes()];
        vkey.into_bytes(&mut vkey_bytes.as_mut_slice()).unwrap();
        let mut proof_bytes = vec![0u8; Proof::<Bls12>::serialised_bytes()];
        proof.into_bytes(&mut proof_bytes.as_mut_slice()).unwrap();

        assert_ok!(ProofsModule::verify_porep(
            RuntimeOrigin::signed(1),
            seal_proof,
            comm_r,
            comm_d,
            sector,
            ticket,
            seed,
            vkey_bytes,
            proof_bytes,
        ));
    });
}
