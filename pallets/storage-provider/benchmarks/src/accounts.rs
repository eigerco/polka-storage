use sp_io::crypto::sr25519_generate;
use sp_runtime::{traits::IdentifyAccount, AccountId32, MultiSigner};

pub const ALICE: &'static str = "//Alice";

pub fn generate_benchmark_account(name: &'static str) -> (AccountId32, MultiSigner) {
    let signer: sp_core::sr25519::Public =
        sr25519_generate(0.into(), Some(name.as_bytes().to_vec())).into();
    let signer: MultiSigner = signer.into();
    let account_id = signer.clone().into_account();

    (account_id, signer)
}
