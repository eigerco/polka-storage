use codec::{Decode, Encode};
use frame_support::{assert_noop, assert_ok};
use hex::FromHex;
use polka_storage_proofs::{Bls12, VerifyingKey};
use primitives_proofs::{
    ProofVerification, ProverId, PublicReplicaInfo, RawCommitment, RegisteredPoStProof,
    RegisteredSealProof, SectorNumber, Ticket,
};
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;
use sp_std::collections::btree_map::BTreeMap;

use crate::{mock::*, Error, PoRepVerifyingKey, PoStVerifyingKey};

const TEST_SEED: [u8; 16] = [
    0x59, 0x62, 0xbe, 0x5d, 0x76, 0x3d, 0x31, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc, 0xe5,
];

#[test]
fn sets_porep_verifying_key() {
    new_test_ext().execute_with(|| {
        assert_eq!(None, PoRepVerifyingKey::<Test>::get());
        let vk = default_porep_verifyingkey();

        assert_ok!(ProofsModule::set_porep_verifying_key(
            RuntimeOrigin::signed(1),
            vk.clone()
        ));
        let scale_vk: VerifyingKey<Bls12> = Decode::decode(&mut vk.as_slice()).unwrap();
        assert_eq!(Some(scale_vk), PoRepVerifyingKey::<Test>::get());
    });
}

