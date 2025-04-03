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
            // DO NOT MODIFY
            // This code has been generated by `target/release/polka-storage-provider-client proofs benchmark-data --sector-size 2KiB examples/test-data-big.car`
            SectorSize::_2KiB => BenchmarkData {
                storage_provider_name: "//StorageProvider",
                verifying_key: include_bytes!("../../target/bench/params/2KiB.porep.vk.scale"),
                seal_proof: RegisteredSealProof::StackedDRG2KiBV1P1,
                post_type: RegisteredPoStProof::StackedDRGWindow2KiBV1P1,
                seal_randomness_height: 1u64,
                pre_commit_block_number: 5u64,
                comm_p: Commitment::<CommP>::from_cid(
                    &Cid::from_str(
                        "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                    )
                    .expect("valid cid"),
                )
                .expect("valid commitment"),
                sectors: vec![
                    SectorData {
                        sector_number: SectorNumber::new(0u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbv4g64ferqc4gjk6npyojx4t27j74ma55g2zdbysnjg5rtahuesl",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/0.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(1u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcb7eocpghnr76agceurtjagst5akkajwv3e4xy4gdzmcfnbfn4fy2",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/1.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(2u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcagit3atimdk7h4tkfe7tguscztc4t2wsgqww6cb25e2kntds6vsw",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/2.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(3u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbue5o7j45mhajaw24z5uilionz2rtpriu53pyqsp6vn4o523blrn",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/3.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(4u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcaeanji6dst35frbfkiyf44bjioh5t66keybitl24nkry5b746da6",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/4.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(5u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcavt7uca7cjtjckf26emqlo554bvdqnbjkjggbooszrq4m35emvdj",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/5.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(6u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcayb46rxqjwqrnwrvkkvoflkj4e353bzdnjvzwkhv7ca7kjyv7tiv",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/6.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(7u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcb75ae5s4ibyjqnq6qfcrccuv43rjmlicmvqk7lpaitl7zhjukk2q",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/7.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(8u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbbnclze3khcjc6ht3uffkxydr3vpym7gapukwppbarkqhivgdmby",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/8.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(9u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcagrf7e4vnadv7joos44pj7k5ildeeuxrgsn3itjkvpgb4yzu25bv",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/9.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(10u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbrgrbtm7p566pqosmkh2dagfckgm2mx2u3go3q4mzwjyv6kjzcss",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/10.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(11u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbi3dk45lfojo5v5tifnhd6vf4fglgvotdnjqjt7q6bccpalh4rzk",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/11.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(12u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcasod2b6civvvucmw2qtnxpcdnuldffwsqgio4nyqxijtisg2jmla",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/12.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(13u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbdw436pvdhjeem43fbdeccqlukpqtvt33cuopq5b5ifapdwhkta2",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/13.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(14u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcauxe4rhcusqmjgor4g3gzmbyf7g7wkkd6fj6fomwt4t6uasoioio",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/14.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(15u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbqn7jtrlifo5tcuxdlbchqh44bhmejkfxk3ouwpk3ovkw4gj7vqq",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/15.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(16u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcb52undra3nthpc3qq777uinabeomeax5iladuymv7e4weqvb6nig",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/16.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(17u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbpay4yzwkx3dyysev7h5vjqn23xzas7gyajr5be733v4b7wxy7tm",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/17.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(18u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbn4lyxhbaoq5fyuodyzxaypyaimpf7iezff34msd3ybja4u26aae",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/18.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(19u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcaj3c5bgk2ssykzpxp7fgqkviwqwrbmsbt2vnzhgsz7qw677ar6jv",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/19.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(20u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcairacttgbuprkkqvlhtfvznavr2ymsrfyhev6lmwff7u6xyw2maq",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/20.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(21u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbvwm2m4kpu7ft2rrawlrizr4fnt63zfcsiccc534ive5hbcwjpcs",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/21.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(22u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcadcppiabt3vezgqqbqnwpzzkbn67dpl25iyvwcxmd4pe6kt46vyt",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/22.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(23u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcajczlkywnyb3xpyx3ot4tvgr6tnanlkv5kolt45friigewzajcje",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/23.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(24u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbgewy3anrkfpwxxg4ikgfc6jcisraaqhgsxlgvdpfdepzsucupcm",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/24.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(25u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abca6xxi3knb6isvl7jmlvshdgpma7ew5mvctpp6olpwtqdwstpqnlp",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/25.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(26u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcb47buyvo3jrtqmrvm67qvy5bvcuxrk2glmaad6ytyaa3ils42dzm",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/26.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(27u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcafj4ianlfgyazzqiktijkdb4jiy3ztdvwka3ybzrypxdzuwzt6ti",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/27.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(28u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcb54nnhzt2inbesa5ve67xy22piiinlgwirgns4bwvxgs7u5n6xi5",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/28.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(29u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcaonfuqqqn5emger2iq24wbtdwhuy4ovk3pnq2ijfw4wmr3h2g3iz",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/29.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(30u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcahfw3hjrlqq4up6dp2cfhtmj7mpa6gz4spnniq7jq6prydwk5ise",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/30.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(31u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcag4u5fuhwtkwjamn5vscwsv36potdjb3rtrz2dr2a2j2upwr343d",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/2KiB/31.sector.proof.porep.scale"
                        ),
                    },
                ],
                _phantom: PhantomData,
            },
            // DO NOT MODIFY
            // This code has been generated by `target/release/polka-storage-provider-client proofs benchmark-data --sector-size 8MiB examples/test-data-big.car`
            SectorSize::_8MiB => BenchmarkData {
                storage_provider_name: "//StorageProvider",
                verifying_key: include_bytes!("../../target/bench/params/8MiB.porep.vk.scale"),
                seal_proof: RegisteredSealProof::StackedDRG8MiBV1,
                post_type: RegisteredPoStProof::StackedDRGWindow8MiBV1,
                seal_randomness_height: 1u64,
                pre_commit_block_number: 5u64,
                comm_p: Commitment::<CommP>::from_cid(
                    &Cid::from_str(
                        "baga6ea4seaqbfhdvmk5qygevit25ztjwl7voyikb5k2fqcl2lsuefhaqtukuiii",
                    )
                    .expect("valid cid"),
                )
                .expect("valid commitment"),
                sectors: vec![
                    SectorData {
                        sector_number: SectorNumber::new(0u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcb7czmo3lnn5rrxou2bkx3keynqkdjzdbynnu5z2eeuuv6be26tko",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/0.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(1u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcb5f5lb4i2hsliovixs65myk3fme2qvi4dkgjlllhn4vvxi2tctqh",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/1.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(2u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcam7xblgp6u25gbhhd57eaii4lml5mvxiqub4wdjtaxeqaxcmk6tc",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/2.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(3u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcakfyhcqjfegakjms3jxrdrbzuiykjuer3lucz5ki3zpxplye2ocx",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/3.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(4u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abca6iykd5l5ses5mlkjpgxscaaxmint76pk6ldyk7fupuzdv676yts",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/4.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(5u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcayzdgaojs37vdm6i6xhb2lb2oi6cunfgfkepyefiwrl7cqmntwb6",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/5.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(6u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcb3l4pcgwugoaho4lsewryvomqlzswvjijaxnoaair2syuroonijb",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/6.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(7u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbn6rr4svphc7wikram2ik2om77xiy7ljdm4vj522h2ty62w73os7",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/7.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(8u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbyq7ollks32uwg2kgmeps3upqdqnxsw7odsq273tcvwwu5ycobya",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/8.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(9u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbwoj7jxvjsgjgllh7bsx6o2yobnboks7decm4iebd323vgj44o2u",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/9.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(10u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbwhtkogxf3e7z5bu46kl6zczzsk7q7ofo3anyytiqceoguncc4cz",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/10.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(11u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abca6nl737gj5cdozbvdtxnjcju7iik5ljwj6rrm4xrasyqxsynyzzb",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/11.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(12u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abca3y7gxe7metpovzap2asurqstlcp7ccibqtehe7nrfcrowd2bfdo",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/12.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(13u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcag5vmquskji67zy3boqb5tqkqvnoko4mvyexkt3sjzo7binvhkct",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/13.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(14u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbjbprsno2y3giyl3juyw254apfypyoe7zo35r5ryunqgy4ajyzzb",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/14.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(15u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbbsooa3oyexzk6iu72lwamlnskigel45drw5q4fyyfd6ooyahm3n",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/15.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(16u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcai6ueewlmuuvqtxz2elmdlwmajtcjhrkoekrgy6iem72g2a7pj2o",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/16.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(17u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbeutrxdnghhpbzxkecitkcdxwetdfmqsi73xkmpttelbzsj6m2bo",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/17.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(18u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcahz526sfzxjaxxczv7ucuqqvq3qkaki6xiqhlfx6y7j62mpvacau",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/18.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(19u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abca6zk5paehqib5m3tz42pqkurbc5juwplaguautekmng46a5o62iu",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/19.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(20u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcb5fu2qintnjv4t32ukrcofygnyqt624gt5kfvtmgojwzbwcfmdc6",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/20.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(21u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbr3lyx523gzydctzvq45yk37a7ammtbjjxh6fy57fc2p5wjmtls7",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/21.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(22u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcadjlnlrdxwkmoaf6n4kh62ipo73ujhq3qmouecq64h27yz3k5llf",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/22.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(23u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbt3wvcftjpnf3gh4w22w34jhrckk5fwevkrm5jrhmsn7dhw45hy2",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/23.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(24u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbwc42vc7c2cmpwpdibqvnvzv3u3qo56hjpdfsxud2jw25acelaqh",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/24.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(25u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcak4kkq3ss5npadmveryzkvjldfjkwdlabwod6wlvsbipxmks7vjt",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/25.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(26u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcaqfblnqsys3we7d6frc2u7y4bcyf74qqsviksbnev5isq7mumwrx",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/26.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(27u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbip7hb2eqa7lirxpmoej33kcmyefu2hkc3s264frbkfswfd35usk",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/27.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(28u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcaihcetyqhdv7txz5oqcpz27tgdnxm6l2wmzkzpb7ooxchd77uckt",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/28.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(29u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbvwv55mysee5mlwclf2cmc5sml7g5sc6zfeewqua2cpz24ugy4k2",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/29.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(30u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abca57vugq4zxee2nvero2xofyy2ykjjdpcknc6ptkpu2464rl5dgkb",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/30.sector.proof.porep.scale"
                        ),
                    },
                    SectorData {
                        sector_number: SectorNumber::new(31u32).expect("valid sector ID"),
                        padded_piece_size: PaddedPieceSize::new(2048u64)
                            .expect("valid padded piece size"),
                        comm_r: Commitment::<CommR>::from_cid(
                            &Cid::from_str(
                                "bagboea4b5abcbn64jw3l5kxijjrmqovf2laoco57w7ddi3fi2ql45je5xqqvvp2t",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        comm_d: Commitment::<CommD>::from_cid(
                            &Cid::from_str(
                                "baga6ea4seaqp7gdmc6b7ruugxxee762dptklzxtyd3lzls26bcwhxdyrcnjkcoi",
                            )
                            .expect("valid cid"),
                        )
                        .expect("valid commitment"),
                        proof: include_bytes!(
                            "../../target/bench/proofs/8MiB/31.sector.proof.porep.scale"
                        ),
                    },
                ],
                _phantom: PhantomData,
            },
            s => panic!("unsupported sector size {s}"),
        }
    }
}
