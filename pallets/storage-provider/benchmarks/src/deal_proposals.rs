use codec::Encode;
use frame_system::pallet_prelude::BlockNumberFor;
use pallet_market::{BalanceOf, ClientDealProposal, DealProposal};
use sp_core::sr25519;
use sp_io::crypto::sr25519_sign;
use sp_runtime::{MultiSignature, MultiSigner};

pub type ClientDealProposalOf<T> = ClientDealProposal<
    <T as frame_system::Config>::AccountId,
    BalanceOf<T>,
    BlockNumberFor<T>,
    MultiSignature,
>;
pub type DealProposalOf<Test> =
    DealProposal<<Test as frame_system::Config>::AccountId, BalanceOf<Test>, BlockNumberFor<Test>>;

pub fn sign_proposal<T: pallet_market::Config>(
    pubkey: MultiSigner,
    proposal: DealProposalOf<T>,
) -> ClientDealProposalOf<T> {
    let client_signature = create_sr25519_signature(&Encode::encode(&proposal), pubkey);
    ClientDealProposal {
        proposal,
        client_signature,
    }
}

pub fn create_sr25519_signature(payload: &[u8], pubkey: MultiSigner) -> MultiSignature {
    let srpubkey = sr25519::Public::try_from(pubkey).unwrap();
    let srsig = sr25519_sign(0.into(), &srpubkey, payload).unwrap();
    srsig.into()
}
