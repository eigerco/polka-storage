use cid::Cid;
use clap::Parser;
use codec::Encode;
use multihash_codetable::{Code, MultihashDigest};
use pallet_market::{ClientDealProposal, DealProposal, DealState, CID_CODEC};
use sp_core::Pair;
use sp_runtime::{
    bounded_vec,
    traits::{IdentifyAccount, Verify},
    AccountId32, MultiSignature, MultiSigner,
};

use crate::cli::CliError;

#[derive(Debug, Clone, Parser)]
pub(crate) struct DealProposalCommand;

/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
type Signature = MultiSignature;

// probably make those types in the runtime public, and reuse them
// that's the way!

type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;
type Balance = u128;
// type Address = MultiAddress<AccountId, ()>;
type BlockNumber = u32;

pub fn key_pair(name: &str) -> sp_core::sr25519::Pair {
    sp_core::sr25519::Pair::from_string(name, None).expect("pls work")
}

pub fn account(name: &str) -> AccountId32 {
    let user_pair = key_pair(name);
    let signer = MultiSigner::Sr25519(user_pair.public());
    signer.into_account()
}

pub fn sign(pair: &sp_core::sr25519::Pair, bytes: &[u8]) -> MultiSignature {
    MultiSignature::Sr25519(pair.sign(bytes))
}

pub fn sign_proposal(
    client: &str,
    proposal: DealProposal<AccountId, Balance, BlockNumber>,
) -> ClientDealProposal<AccountId, Balance, BlockNumber, MultiSignature> {
    let alice_pair = key_pair(client);
    let client_signature = sign(&alice_pair, &Encode::encode(&proposal));
    ClientDealProposal {
        proposal,
        client_signature,
    }
}

pub fn cid_of(data: &str) -> cid::Cid {
    Cid::new_v1(CID_CODEC, Code::Blake2b256.digest(data.as_bytes()))
}

impl DealProposalCommand {
    pub async fn run(&self) -> Result<(), CliError> {
        let client: AccountId32 = account("//Alice");
        let provider: AccountId32 = account("//Charlie");

        println!("client: {}", client);
        println!("provider: {}", provider);
        let c = cid_of("marrocc");
        let deal_proposal = DealProposal::<AccountId, Balance, BlockNumber> {
            piece_cid: c.to_bytes().try_into().expect("work eh"),
            piece_size: 1,
            client,
            provider,
            label: bounded_vec![0xd, 0xe, 0xa, 0xd],
            start_block: 100,
            end_block: 1300000,
            storage_price_per_block: 1,
            provider_collateral: 1,
            state: DealState::<BlockNumber>::Published,
        };
        let client_proposal = sign_proposal("//Alice", deal_proposal);

        println!("c'est la vi, {:?}", client_proposal);
        println!("numbli je: {:?}", hex::encode(c.to_bytes()));

        Ok(())
    }
}
