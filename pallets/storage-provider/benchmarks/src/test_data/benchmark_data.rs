extern crate alloc;

use alloc::{str::FromStr, vec, vec::Vec};
use core::marker::PhantomData;

use cid::Cid;
use frame_system::pallet_prelude::BlockNumberFor;
use primitives::{
    commitment::{piece::PaddedPieceSize, CommD, CommP, CommR, Commitment},
    deals::DealState,
    proofs::{RegisteredPoStProof, RegisteredSealProof},
    sector::{ProveCommitSector, SectorNumber, SectorPreCommitInfo},
    MAX_LABEL_SIZE, MAX_POREP_PROOFS_PER_BLOCK, MAX_SEAL_PROOF_BYTES, MAX_SECTORS_PER_CALL,
};
use sp_runtime::{traits::ConstU32, AccountId32, BoundedVec, MultiSigner};

use crate::test_data::{
    absolute_block_number::Absolute, deal_timeline::DealTimeline, generate_benchmark_account,
    relative_block_number::Relative, sector_data::SectorData, sector_timeline::SectorTimeline,
    sign_proposal, storage_provider_data::StorageProviderData, ClientDealProposalOf,
    DealProposalOf,
};

// If this is changed, also update BenchmarkData in `storage-provider/client/src/commands/proofs.rs`.
#[derive(Debug)]
pub struct BenchmarkData<T>
where
    T: frame_system::Config,
{
    pub storage_provider_name: &'static str,
    pub porep_verifying_key: &'static [u8],
    pub post_verifying_key: &'static [u8],
    pub seal_proof: RegisteredSealProof,
    pub post_type: RegisteredPoStProof,
    pub comm_p: Commitment<CommP>,
    pub sectors: Vec<SectorData>,
    pub timeline: SectorTimeline<BlockNumberFor<T>, T::AccountId>,
    _phantom: PhantomData<T>,
}

impl<T> BenchmarkData<T>
where
    T: frame_system::Config<AccountId = AccountId32>,
{
    pub fn storage_provider(&self) -> StorageProviderData {
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
        T: pallet_storage_provider::Config,
    {
        self.sectors
            .iter()
            .take(limit as usize)
            .map(|sector| {
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
                    // TODO(@Jinxit,29/04/2025): Make use of multiple deals instead of hardcoding for a single one.
                    start_block: self.timeline.deals()[0].start().0,
                    end_block: self.timeline.deals()[0].end().0,
                    storage_price_per_block: 5u32.into(),
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
    ) -> BoundedVec<SectorPreCommitInfo<BlockNumberFor<T>>, ConstU32<MAX_SECTORS_PER_CALL>> {
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
                expiration: self.timeline.sector_expiration().0,
                unsealed_cid: sector.comm_d.cid().to_bytes().try_into().unwrap(),
                seal_randomness_height: self.timeline.seal_randomness_height().0,
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
                    .porep_proof
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

    pub fn load() -> BenchmarkData<T> {
        // DO NOT MODIFY
        // This code has been generated by `target/release/polka-storage-provider-client proofs benchmark-data --sector-size 8MiB examples/big_file_184k.car`
        BenchmarkData {
            storage_provider_name: "//StorageProvider",
            porep_verifying_key: include_bytes!(
                "../../../../../test-fixtures/keys/1GiB.porep.vk.scale"
            ),
            post_verifying_key: include_bytes!(
                "../../../../../test-fixtures/keys/1GiB.post.vk.scale"
            ),
            seal_proof: RegisteredSealProof::StackedDRG1GiBV1,
            post_type: RegisteredPoStProof::StackedDRGWindow1GiBV1,
            comm_p: Commitment::<CommP>::from_cid(
                &Cid::from_str("baga6ea4seaqhx2sxpfc2f3k2o75m3acskihug7me3g4coyw6adjqnd6ioszfqay")
                    .expect("valid cid"),
            )
            .expect("valid commitment"),
            sectors: vec![SectorData {
                sector_number: SectorNumber::new(0u32).expect("valid sector ID"),
                padded_piece_size: PaddedPieceSize::new(262144u64)
                    .expect("valid padded piece size"),
                comm_r: Commitment::<CommR>::from_cid(
                    &Cid::from_str(
                        "bagboea4b5abcarklyf5uz74bkznzt74mkl7gcxym63ap44ep53u5msiygilq43jm",
                    )
                    .expect("valid cid"),
                )
                .expect("valid commitment"),
                comm_d: Commitment::<CommD>::from_cid(
                    &Cid::from_str(
                        "baga6ea4seaqcjdzgezdmdynwaoursai6zwafbxjmz7k4r3fnwwioizcwbq3zwki",
                    )
                    .expect("valid cid"),
                )
                .expect("valid commitment"),
                porep_proof: include_bytes!(
                    "../../../../../test-fixtures/proofs/1GiB/0.sector.proof.porep.scale"
                ),
                post_proof: include_bytes!(
                    "../../../../../test-fixtures/proofs/1GiB/0.sector.proof.post.scale"
                ),
            }],
            timeline: {
                let _proving_period_offset = 15u32;
                let _proving_period_start_initial = 75u32;
                let _seal_randomness_height = 93u32;
                let _sector_expiration = 230u32;
                let _interactive_block_number = 110u32;
                let _prove_commit_sectors = 111u32;
                let _deadline_index = 0u32;
                let _deadline_challenge_block = 245u32;
                let _deadline_start = 255u32;
                let _submit_windowed_post = 256u32;
                let _deadline_close = 275u32;
                SectorTimeline::new(
                    AccountId32::new([
                        182u8, 193u8, 149u8, 53u8, 114u8, 191u8, 35u8, 38u8, 36u8, 236u8, 20u8,
                        137u8, 254u8, 55u8, 102u8, 15u8, 54u8, 170u8, 187u8, 9u8, 38u8, 195u8,
                        121u8, 127u8, 99u8, 252u8, 61u8, 105u8, 112u8, 109u8, 83u8, 119u8,
                    ]),
                    Absolute::from(BlockNumberFor::<T>::from(1u32)),
                    Absolute::from(BlockNumberFor::<T>::from(5u32)),
                    Absolute::from(BlockNumberFor::<T>::from(100u32)),
                    vec![DealTimeline::new(
                        Absolute::from(BlockNumberFor::<T>::from(180u32)),
                        Relative::from(BlockNumberFor::<T>::from(50u32)),
                    )],
                    3u32,
                    Relative::from(BlockNumberFor::<T>::from(60u32)),
                    Relative::from(BlockNumberFor::<T>::from(20u32)),
                    Relative::from(BlockNumberFor::<T>::from(10u32)),
                    Relative::from(BlockNumberFor::<T>::from(10u32)),
                )
                .expect("valid timeline")
            },
            _phantom: PhantomData,
        }
    }
}
