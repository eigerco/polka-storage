use codec::{Decode, Encode};
use frame_support::{assert_noop, assert_ok};
use hex::FromHex;
use polka_storage_proofs::{Bls12, VerifyingKey};
use primitives_proofs::{
    ProofVerification, ProverId, RawCommitment, RegisteredSealProof, SectorNumber, Ticket,
};
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;

use crate::{mock::*, Error, PoRepVerifyingKey};

const TEST_SEED: [u8; 16] = [
    0x59, 0x62, 0xbe, 0x5d, 0x76, 0x3d, 0x31, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc, 0xe5,
];

#[test]
fn sets_verifying_key() {
    new_test_ext().execute_with(|| {
        assert_eq!(None, PoRepVerifyingKey::<Test>::get());
        let vk = default_verifyingkey();

        assert_ok!(ProofsModule::set_porep_verifying_key(
            RuntimeOrigin::signed(1),
            vk.clone()
        ));
        let scale_vk: VerifyingKey<Bls12> = Decode::decode(&mut vk.as_slice()).unwrap();
        assert_eq!(Some(scale_vk), PoRepVerifyingKey::<Test>::get());
    });
}

#[test]
fn verification_invalid_verifyingkey() {
    new_test_ext().execute_with(|| {
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let vkey = Encode::encode(&VerifyingKey::<Bls12>::random(&mut rng));

        assert_ok!(ProofsModule::set_porep_verifying_key(
            RuntimeOrigin::signed(1),
            vkey
        ));

        let (seal_proof, sector, prover_id, ticket, seed) = default_test_setup();
        let comm_r = default_comm_r();
        let comm_d = default_comm_d();
        let proof_bytes = default_proof();

        assert_noop!(
            <ProofsModule as ProofVerification>::verify_porep(
                prover_id,
                seal_proof,
                comm_r,
                comm_d,
                sector,
                ticket,
                seed,
                proof_bytes,
            ),
            Error::<Test>::InvalidVerifyingKey,
        );
    });
}

#[test]
fn verification_succeeds() {
    new_test_ext().execute_with(|| {
        let (seal_proof, sector, prover_id, ticket, seed) = default_test_setup();
        let comm_r = default_comm_r();
        let comm_d = default_comm_d();
        let proof_bytes = default_proof();
        let vkey_bytes = default_verifyingkey();

        assert_ok!(ProofsModule::set_porep_verifying_key(
            RuntimeOrigin::signed(1),
            vkey_bytes
        ));
        assert_ok!(<ProofsModule as ProofVerification>::verify_porep(
            prover_id,
            seal_proof,
            comm_r,
            comm_d,
            sector,
            ticket,
            seed,
            proof_bytes,
        ));
    });
}

// Values hardcoded in this function are matching the ones in https://github.com/eigerco/polka-storage/blob/9433eb81bfa76a30fbac1f8f79101ab6359f4f3e/cli/polka-storage-provider/src/commands/utils.rs#L188.
// This is because those values are coming from the clients of this pallet and are related to the proof system.
// Prover and verifier must match those values when verifiying and proving the data.
fn default_test_setup() -> (
    RegisteredSealProof,
    SectorNumber,
    ProverId,
    Ticket,
    RawCommitment,
) {
    let seal_proof = RegisteredSealProof::StackedDRG2KiBV1P1;
    // Those values match the ones from:
    let sector = SectorNumber::from(77u64);
    let prover_id = [0u8; 32];
    let ticket = [12u8; 32];
    let seed = [13u8; 32];
    (seal_proof, sector, prover_id, ticket, seed)
}

