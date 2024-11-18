use codec::{Decode, Encode};
use frame_support::{assert_noop, assert_ok};
use hex::FromHex;
use polka_storage_proofs::{Bls12, VerifyingKey};
use primitives_proofs::{
    ProofVerification, PublicReplicaInfo, RawCommitment, RegisteredPoStProof, SectorNumber, Ticket,
};
use rand::SeedableRng;
use rand_xorshift::XorShiftRng;
use sp_runtime::{BoundedBTreeMap, BoundedVec};
use sp_std::collections::btree_map::BTreeMap;

use crate::{mock::*, tests::TEST_SEED, Error, PoRepVerifyingKey, PoStVerifyingKey};

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
fn post_verification_succeeds() {
    new_test_ext().execute_with(|| {
        let (post_type, proof_bytes, vkey_bytes, randomness, replicas) = test_setup();

        assert_ok!(ProofsModule::set_post_verifying_key(
            RuntimeOrigin::signed(1),
            vkey_bytes
        ));

        assert_ok!(<ProofsModule as ProofVerification>::verify_post(
            post_type,
            randomness,
            BoundedBTreeMap::try_from(replicas).expect("replicas should be valid"),
            BoundedVec::try_from(proof_bytes).expect("proof_bytes should be valid"),
        ));
    });
}

#[test]
fn post_verification_fails() {
    new_test_ext().execute_with(|| {
        let (post_type, proof_bytes, _, randomness, replicas) = test_setup();
        let mut rng = XorShiftRng::from_seed(TEST_SEED);
        let vkey = Encode::encode(&VerifyingKey::<Bls12>::random(&mut rng));

        assert_ok!(ProofsModule::set_post_verifying_key(
            RuntimeOrigin::signed(1),
            vkey
        ));

        assert_noop!(
            <ProofsModule as ProofVerification>::verify_post(
                post_type,
                randomness,
                BoundedBTreeMap::try_from(replicas).expect("replicas should be valid"),
                BoundedVec::try_from(proof_bytes).expect("proof_bytes should be valid"),
            ),
            Error::<Test>::InvalidPoStProof
        );
    });
}

fn test_setup() -> (
    RegisteredPoStProof,
    Vec<u8>,
    Vec<u8>,
    Ticket,
    BTreeMap<SectorNumber, PublicReplicaInfo>,
) {
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

    (post_type, proof_bytes, vkey_bytes, randomness, replicas)
}

// `default_comm_r`, `default_comm_d` and `default_proof` come from running `polka-storage-provider utils po-rep` command
// for a random file with `.params` matching `default_verifying key`.
// They have been hardcoded here, because proof generation takes a long time.
// It is possible to generate proof and replica in the test.
fn default_porep_comm_r() -> RawCommitment {
    let hex_str = "4afb35f82a95a10187a913bc14520d9a1d173328265b301b5dcf440ef2583950";
    <[u8; 32]>::from_hex(hex_str).unwrap()
}

