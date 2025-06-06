extern crate alloc;

use codec::Encode;
use primitives::deals::ClientDealProposal;
use sp_core::sr25519;
use sp_io::crypto::{sr25519_generate, sr25519_sign};
use sp_runtime::{traits::IdentifyAccount, AccountId32, MultiSignature, MultiSigner};

use crate::benchmarking::{ClientDealProposalOf, DealProposalOf};

pub mod absolute_block_number;
pub mod benchmark_data;
pub mod deal_timeline;
pub mod relative_block_number;
mod sector_data;
pub mod sector_timeline;
mod storage_provider_data;

pub fn generate_benchmark_account<T>(name: &'static str) -> (AccountId32, MultiSigner)
where
    T: frame_system::Config<AccountId = AccountId32>,
{
    let signer: sp_core::sr25519::Public =
        sr25519_generate(0.into(), Some(name.as_bytes().to_vec())).into();
    let signer: MultiSigner = signer.into();
    let account_id = signer.clone().into_account();

    // NOTE(@Jinxit,#739,05/03/2025): Inlined from `frame_benchmarking::whitelist_account`
    // to make the T  explicit.
    frame_benchmarking::benchmarking::add_to_whitelist(
        frame_system::Account::<T>::hashed_key_for(&account_id).into(),
    );

    (account_id, signer)
}

pub fn sign_proposal<T>(pubkey: MultiSigner, proposal: DealProposalOf<T>) -> ClientDealProposalOf<T>
where
    T: pallet_storage_provider::Config,
{
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