// `polka-storage-provider utils porep-params` - cached here, because it takes a long time.
fn default_verifyingkey() -> Vec<u8> {
    let hex_str = "867a45c1b8b279f6a76fd8bff20b33d876777b4f36884efe03c4ea24e597ad8c8474b0a059d17e6b99b34e2814fa10a8b880a849aad92ac140f7ea3c301db867827eb031d9555b68eb59cb9a49eff2dcc1c3872eedb4a1c40e47b1135b4d0b75b8d7c67e06e024de81b3cc09025235a7ee5e24b01c826216831a0d0c88df86ab7c2a45d5cafe8f9c477e0003a27c3c2107311ad240bab19fc8e2109c6e1c7d64c745188d507ba6db1baa372713c8ab3f54003a1c89a19b902d9a7e091af3a7538838e126d7cd85a5e655d100749a7e4967df3f64a45bda66b3347b19277686a48eb195a876bf2993ba8b6443a752034011b2ff1b23d30986e72d5a12653d02fed46c41a6984f80cb779f2b93044debdebf376f6a32445ac52a0d4f159dbed67fb544c5590043e7371702eb1f488eda5cddf7302d91252c2ef0852a73a79a91edbae934b264f6f54cab54454a6822058587ace91b27fd770a9d4acd54a642175fb0f50af7c60e0b2404b82282d8da29985c15c42f1887082e9187f4273d0634561653cd139bf9cb5433ddee519fa65c5942ecad355854419e537378c8ab64738a6c98e0fa17325d9469b3681a7510b3c600000028931742fc0c578131ed6c9183a6c4b013077a5aad5602a29d5ac66ff51ffabba0934dd0a92ec08285326c33ffafc8c55095ccac81c03b56bea5bc2632a3e1f555e59d918a44f393b4f5fcc469f543a7b9525f7b3fd663a563320033b76db28a798926142ff17095bc756afa620cd7f4c7344e840938b166ea8360b0a32fa1ae0f39fd1a9c9408f73779f11bc4d8a4e55c8490cc6c0140511999fbec9ebcf3520be431e669016021af79045bd5b0c5d709ffbd8a56a7825519a7964abd2829641d83596c3cb7cb27c75b5e99d3fe1683b86d74e0004136f61e246162efb61aaa25b5eed376b2ecd47854bcc4ee5a3c141aa752f968593b9a318e46c80b02365ef5740ce28f7cf045c16a3072f7d5b66a5ce994bca506b016ab97af9bd9d670e7a5ac5c6b91d1e753418b6818ff53ee4db9fe098368fab412b13d43ab782c926571212eee0405096d9d68d63f0fabb2526b8f1adfc6eccb7b1c8be170109af421c7db2a6d95ae0fc9f7640c3c6db80886bdb75751bed5dc3d29a37f370fe0b53d1db5db76b4bfbd0863df111228403065b6cb6ee00ec35ff43ffbb30afd96593e07dbc2dfee47c504cfff7b641e40583ef1b572c93d6d7c05e7328d1d8daee18a416464ac935c7070d049172ed46aa9289118a4348cb5f65439e31cb300528b9d35b2c1673a737dd68527e4ad50c45a92b99f7af099cd5f34208909c372729c0a084c2aaf9db82c2ad1d7bc4997e4c5d274879b252c32bd567f9ab1ea6fbd83909909ff4e727b9cc5d65b17bbfea40203f21512fe5d4178683c3241058f8425f792ac457f37dc8d4c597d1fac96dc84408808dbcb056e6afb01d971b87601772de5310d913e6319adeae710a636e80bfda9b513cc8efcd768128bae757baba7389ee6127e8fcdea86d6b2125fa0c504c139d1df2f0475288d8daab330e46169f2bfb32c93e0b8642043bb34f5a0da60e88735be59abbf95ec6ebb1707f12fe8d146a83fcd50a4f737b9d94becd63661d9a5ac5d3444dcf76e28579cecada1fcd0ef9be8a2d433f32cc5912d1600cdc0fe56a39164280590b4ccf4263345fa0c3b3799a9bfb8352ebe97ba8676d2385913ec9d1c4990a479aa81a4ba84953462276ec9d0ccf17564d6ecdd0fd3beae451779aa4d71d09ff9e33a9bc850979cc7b48e6a10dcd40522ac09f2d0da5fb8d21ff653ddd2d2aa5f7c46f4a24b30cb7536739460f54562fe7e3a451d715909164aadbe74e050463579517500c21e606784af56327b7e823d64e60d79560e8061fe1a8376a23b34665829b072be22365704551f31888ee56b39551261ae92426ac18d547cc176cf87b434e9e4b265294a2da5a8303f84ffaa07b65f7ce963d4dc383c100d1b4dc172b04b7b89bc0ad2164d905d26c994114c091d903add2d35b6dd0494ccad144530d7e6212cab4cdd942c9d9f049f94a409383ce0c5391b07e593116490e4bda1d63b995c31fd2c5a4ddc318fd8ebd1cbf8f5cf5b788aaeaa2c8b4a107126d731ea9c66a117379e4d621547f56ff5867cd65c947bb6ab2bc3b2fdf4af34e897c4e9427df4c482e2448e3954c8e5d80c31542c8f998994ad45f75552561903a5fe24d8d7627d150f4c1acd3a8e6ed7da32138647c42561282216687fcc98eeb60112c51e3fbe8f65e5968ca8d923b4a3971567257283af7af9b6b38d88a4d6f236c51e9ddb5b5a9ffc046bca146efa39d8b4a8dd069e6b76643a2ccc7c74d5cdd6e740fe34dd340381ce553894dd361bb535459b5ab06be62318b84ce210e66115461014174a89efb3e893c6d7d69a7f04cd5fb739b5dce5c8b48a5e952b0b403d0e06a71da9e4690e9cb9c67afc6f3047fc3a8e4a42b7a71407b05f5d0e738f51ef3e9e2a4464fde5aa2e6c80b38ae336b09d25597e33b36ebfc1a8f6d307d61ec130a43ae363541be43b28dd599af039b31b865b71f1176775821884688bc2929aea04141fe6f7e2d1776a9977fda5692cf22290ab0e19ce7a536cab1eb34d76735aa72d4849d1d17227e0b16ff3c66c576959d0086b513397e46212390ad6b4052e7f1df2484926a806d1075f7a397e1245312595af3d536211eeb83edf4151162b53704b1f5997784179d43b33f1da439ab6a218266cfd05bd5ee4b3e36d7428343f5164d9659d1790fe85fc767fe5657c06ee80c682d6e7dc8963e4dc576ddf33e0bf2e6b3010bfc9ab839a559683717756de1ce0fb08f0dbbe934c6c7e300d34ef7cbdedce5ed4acee8a8b8e6df69aa574650ed081a07fca0f0c1d402de4ae2ec065a4b8962fdb5677ad5a6842a5378d75e76720031427cbbb5f6c842f5319d6013ab4f1b8f4de8aa3ba0c1af8a561e9a82dc7e639b10bf7b3a069660058b903f6a8ed11331d480e23a8c9591b2718ddb69fbf7659bbc3f6e9b0588bfce8e1933153f7182c38a060aca60fcfe0d94bb02d61f2d67d99eb9c654f4a5286b635e359b675d32d44e1ba5572c5ed30196f01b1c156e26ebf4d3796b6ea706d9bc8fc5f45dcbffcadedcce94e13b3498fff4a07f80d89c479f1e0fd12092b529e78d25da11f245f577ccbbeaf8c9671baad815c38426f993d1c64cc694ee60de13c2e4aa6f39b2b7a07b57e5a99102a18e2e99e17ee27271f642b6f94a643bd29317d148dca2e93cf88e38c92ce6d1e7d680581216f02e089e614dd60bfc72c025b0ecc78bab0e998af8fa1";
    Vec::from(hex::decode(hex_str).unwrap())
}