// `polka-storage-provider utils post-params` - cached here, because it takes a long time.
fn default_post_verifyingkey() -> Vec<u8> {
    let hex_str = "8b1be5427dd16c968793f62f58f33a8ae382e15b183fda0cea1a4f42781d1d7e3a7bb27d51f4dbe88f95435d8b07a54f8be505265d1cce97d82c5c9d8a4720fabec0d277749c77d456b1872d38bbdd9929f588147ef7c87acaacb26c081fafb88cac3b1d81d5b3c303e41a1986e4662911a3815a4bfdf1df74c09a4f36da064aa0d3c718f44201c86ab92b3936c717fc04710c3800d48bee12eba80b305ced25f0733ed8ee0962be67a0acff5632da4d1164be58f27efa08445e38c02008b87e95c204814cd7f3ee755ca466efb0e5096c3dac0ca518d20a3e7f9f5967ffc600440b4abc82552e6b1909fb4e4cfe59f914e523dc176699fd884fa8d5e42495e4708bfc8313935c27c28633e01d023d66fc919c6ddcf53fba46364348ed8d7112870a96af7c08796df6f6992bf09c98d5b53e7b85bc1c159c004866ac20bc07d8921fe4877d5c0140e234b922105e84b08318b66c0a58d52c17031beeb29de0ae62d9f722cfafa6bcca99a5d9dc249238ea7b1bb479430881e35f885b3c3c7ded0f32bed601ae0adb9465264696fb3741adf02ba0168383ebf325ad7284903de9bd9ed4c8d560349ece3b18cbd2c7c5680000001789ff37f3a354b6ef2b72b942081a3d5e1db890ae9751b1e223fd6b14641f262f50c1d27bbc325e21ee628a8bdf5a1445b99439e3e35781a962ba807b8bdcc8becb47106f669c3f2820b082c379082defcc06fa9eb92a9ec5551c9d345b5510acb3a8b5fd9dd4dd5317815e01baf5ba099710935e3618848133e4eb3a2231a9540902bc6059cce3fed6792bef484f3d6d94afa87bcc7c240d66e7d3995439fa97fe785754a3242ec17ec4ef16fe0f9d9c6b2fe03bf85f9f4e64a3bd0a8f6fa2c2b57bfe2bf1cfe6457782f1a62df4e8d4d9946b0fb7c9c73d4f1ee127a222afe5dd4da6f6cfb759a98826ac3d26f4f209b2cc365ccc9b00ab3ec83e1d4a5760986f1b7a528544e7e58d046b7d94a854d5519a3d522734794d1382c68c0c89ea9a8a64eb971b07744a4328d82ce6e52dcb4926852e02867d853bf29d1a1e7af9d31d0dad89eca94d0c5092cfb0a8c1b8d88869dcf27221d29402a1d2a7666c2950db8c4d9dd3a8f67d1e3cff2fc28cab0f6b87dfc6ab0728db8c8519503acf11a384eea19e5796cff505629a5a7f8ee05a60814d4f8f975ebcf107526e8f358b005a4a17c20283d590095ca2d0785d9e55b9ac21e3d65b6112a6be03e00c91ad390f45dc985438c36de765c849dd41defa5dba3252638577396c2bfcf43aa41c45b32618d3a571681b111d53aff797cb82e72df71fdde2cee041748df545022ca0c728c93495ce9d082434f2bff3a46bf3b00a7fb3fe7b09cc6f2df28cbdbbce21016d4b9228683ee267137e1435c5ae5de6bc8d9c255c361b0fe37c6bd3b824cca3634900473c90b8f4605b736ed6b1b35efc60e52d7f78d33dedd0da090967b549bffa63f4f6d51d6078794996778aa78db077e900c3cb4acac702d141e1b4674f192d5657c65a7de6582b723c53fae4a2aeb48becb38f8d646d78a6d2b87e97b93b631e6450955e37febe18fb9f00d0dcd5540e5d115f369f2303aa144da71a0ae1dfd7b5e7321e26a0ce2e25c25e57b5e891934447d95542e9f2523110b4f0e1fcc1b300dd7a8b59cb428e76bdd3c8c99e7e42ef1904a1c04c0ddff8fd1554999d4817fb276f2679ac6d9f8bc1e504b137efca6b820636f8a20c79a3081cf63d363815586292424886e9a4fb7f924cb4776c0004f92e29b4c49205d5cac8b38981db405522bccc3fef9ef1eff06855d8d5ff9174e38e2bc5d3b876caf0fb20a21e85e9a065f163390092caf8a86ffdf41e2e841313fbfc7697e25c2144d32d0aeb6916d3e899c70fadf42fb0e99544874ce4e14e87174f18a93fbfef6f0d214e0659b6a11c531c465fa122c70eef88a516ddac7bfe4ddc7ef9ed52a94d38b3aecc37a3e16889bc94238b42e919f793c77f40ef4ab645452e5268b36f8ee66680985f58a59558328a9d1210041bf317ada0a7c2ffc0698cf84a4cf7dc9784409bab5c0faeac9b98bfb09a41db89ccd8e4d24c6cf5001806debc1102491028f4a1d08bde958859a4e7e62937a3bbd85d7e7efb00422504eba84e246a6175d321a24cb44422c7c4fd49ac378ec44809bf";
    Vec::from(hex::decode(hex_str).unwrap())
}

fn default_post_proof() -> Vec<u8> {
    let hex_str = "b9a2946cc51995918456f26f9018da4cf30453457b7f638096f5dc44bdea917948e849e3460f610e4a0ff380fd27748796bd14b4d78c9416baa4f2180ed54604d48dfb100360fa7cc36149a0779254934906423e61d472792d52132a6f3873440ed81c5f606e290da2d4a88ea170ac843c9d0f0ce90e1f9e628e1fc060e3e215d04ad71ce58b0283a017dc35a551751380f5eb33492f16bdb2385cf445a31609c01714c47698d6b5119c5c4d90983226c39ab9a1a0ff51d5adda64c96f871e70";
    Vec::from(hex::decode(hex_str).unwrap())
}