#[test]
fn sets_post_verifying_key() {
    new_test_ext().execute_with(|| {
        assert_eq!(None, PoRepVerifyingKey::<Test>::get());
        let vk = default_post_verifyingkey();

        assert_ok!(ProofsModule::set_post_verifying_key(
            RuntimeOrigin::signed(1),
            vk.clone()
        ));
        let scale_vk: VerifyingKey<Bls12> = Decode::decode(&mut vk.as_slice()).unwrap();
        assert_eq!(Some(scale_vk), PoStVerifyingKey::<Test>::get());
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

        let (seal_proof, sector, prover_id, ticket, seed) = default_porep_test_setup();
        let comm_r = default_porep_comm_r();
        let comm_d = default_porep_comm_d();
        let proof_bytes = default_porep_proof();

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
fn porep_verification_succeeds() {
    new_test_ext().execute_with(|| {
        let (seal_proof, sector, prover_id, ticket, seed) = default_porep_test_setup();
        let comm_r = default_porep_comm_r();
        let comm_d = default_porep_comm_d();
        let proof_bytes = default_porep_proof();
        let vkey_bytes = default_porep_verifyingkey();

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

#[test]
fn post_verification_succeeds() {
    new_test_ext().execute_with(|| {
        let post_type = RegisteredPoStProof::StackedDRGWindow2KiBV1P1;
        let proof_bytes = default_post_proof();
        let vkey_bytes = default_post_verifyingkey();
        let sector_id = 77;
        let randomness = [1u8; 32];
        let mut replicas = BTreeMap::new();
        replicas.insert(
            sector_id,
            PublicReplicaInfo {
                comm_r: default_porep_comm_r(),
            },
        );

        assert_ok!(ProofsModule::set_post_verifying_key(
            RuntimeOrigin::signed(1),
            vkey_bytes
        ));

        assert_ok!(<ProofsModule as ProofVerification>::verify_post(
            post_type,
            randomness,
            replicas,
            proof_bytes,
        ));
    });
}

// Values hardcoded in this function are matching the ones in https://github.com/eigerco/polka-storage/blob/9433eb81bfa76a30fbac1f8f79101ab6359f4f3e/cli/polka-storage-provider/src/commands/utils.rs#L188.
// This is because those values are coming from the clients of this pallet and are related to the proof system.
// Prover and verifier must match those values when verifiying and proving the data.
fn default_porep_test_setup() -> (
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
fn default_porep_verifyingkey() -> Vec<u8> {
    let hex_str = "9236d7bb99a700bbd20e8aee74c8533fcefdd8a2f8c3ec5f9cf35d6b5ff588dad3e8f75c8f1c212a4947fdb41a9a6ed7b9fc09eb20807ec190ca16e997af1c9caf65d4663c5920d6551aaad767559696c469b5ab495e84123fa7b6a1175e1f49ae6de8481137a36c7f47ac50b21580e6f18cb7966a670f44594e78079547e5008fe4471fe886d49b3d5fb43cdd41879819ea19f9d0845dccdc225f90952e8ecdc0483b834f63a98cbf97adcf95be127afd2fa6022f4108def4b042ae6ddb11ff96de424de262695c22ebbb497648e69381a5d39d8189aac667d94a9c544abe6c8a72d209a898c7395df238e45cd98d1e0337ce093062db2b47e14ee03a19be62845e9618b4f90df73c2fd21aad43d05404b3b29f0dccd20d1fdcf2bf6fff5394b6f0cfce26bbb086c9bc0c51280fe1a422553b65a9c28bcbab29b70b1e9f045e29470cf6aca81bba4bc69f285153942aa630362628154b06208c70fa4a27185a5c75636b401acb5d82c044f52767143b03f4595769c1888b96aa105471577ca60daa34b40dde2e345a1edc810567538157220b09e2ae8256378aa2d1f4960ce5abd2f02c5ebca6ed095f2991274b7f460000002885871a5d3ded0869cd25cfa1a18c7df601154de9e4cf3f5ff8031e69621f053e983b504652382d40a8ba3a0138a997be844e90eff46da4380a548962a796e91e99cd979dcc92e4d5de9b812fb75f75b46ba3aa129facfc8130b34ecbce6ec63ab7a4f9f61db724d829ec6da0a70c9b06892e3ee64b88c8299c53e08a016ed6e427e3c7b400c9d474fb9a6739dfea6df4a145c13418648e093964f01767155410db3a4cfce4d77f62d1c4cea996797daf97e44bc3b382da85714614d6529ca7fcb237b6cf81a4c0af4bf45436be49c1ff184ddb42b4976396e1b5998c69fc6c7147f676aeea719bb42ccbe3bd9d4567a989fbbb80820981c286e79c9e562abca71efc66b0624f9def12e73ba27ca115e3a575d9bd8d480de977dc516de3d26e708075fba08aba941499273084b2ca923f0f284de2ecade47375e3370dbe120028a1a5d21194a5fbacbbeb256b9a062fd78aa3138ada4fa5cf5a903b304271d5f8112b620ab8962f46af5397b3fbe4e6c9386762a173644bff6a345176866d16859613cdc23fd183814ace85a70c6c2db1ad77734153303cc2f448afccde75b62a574acf27a6c13d431d2d23006c9468b8a0edb597756e915e3f3554e5bd4f2d864693f7edd96786a642afa5181e4f4e467d092a56506030ef4dee2bfff945b3ca8ccdd1c74a44a9735b573f47f812ceb80ea17b7454b4072bb1f57986bb676ef15d8fe9b4d0eef1b0add9f1b1dfb09b4dad84dec2fc1610a31f194c0b1f7dd580da81039ff34096132d72f940e9e06f2e1c067d251abccb0065de85e19af86e0391d0680e2ccce4f5117baf7a39d8a7a37003f6fa2eed8521580d9bc4e0dbbd5a01d9b6db16fe14c93e35a7be33bcc32a98dcd4ce1596d1b0ee4a079426769bfac31dc9f80e898a268c1b122804890215a6d455ba9546ed41e4522b4663f28e498e775e550fdba74702dd90ce42c1ee1ac84d05d5c6c539e03716be4b6249993edd61d772850b3ef640c81cf0b25509a0a119120d6a3421613ec6feb1c743527ff8d4565a1a125d681e1aec80351c04aeece182670fe8c257c5d73630b8cc74dab22ee0bc87140568ade580f46a32d67438ceb4adf816ff0886c61008d61e3c96423584b77e5ce373883698e7b2963bf686efa98f3d07258e1632f6a03ee276b194dd0497607b3a98317429f2dd1d6365750b075d4451e05764fc1b543902e0d3abdce545a99bac1f4c70b18106d0623c22c603dd4139f197eb91b469dd176b446007e58ae8e361bacb570d387d079703889493b9cebb05e1d9cf272503fcbb7d3832fff3b9c0cfffc91626a39f4f09f20ab8ea18d6a928bc67c09a0af7cc1dc1b414fdb5a61d17fb59a6ee2efaa6d36525993630967eb797cfafad9d8bdae5d6252fd1bddde5655f534182fb6f79a73fb0a86864833fef6e3b738e45721a8b9b0229e57c6b72d8b0130b9571e76967b18f360f688a584c9418e1e6723ff1d5b9b6ded7d00d272b2eb64110ad03d0304268041e435be5041105ba7fe0949ba7c834a0b83172a8b3f1a54e8b2af1c752798fe38d28d1ddd951aac9b49fbef818abc46fe8e409e18349c842f303fd766435bfd52dd5d9f7e542b9c6243ca6b041c8a74b40d44805a81c9f5ed4ef7bc83ca52f517d4d525c6089ca35a6ad99b4d204212b224b50939426004fb4f87d8dcf58ae058685a4b614660ea426210ca94ce39c3e31673449551cd624a16c45378f89302e87b10bbf8c4202c4d7adef0a382391cd2832076715e9a0ae4de80b694a3b7ae764ba37f834d646609399a9c4915f9a3c4a19d3c37e774328d8b1aec9a38ab6fa6ba7c9130768dc208f7842118088dd5c15660c914ae33bc4eeae0dcc2c64e186a131c95cd4f5866be6dc97f440278f7bda146bb3c620b07b0c7bef6cebf8843543c869711bcd29ce7524696fc8a5b8810956f123233a067763c75d36f08c8aa082309be8c708d00b88b1b2c69aba470d16d97620b7e66186e7367c505502f432379d7e15fd33a5279370d1786f218bbbe5fa807782f2aac45b4c3965494f57bb3d8cdba839a688e1a3649f437ed64f98971bfd9cb0867167ccd195d6d12592aa53a249b2b5848865c1b53bff79b2fee04fc1d3f14045e3d016968b13cf2accbc3e3cd238d61ae47309d428cdf96e889c93fd43c00f0c35d5e20e421a192a65c253b5336f1533956771ad4a465e964d042fd5a455351e6d9968795cde0f7d8e7572220591267ec5c691b7caff37872cc986243e45cf0928991dad972d33cdd77b57ebd14c3c8de0f80a5e3cc8d100adf9e9a26e0c670b305633a85f1763882391573f84773ce7dd467cb7fe9677bb5eee5065c41e2fed4a63e18e08303e728eb3033d95cde897cb601d005071397701e7996bbb019cc8bcc1adf712b76e356fb2eaaa808a47a2e8e943c3dfa345ff97a3482e4ecb249c944cb2f0468ae3a266be67da2c477a9d125fa87603a0fc0835bc04a3cdab45064f21cf118df4a520894d57105868884985398b1d71f19d54919c7568f82da5e59d1d91f0942a66bb6fc2bb2b298d94a868f0c7d51efc5f8ab466fc625607f2622a6e39f672e6c8355c6e9edb0fea2e26226a5a8395fad182bf4ce40c3eac40d2d658ca9883240733b75d76c77a683b83ead95498d96c1ea8d88031e94d3a21ca8ca3c5edf16763812b1c50f6beb4a85b883de879c8cef3d2";
    Vec::from(hex::decode(hex_str).unwrap())
}

// `polka-storage-provider utils post-params` - cached here, because it takes a long time.
fn default_post_verifyingkey() -> Vec<u8> {
    let hex_str = "8b1be5427dd16c968793f62f58f33a8ae382e15b183fda0cea1a4f42781d1d7e3a7bb27d51f4dbe88f95435d8b07a54f8be505265d1cce97d82c5c9d8a4720fabec0d277749c77d456b1872d38bbdd9929f588147ef7c87acaacb26c081fafb88cac3b1d81d5b3c303e41a1986e4662911a3815a4bfdf1df74c09a4f36da064aa0d3c718f44201c86ab92b3936c717fc04710c3800d48bee12eba80b305ced25f0733ed8ee0962be67a0acff5632da4d1164be58f27efa08445e38c02008b87e95c204814cd7f3ee755ca466efb0e5096c3dac0ca518d20a3e7f9f5967ffc600440b4abc82552e6b1909fb4e4cfe59f914e523dc176699fd884fa8d5e42495e4708bfc8313935c27c28633e01d023d66fc919c6ddcf53fba46364348ed8d7112870a96af7c08796df6f6992bf09c98d5b53e7b85bc1c159c004866ac20bc07d8921fe4877d5c0140e234b922105e84b08318b66c0a58d52c17031beeb29de0ae62d9f722cfafa6bcca99a5d9dc249238ea7b1bb479430881e35f885b3c3c7ded0f32bed601ae0adb9465264696fb3741adf02ba0168383ebf325ad7284903de9bd9ed4c8d560349ece3b18cbd2c7c5680000001789ff37f3a354b6ef2b72b942081a3d5e1db890ae9751b1e223fd6b14641f262f50c1d27bbc325e21ee628a8bdf5a1445b99439e3e35781a962ba807b8bdcc8becb47106f669c3f2820b082c379082defcc06fa9eb92a9ec5551c9d345b5510acb3a8b5fd9dd4dd5317815e01baf5ba099710935e3618848133e4eb3a2231a9540902bc6059cce3fed6792bef484f3d6d94afa87bcc7c240d66e7d3995439fa97fe785754a3242ec17ec4ef16fe0f9d9c6b2fe03bf85f9f4e64a3bd0a8f6fa2c2b57bfe2bf1cfe6457782f1a62df4e8d4d9946b0fb7c9c73d4f1ee127a222afe5dd4da6f6cfb759a98826ac3d26f4f209b2cc365ccc9b00ab3ec83e1d4a5760986f1b7a528544e7e58d046b7d94a854d5519a3d522734794d1382c68c0c89ea9a8a64eb971b07744a4328d82ce6e52dcb4926852e02867d853bf29d1a1e7af9d31d0dad89eca94d0c5092cfb0a8c1b8d88869dcf27221d29402a1d2a7666c2950db8c4d9dd3a8f67d1e3cff2fc28cab0f6b87dfc6ab0728db8c8519503acf11a384eea19e5796cff505629a5a7f8ee05a60814d4f8f975ebcf107526e8f358b005a4a17c20283d590095ca2d0785d9e55b9ac21e3d65b6112a6be03e00c91ad390f45dc985438c36de765c849dd41defa5dba3252638577396c2bfcf43aa41c45b32618d3a571681b111d53aff797cb82e72df71fdde2cee041748df545022ca0c728c93495ce9d082434f2bff3a46bf3b00a7fb3fe7b09cc6f2df28cbdbbce21016d4b9228683ee267137e1435c5ae5de6bc8d9c255c361b0fe37c6bd3b824cca3634900473c90b8f4605b736ed6b1b35efc60e52d7f78d33dedd0da090967b549bffa63f4f6d51d6078794996778aa78db077e900c3cb4acac702d141e1b4674f192d5657c65a7de6582b723c53fae4a2aeb48becb38f8d646d78a6d2b87e97b93b631e6450955e37febe18fb9f00d0dcd5540e5d115f369f2303aa144da71a0ae1dfd7b5e7321e26a0ce2e25c25e57b5e891934447d95542e9f2523110b4f0e1fcc1b300dd7a8b59cb428e76bdd3c8c99e7e42ef1904a1c04c0ddff8fd1554999d4817fb276f2679ac6d9f8bc1e504b137efca6b820636f8a20c79a3081cf63d363815586292424886e9a4fb7f924cb4776c0004f92e29b4c49205d5cac8b38981db405522bccc3fef9ef1eff06855d8d5ff9174e38e2bc5d3b876caf0fb20a21e85e9a065f163390092caf8a86ffdf41e2e841313fbfc7697e25c2144d32d0aeb6916d3e899c70fadf42fb0e99544874ce4e14e87174f18a93fbfef6f0d214e0659b6a11c531c465fa122c70eef88a516ddac7bfe4ddc7ef9ed52a94d38b3aecc37a3e16889bc94238b42e919f793c77f40ef4ab645452e5268b36f8ee66680985f58a59558328a9d1210041bf317ada0a7c2ffc0698cf84a4cf7dc9784409bab5c0faeac9b98bfb09a41db89ccd8e4d24c6cf5001806debc1102491028f4a1d08bde958859a4e7e62937a3bbd85d7e7efb00422504eba84e246a6175d321a24cb44422c7c4fd49ac378ec44809bf";
    Vec::from(hex::decode(hex_str).unwrap())
}

// `default_comm_r`, `default_comm_d` and `default_proof` come from running `polka-storage-provider utils po-rep` command
// for a random file with `.params` matching `default_verifying key`.
// They have been hardcoded here, because proof generation takes a long time.
// It is possible to generate proof and replica in the test.
fn default_porep_comm_r() -> RawCommitment {
    let hex_str = "4afb35f82a95a10187a913bc14520d9a1d173328265b301b5dcf440ef2583950";
    <[u8; 32]>::from_hex(hex_str).unwrap()
}

fn default_porep_comm_d() -> RawCommitment {
    let hex_str = "129c7562bb0c189544f5dccd365feaec2141eab458097a5ca8429c109d154421";
    <[u8; 32]>::from_hex(hex_str).unwrap()
}

fn default_porep_proof() -> Vec<u8> {
    let hex_str = "b16ded2118ec297cb64bd5af327b9a70e732669530ba4ab34fc44e1510d9ecbc01d71c16133a2dc938b6dcfddb2a54e3a492f6e306f6bdc28e2de306ff1bf5addc7cf910f6d5602d554743981998d650241a29b2288a5ca42d5b62ebe648410e19e9cd1992294d2fd558904b7687d3c246beb03f330db5c456b8675b6bcf6fd3d08049699015e570fd915d271dbd40519560888dd56ba2093a855cae488d4870b480440dac22150c01f2a7dfc11fa638a5cf4cc9c7d95d16b0458b658c220803";
    Vec::from(hex::decode(hex_str).unwrap())
}

fn default_post_proof() -> Vec<u8> {
    let hex_str = "b9a2946cc51995918456f26f9018da4cf30453457b7f638096f5dc44bdea917948e849e3460f610e4a0ff380fd27748796bd14b4d78c9416baa4f2180ed54604d48dfb100360fa7cc36149a0779254934906423e61d472792d52132a6f3873440ed81c5f606e290da2d4a88ea170ac843c9d0f0ce90e1f9e628e1fc060e3e215d04ad71ce58b0283a017dc35a551751380f5eb33492f16bdb2385cf445a31609c01714c47698d6b5119c5c4d90983226c39ab9a1a0ff51d5adda64c96f871e70";
    Vec::from(hex::decode(hex_str).unwrap())
}