// `default_comm_r`, `default_comm_d` and `default_proof` come from running `polka-storage-provider utils po-rep` command
// for a random file with `.params` matching `default_verifying key`.
// They have been hardcoded here, because proof generation takes a long time.
// It is possible to generate proof and replica in the test.
fn default_comm_r() -> RawCommitment {
    let hex_str = "70e82695a03f9dbd96e568fef583283266a2048766e2c7abe78dcd9896996d68";
    <[u8; 32]>::from_hex(hex_str).unwrap()
}

fn default_comm_d() -> RawCommitment {
    let hex_str = "129c7562bb0c189544f5dccd365feaec2141eab458097a5ca8429c109d154421";
    <[u8; 32]>::from_hex(hex_str).unwrap()
}

fn default_proof() -> Vec<u8> {
    let hex_str = "ab83b2fbb64493d194deb78c3175b1d01a0dcfe35c47a57c7743a7387888b536b6b79cbb7842359f77961b141b322107806fb50db39374d0692e40004e00e0a038ce329ab401f9b090c66e70c5eaaf74fcaa03f883fd369bceee0e9e351fafa303c4dc8bd174f40b3bd515595cf57c62946acdc0eee797d31a80caf47fc93b6d067dc0093e17c2c9464e678257d14633a7cd8543bc122d9363e41e8eb905689c60d053be9a74bce51d4d000aae3975dc980ae7b43c5d3494e2bfa1dc92734b4e";
    Vec::from(hex::decode(hex_str).unwrap())
}
