use codec::Encode;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_market::{BalanceOf, ClientDealProposal, DealProposal};
use sp_core::ed25519;
use sp_io::crypto::{ed25519_generate, ed25519_sign};
use sp_runtime::{traits::IdentifyAccount, AccountId32, MultiSignature, MultiSigner};

pub const ALICE: &'static str = "//Alice";
#[allow(dead_code)]
pub const BOB: &'static str = "//Bob";
#[allow(dead_code)]
pub const CHARLIE: &'static str = "//Charlie";

pub type ClientDealProposalOf<T> = ClientDealProposal<
    <T as frame_system::Config>::AccountId,
    BalanceOf<T>,
    BlockNumberFor<T>,
    MultiSignature,
>;
pub type DealProposalOf<Test> =
    DealProposal<<Test as frame_system::Config>::AccountId, BalanceOf<Test>, BlockNumberFor<Test>>;

pub fn generate_benchmark_account(name: &'static str) -> (AccountId32, MultiSigner) {
    let signer: sp_core::ed25519::Public =
        ed25519_generate(0.into(), Some(name.as_bytes().to_vec())).into();
    let signer: MultiSigner = signer.into();
    let account_id = signer.clone().into_account();

    (account_id, signer)
}

pub fn create_ed25519_signature(payload: &[u8], pubkey: MultiSigner) -> MultiSignature {
    let edpubkey = ed25519::Public::try_from(pubkey).unwrap();
    let edsig = ed25519_sign(0.into(), &edpubkey, payload).unwrap();
    edsig.into()
}

pub fn sign_proposal<T: pallet_market::Config>(
    pubkey: MultiSigner,
    proposal: DealProposalOf<T>,
) -> ClientDealProposalOf<T> {
    let client_signature = create_ed25519_signature(&Encode::encode(&proposal), pubkey);
    ClientDealProposal {
        proposal,
        client_signature,
    }
}
