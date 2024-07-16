use pallet_market::{DealProposal, DealState};
use clap::Parser;
use sp_runtime::{AccountId32, MultiSigner, MultiSignature, bounded_vec, traits::{Verify, IdentifyAccount}};
use sp_core::Pair;

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
    sp_core::sr25519::Pair::from_string(name, None).unwrap()
}

pub fn account(name: &str) -> AccountId32 {
    let user_pair = key_pair(name);
    let signer = MultiSigner::Sr25519(user_pair.public());
    signer.into_account()
}

impl DealProposalCommand {
    pub async fn run(&self) -> Result<(), CliError> {
        let client: AccountId32 = account("//Alice");
        let provider: AccountId32 = account("//Charlie");

        let deal_proposal = DealProposal::<AccountId, Balance, BlockNumber> {
            piece_cid: bounded_vec![],
            piece_size: 1,
            client,
            provider,
            label: bounded_vec![0xd, 0xe, 0xa, 0xd],
            start_block: 100,
            end_block: 120,
            storage_price_per_block: 10,
            provider_collateral: 100, 
            state: DealState::<BlockNumber>::Published,
        };

        println!("c'est la vi, {:?}", deal_proposal);
        Ok(())
    }
}