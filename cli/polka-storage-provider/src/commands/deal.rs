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
    let encoded = Encode::encode(&proposal);
    println!("encoded proposal: {}", hex::encode(&encoded));
    let client_signature = sign(&alice_pair, &encoded);
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
            label: bounded_vec![0xde, 0xad],
            start_block: 100,
            end_block: 100 + 741265408 + 200,
            storage_price_per_block: 1,
            provider_collateral: 1,
            state: DealState::<BlockNumber>::Published,
        };
        let client_proposal = sign_proposal("//Alice", deal_proposal);
        // println!("client proposal {:?}", client_proposal);
        println!("encoded cid  {:?}", hex::encode(c.to_bytes()));
        let MultiSignature::Sr25519(crypto_bytes) = &client_proposal.client_signature else {
            panic!("no no no");
        };
        println!("signature hex: {:?}", hex::encode(&crypto_bytes[..]));
        Ok(())
    }
}

// 980155a0e4022022e14069bfa61a3c7440209dbc8922d591719a1b5cf96cf1262b0fc6aec0b60f0100000000000000d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d90b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe2208dead6400000020d61300010000000000000000000000000000000100000000000000000000000000000000
// 980155a0e4022022e14069bfa61a3c7440209dbc8922d591719a1b5cf96cf1262b0fc6aec0b60f0100000000000000d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d90b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe2208dead6400000020d61300010000000000000000000000000000000100000000000000000000000000000000 