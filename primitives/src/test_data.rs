extern crate alloc;

use alloc::{str::FromStr, vec, vec::Vec};
use core::{cmp::min, marker::PhantomData};

use cid::Cid;
use codec::Encode;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::sr25519;
use sp_io::crypto::{sr25519_generate, sr25519_sign};
use sp_runtime::{
    traits::{ConstU32, Get, IdentifyAccount},
    AccountId32, BoundedVec, MultiSignature, MultiSigner,
};

use crate::{
    commitment::{piece::PaddedPieceSize, CommD, CommP, CommR, Commitment},
    configs::{BalanceOf, CurrencyProvider, MarketProvider},
    deals::{ClientDealProposal, ClientDealProposalOf, DealProposalOf, DealState},
    proofs::{RegisteredPoStProof, RegisteredSealProof},
    sector::{ProveCommitSector, SectorNumber, SectorPreCommitInfo, SectorSize},
    MAX_LABEL_SIZE, MAX_POREP_PROOFS_PER_BLOCK, MAX_SEAL_PROOF_BYTES, MAX_SECTORS_PER_CALL,
    PEER_ID_MAX_BYTES,
};

/// The sector size used in benchmarks.
///
/// TODO(@Jinxit,#739,28/02/2025): Change to 1GiB when we have the data for it.
pub const BENCH_SECTOR_SIZE: SectorSize = SectorSize::_8MiB;

#[derive(Debug)]
pub struct BenchmarkData<T> {
    pub storage_provider_name: &'static str,
    pub verifying_key: &'static [u8],
    pub seal_proof: RegisteredSealProof,
    pub post_type: RegisteredPoStProof,
    pub seal_randomness_height: u64,
    pub pre_commit_block_number: u64,
    pub comm_p: Commitment<CommP>,
    pub sectors: Vec<SectorData>,
    _phantom: PhantomData<T>,
}

#[derive(Debug)]
pub struct SectorData {
    pub sector_number: SectorNumber,
    pub padded_piece_size: PaddedPieceSize,
    pub comm_r: Commitment<CommR>,
    pub comm_d: Commitment<CommD>,
    pub proof: &'static [u8],
}

#[derive(Debug)]
pub struct StorageProviderData {
    pub account_id: AccountId32,
    pub sign: MultiSigner,
    pub peer_id: BoundedVec<u8, ConstU32<PEER_ID_MAX_BYTES>>,
}

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
    T: frame_system::Config + CurrencyProvider + MarketProvider<OffchainSignature = MultiSignature>,
    BalanceOf<T>: Encode,
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

impl<T> BenchmarkData<T> {
    pub fn storage_provider(&self) -> StorageProviderData
    where
        T: frame_system::Config<AccountId = AccountId32>,
    {
        let (account_id, sign) = generate_benchmark_account::<T>(&self.storage_provider_name);

        let peer_id: &[u8; 32] = &account_id.as_ref();
        let peer_id = peer_id.to_vec().try_into().unwrap();
        StorageProviderData {
            account_id,
            sign,
            peer_id,
        }
    }

    pub fn deal_proposals(
        &self,
        client: &(AccountId32, MultiSigner),
        limit: u32,
    ) -> BoundedVec<ClientDealProposalOf<T>, T::MaxDeals>
    where
        T: frame_system::Config<AccountId = AccountId32>
            + CurrencyProvider
            + MarketProvider<OffchainSignature = MultiSignature>,
        BlockNumberFor<T>: From<u64>,
        BalanceOf<T>: From<u32> + Encode,
    {
        self.sectors
            .iter()
            .take(limit as usize)
            .map(|sector| {
                let min_dur = T::MinDealDuration::get();
                let label = vec![u32::from(sector.sector_number) as u8; MAX_LABEL_SIZE as usize];
                let proposal = DealProposalOf::<T> {
                    piece_cid: self
                        .comm_p
                        .cid()
                        .to_bytes()
                        .try_into()
                        .expect("hash is always 32 bytes"),
                    piece_size: *sector.padded_piece_size,
                    client: client.0.clone(),
                    provider: self.storage_provider().account_id,
                    label: BoundedVec::try_from(label).unwrap(),
                    start_block: 100.into(),
                    end_block: min_dur + 100.into(),
                    storage_price_per_block: 5u32.into(),
                    provider_collateral: 25u32.into(),
                    state: DealState::Published,
                };
                sign_proposal::<T>(client.1.clone(), proposal)
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or_else(|_| panic!("deals did not fit into BoundedVec"))
    }

    pub fn pre_commit_sectors(
        &self,
        limit: u32,
    ) -> BoundedVec<SectorPreCommitInfo<BlockNumberFor<T>>, ConstU32<MAX_SECTORS_PER_CALL>>
    where
        T: crate::configs::StorageProviderProvider,
        BlockNumberFor<T>: From<u64>,
    {
        self.sectors
            .iter()
            .take(limit as usize)
            .map(|sector| SectorPreCommitInfo {
                seal_proof: self.seal_proof,
                sector_number: sector.sector_number,
                sealed_cid: sector.comm_r.cid().to_bytes().try_into().unwrap(),
                deal_ids: vec![u64::from(sector.sector_number) as u64]
                    .try_into()
                    .unwrap(),
                expiration: min(
                    T::SectorMaximumLifetime::get(),
                    T::MaxSectorExpiration::get(),
                ),
                unsealed_cid: sector.comm_d.cid().to_bytes().try_into().unwrap(),
                seal_randomness_height: self.seal_randomness_height.into(),
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    pub fn prove_commit_sectors(
        &self,
        limit: u32,
    ) -> BoundedVec<ProveCommitSector, ConstU32<MAX_SECTORS_PER_CALL>> {
        self.sectors
            .iter()
            .take(limit as usize)
            .map(|sector| {
                let proofs: BoundedVec<_, ConstU32<MAX_POREP_PROOFS_PER_BLOCK>> = sector
                    .proof
                    .chunks_exact(MAX_SEAL_PROOF_BYTES as usize)
                    .map(|chunk| chunk.to_vec().try_into().unwrap())
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap();
                ProveCommitSector {
                    sector_number: sector.sector_number,
                    proofs,
                }
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap()
    }

    pub fn load(sector_size: SectorSize) -> BenchmarkData<T> {
        match sector_size {
            s => panic!("unsupported sector size {s}"),
        }
    }
}
