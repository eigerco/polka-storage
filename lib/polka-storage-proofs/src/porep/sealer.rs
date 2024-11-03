use std::{fs::File, path::Path};

use bellperson::groth16;
use blstrs::Bls12;
use filecoin_hashers::Domain;
use filecoin_proofs::{
    add_piece, as_safe_commitment, parameters::setup_params, DefaultPieceDomain,
    DefaultPieceHasher, PaddedBytesAmount, PoRepConfig, SealCommitPhase1Output,
    SealPreCommitOutput, SealPreCommitPhase1Output, SectorShapeBase, UnpaddedBytesAmount,
};
use primitives_commitment::{
    piece::{PaddedPieceSize, PieceInfo},
    Commitment,
};
use primitives_proofs::{RawCommitment, RegisteredSealProof, SectorNumber};
use storage_proofs_core::{compound_proof, compound_proof::CompoundProof};
use storage_proofs_porep::stacked::{self, StackedCompound, StackedDrg};

use super::{seal_to_config, PoRepError};
use crate::{
    types::{ProverId, Ticket},
    ZeroPaddingReader,
};

pub type Proof = groth16::Proof<Bls12>;
pub type SubstrateProof = crate::Proof<bls12_381::Bls12>;

/// Prepares an arbitrary piece to be used by [`Sealer::create_sector`].
///
/// It does so by calculating the proper size for the padded reader
/// (by means of converting the raw size into a padded size and then into an unpadded size),
/// and then by wrapping the respective file reader with a [`ZeroPaddingReader`].
pub fn prepare_piece<P>(
    piece_path: P,
    piece_comm_p: Commitment,
) -> Result<(ZeroPaddingReader<File>, PieceInfo), std::io::Error>
where
    P: AsRef<Path>,
{
    let piece_file = File::open(piece_path)?;
    let piece_raw_size = piece_file.metadata()?.len();

    // If a file is unpadded, we can calculate its final size with Fr32 Padding and next power of two padding via
    // `PaddedPieceSize::from_arbitrary_size`. E.g. 900 bytes -> 1024 bytes. However, Filecoin's `add_piece` methods
    // requires size, to be before `Fr32` padding, so we call `.unpadded()` to get the `Fr32 unpadded`.
    // Required because of Filecoin magic, we'll probably need to change our Unpadded/Padded
    // into Filecoin implementations and instead write extensions for them to make them ergonomic
    let padded_piece_size = PaddedPieceSize::from_arbitrary_size(piece_raw_size);
    let piece_padded_unpadded_length = padded_piece_size.unpadded();
    let piece_padded_file = ZeroPaddingReader::new(piece_file, *piece_padded_unpadded_length);

    let piece_info = PieceInfo {
        commitment: piece_comm_p,
        size: padded_piece_size,
    };

    Ok((piece_padded_file, piece_info))
}
pub struct Sealer {
    porep_config: PoRepConfig,
}

impl Sealer {
    pub fn new(seal_proof: RegisteredSealProof) -> Self {
        Self {
            porep_config: seal_to_config(seal_proof),
        }
    }

    /// Adds a Piece and padding to already existing sector file and returns how many bytes were written.
    /// It can return more bytes than the piece size, as it adds padding so a proper Merkle Tree can be created out of the sector.
    /// You need to supply current pieces which are already in the sector, otherwise they'll be overwritten.
    pub fn add_piece<R: std::io::Read, W: std::io::Write>(
        &self,
        piece_data: R,
        piece: PieceInfo,
        current_pieces: &Vec<PieceInfo>,
        mut unsealed_sector: W,
    ) -> Result<u64, PoRepError> {
        let current_pieces_lengths: Vec<UnpaddedBytesAmount> = current_pieces
            .into_iter()
            .map(|p| p.size.unpadded().into())
            .collect();

        let (calculated_piece_info, written_bytes) = add_piece(
            piece_data,
            &mut unsealed_sector,
            piece.size.unpadded().into(),
            &current_pieces_lengths,
        )?;

        if piece.commitment.cid().hash().digest() != calculated_piece_info.commitment {
            return Err(PoRepError::InvalidPieceCid(
                0,
                piece.commitment.cid().hash().digest().try_into().unwrap(),
                calculated_piece_info.commitment,
            ));
        }

        Ok(written_bytes.into())
    }

    /// Adds zero-piece padding to the sector, to fill it out completely, so a proper CommD merkle tree can be calculated.
    /// Accepts current pieces in the sector and how much space they occupy. The space occupied can be calculated by storing results
    /// of [`Self::add_piece`] outputs.
    /// E.g. when sector of size 2048 has a pieces which are 1024 + 256, it'll add zero-commitment pieces so it sums up to 2048.
    pub fn pad_sector(
        &self,
        current_pieces: &Vec<PieceInfo>,
        sector_occupied_space: u64,
    ) -> Result<Vec<PieceInfo>, PoRepError> {
        let mut result_pieces = current_pieces.clone();
        let sector_size: UnpaddedBytesAmount = self.porep_config.sector_size.into();
        let padding_pieces =
            filler_pieces(sector_size - UnpaddedBytesAmount(sector_occupied_space));
        result_pieces.extend(padding_pieces.into_iter().map(|p| {
            PieceInfo::from_filecoin_piece_info(p, primitives_commitment::CommitmentKind::Piece)
        }));

        Ok(result_pieces)
    }

    /// Takes all of the pieces and puts them in a sector with padding.
    /// # Arguments
    ///
    /// * `pieces` - source of the data and its expected unpadded length and CommP.
    /// * `unsealed_sector` - where the sector data should be stored (all of it's pieces padded to the sector size).
    ///
    /// # References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/master/filecoin-proofs/src/api/mod.rs#L416>
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/tests/pieces.rs#L369>
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/master/fil-proofs-tooling/src/shared.rs#L103>
    pub fn create_sector<R: std::io::Read, W: std::io::Write>(
        &self,
        pieces: Vec<(R, PieceInfo)>,
        mut unsealed_sector: W,
    ) -> Result<Vec<PieceInfo>, PoRepError> {
        if pieces.is_empty() {
            return Err(PoRepError::EmptySector);
        }

        let mut result_pieces: Vec<PieceInfo> = Vec::with_capacity(pieces.len());
        let mut piece_lengths: Vec<UnpaddedBytesAmount> = Vec::with_capacity(pieces.len());
        let mut sector_occupied_space: UnpaddedBytesAmount = UnpaddedBytesAmount(0);
        for (idx, (reader, piece)) in pieces.into_iter().enumerate() {
            let fc_piece: filecoin_proofs::PieceInfo = piece.into();
            let (calculated_piece_info, written_bytes) =
                add_piece(reader, &mut unsealed_sector, fc_piece.size, &piece_lengths)?;

            piece_lengths.push(fc_piece.size);

            // We need to add `written_bytes` not `piece.size`, as `add_piece` adds padding.
            sector_occupied_space = sector_occupied_space + written_bytes;

            if fc_piece.commitment != calculated_piece_info.commitment {
                return Err(PoRepError::InvalidPieceCid(
                    idx,
                    fc_piece.commitment,
                    calculated_piece_info.commitment,
                ));
            }

            result_pieces.push(piece);
        }

        let result_pieces = self.pad_sector(&result_pieces, sector_occupied_space.into())?;

        Ok(result_pieces)
    }

    /// Takes the data contained in `unsealed_sector`, seals it and puts it into `sealed_sector`.
    /// Outputs CommR and CommD.
    ///
    /// # Arguments
    /// - `cache_directory` - cache where temporary data to speed up computation is stored.
    /// - `unsealed_sector` - sector's storage path, where all of the pieces are stored.
    /// - `sealed_sector` - a path where sealed data will be written.
    /// - `prover_id` - id of a proving entity, must match between Proving and Verification.
    /// - `sector_id` - id of a sector, must match between Proving and Verification.
    /// - `ticket` - randomness seed, must match between Proving and Verification.
    /// - `piece_infos` - list of pieces contained in the `unsealed_sector`.
    ///
    /// # References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/api/seal.rs#L58>
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/api/seal.rs#L210>
    pub fn precommit_sector<
        UnsealedSector: AsRef<Path>,
        SealedSector: AsRef<Path>,
        CacheDirectory: AsRef<Path>,
    >(
        &self,
        cache_directory: CacheDirectory,
        unsealed_sector: UnsealedSector,
        sealed_sector: SealedSector,
        prover_id: ProverId,
        sector_id: SectorNumber,
        ticket: Ticket,
        piece_infos: &[PieceInfo],
    ) -> Result<PreCommitOutput, PoRepError> {
        let cache_directory = cache_directory.as_ref();
        let sealed_sector = sealed_sector.as_ref();

        let piece_infos = piece_infos
            .into_iter()
            .map(|p| (*p).into())
            .collect::<Vec<filecoin_proofs::PieceInfo>>();

        let p1_output: SealPreCommitPhase1Output<SectorShapeBase> =
            filecoin_proofs::seal_pre_commit_phase1(
                &self.porep_config,
                cache_directory,
                unsealed_sector,
                sealed_sector,
                prover_id,
                sector_id.into(),
                ticket,
                &piece_infos,
            )?;

        let SealPreCommitOutput { comm_r, comm_d } = filecoin_proofs::seal_pre_commit_phase2(
            &self.porep_config,
            p1_output,
            cache_directory,
            sealed_sector,
        )?;

        Ok(PreCommitOutput { comm_r, comm_d })
    }

    /// Generates a zk-SNARK proof guaranteeing a sealed_sector at `replica_path` is being stored.
    ///
    /// # Arguments:
    /// - `proving_paramters` - Groth16 params generated by [`crate::porep::generate_random_parameters`] used to prove the sector.
    /// - `cache_path` - cache directory where temporary data to speed up computation is stored.
    /// - `replica_path` - a path where sealed data is stored
    /// - `prover_id` - id of a proving entity, must match between Proving and Verification.
    /// - `sector_id` - id of a sector, must match between Proving and Verification.
    /// - `ticket` - randomness seed, must match between Proving and Verification.
    /// - `seed` - randomness seed, must match between Proving and Verification.
    /// - `pre_commit` - CommR and CommD produced by `precommit_sector`.
    /// - `piece_infos` - list of pieces contained in the `replica_path`.
    ///
    /// # References:
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/api/seal.rs#L350>
    /// * <https://github.com/filecoin-project/rust-fil-proofs/blob/5a0523ae1ddb73b415ce2fa819367c7989aaf73f/filecoin-proofs/src/api/seal.rs#L507>
    pub fn prove_sector<CacheDirectory: AsRef<Path>, SealedSector: AsRef<Path>>(
        &self,
        proving_parameters: &groth16::MappedParameters<Bls12>,
        cache_path: CacheDirectory,
        replica_path: SealedSector,
        prover_id: ProverId,
        sector_id: u64,
        ticket: Ticket,
        seed: Option<Ticket>,
        pre_commit: PreCommitOutput,
        piece_infos: &[PieceInfo],
    ) -> Result<Vec<groth16::Proof<Bls12>>, PoRepError> {
        let cache_path = cache_path.as_ref();
        let replica_path = replica_path.as_ref();

        let piece_infos = piece_infos
            .into_iter()
            .map(|p| (*p).into())
            .collect::<Vec<filecoin_proofs::PieceInfo>>();

        let scp1: filecoin_proofs::SealCommitPhase1Output<SectorShapeBase> =
            filecoin_proofs::seal_commit_phase1_inner(
                &self.porep_config,
                cache_path,
                replica_path,
                prover_id,
                sector_id.into(),
                ticket,
                seed,
                SealPreCommitOutput {
                    comm_d: pre_commit.comm_d,
                    comm_r: pre_commit.comm_r,
                },
                &piece_infos,
                false,
            )?;

        let SealCommitPhase1Output {
            vanilla_proofs,
            comm_d,
            comm_r,
            replica_id,
            seed,
            ticket: _,
        } = scp1;

        let comm_r_safe = as_safe_commitment(&comm_r, "comm_r")?;
        let comm_d_safe = DefaultPieceDomain::try_from_bytes(&comm_d)?;

        let public_inputs = stacked::PublicInputs {
            replica_id,
            tau: Some(stacked::Tau {
                comm_d: comm_d_safe,
                comm_r: comm_r_safe,
            }),
            k: None,
            seed: Some(seed),
        };

        let compound_setup_params = compound_proof::SetupParams {
            vanilla_params: setup_params(&self.porep_config)?,
            partitions: Some(usize::from(self.porep_config.partitions)),
            priority: false,
        };

        let compound_public_params =
            <StackedCompound<SectorShapeBase, DefaultPieceHasher> as CompoundProof<
                StackedDrg<'_, SectorShapeBase, DefaultPieceHasher>,
                _,
            >>::setup(&compound_setup_params)?;

        let groth_proofs = StackedCompound::<SectorShapeBase, DefaultPieceHasher>::circuit_proofs(
            &public_inputs,
            vanilla_proofs,
            &compound_public_params.vanilla_params,
            proving_parameters,
            compound_public_params.priority,
        )?;

        Ok(groth_proofs)
    }
}

/// Takes remaining space to be filled with zero-byte pieces and generates filler pieces.
/// Sector's CommD is calculated in two ways: from pieces and from the Sector file.
/// During computation from the Sector file, when the sector is not full, zero-bytes are used
/// as padding to make the sector match the necessary node size for the Binary Merkle Tree calculation.
/// To match this logic when calculating CommD out of the Piece Infos we need to generate dummy pieces.
/// Returns dummy pieces with appropriate sizes and commitments.
///
/// Pre-condition:
/// * `remaining_space == (unpadded_sector_size - real_pieces.map(|p| p.unpadded_piece_length).sum())`
///
/// References:
/// * <https://github.com/filecoin-project/lotus/blob/471819bf1ef8a4d5c7c0476a38ce9f5e23c59bfc/lib/filler/filler.go#L9>
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/constants.rs#L164>
/// * <https://github.com/filecoin-project/go-commp-utils/blob/master/zerocomm/zerocomm.go>
pub fn filler_pieces(remaining_space: UnpaddedBytesAmount) -> Vec<filecoin_proofs::PieceInfo> {
    // We convert it to `PaddedBytesAmount` as it makes calculations (on the powers of 2) easier (see below).
    let mut remaining_space: PaddedBytesAmount = remaining_space.into();

    // All of the piece sizes need to be Padded (i.e. Fr32 padded, because we use BLS12-381 cryptography, where each field element is 254 bits)
    // All of the piece sizes need to be a power of 2 as well, because we use Binary Merkle Tree for CommD/CommP computation.
    // Considering the binary representation of remaining_space, each set bit represents a valid piece size.
    let pieces = remaining_space.0.count_ones() as usize;
    let mut piece_infos: Vec<filecoin_proofs::PieceInfo> = vec![];
    for _ in 0..pieces {
        // We create pieces from smaller to bigger, because Merkle Proof is computed from leaves to root.
        // e.g.
        // sector_size = 2048.to_unpadded() = 2032
        // piece_size = 256.to_unpadded() = 254 (Piece 0)
        // remaining = 2032 - 254 = 1778.to_padded() = 1792
        // 1792 == 0b11100000000
        // Piece 1 | trailing_zeros = 8 | piece_size = 1 << 8 = 256
        // remaining ^= (1 << 8) = 0b11000000000
        // Piece 2 | trailing_zeros = 9 | piece_size = 1 << 9 = 512
        // remaining ^= (1 << 9) = 0b10000000000
        // Piece 3 | trailing|zeros = 10 | piece_size = 1 << 10 = 1024
        // And then, out of the pieces, CommD:
        // hash(Piece 0[256], Piece 1 [256])
        // hash(Piece 0|1, Piece 2)
        // hash(Piece 0|1|2, Piece 3)
        let next = remaining_space.0.trailing_zeros();
        let psize = PaddedBytesAmount(1 << next);

        remaining_space.0 ^= psize.0;

        piece_infos.push(filecoin_proofs::pieces::zero_padding(psize.into()).unwrap());
    }

    // We return filler piece infos as this is what `filecoin_proofs::seal_pre_commit_phase1` accepts
    piece_infos
}

/// Public inputs for a PoRep.
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/storage-proofs-porep/src/stacked/vanilla/params.rs#L832>]
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/types/mod.rs#L53>
#[derive(Debug, Clone)]
pub struct PreCommitOutput {
    /// Sealed Sector (Replica) Commitment, after padding and processing it.
    pub comm_r: RawCommitment,
    /// Data commitment, after padding, before processing it into a replica.
    pub comm_d: RawCommitment,
}

#[cfg(test)]
mod test {
    use std::io::Cursor;

    use rand::{RngCore, SeedableRng};
    use rand_xorshift::XorShiftRng;
    use rstest::rstest;

    use super::*;

    #[test]
    fn filler_pieces_for_sizes() {
        assert_eq!(
            Vec::<filecoin_proofs::PieceInfo>::new(),
            filler_pieces(PaddedBytesAmount(0).into())
        );

        assert_eq!(
            vec![
                // Piece Sizes need to be Power of Twos, at least 128.
                filecoin_proofs::pieces::zero_padding(PaddedBytesAmount(128).into()).unwrap(),
                filecoin_proofs::pieces::zero_padding(PaddedBytesAmount(256).into()).unwrap(),
                filecoin_proofs::pieces::zero_padding(PaddedBytesAmount(512).into()).unwrap(),
                filecoin_proofs::pieces::zero_padding(PaddedBytesAmount(1024).into()).unwrap(),
            ],
            filler_pieces((PaddedBytesAmount(2048) - PaddedBytesAmount(128)).into())
        );
    }

    #[rstest]
    // Only one 'real' small piece is added, rest should be padding pieces.
    #[case(vec![128])]
    // Not aligned to the left pieces
    #[case(vec![128, 256])]
    // Not aligned to the right pieces
    #[case(vec![1024, 256])]
    // Biggest possible piece size
    #[case(vec![2048])]
    fn padding_for_sector(#[case] piece_sizes: Vec<usize>) {
        let sealer = Sealer::new(RegisteredSealProof::StackedDRG2KiBV1P1);

        let piece_infos: Vec<(Cursor<Vec<u8>>, primitives_commitment::piece::PieceInfo)> =
            piece_sizes
                .into_iter()
                .map(|size| {
                    let (piece_bytes, piece_info) =
                        piece_with_random_data(PaddedBytesAmount(size as u64));

                    (Cursor::new(piece_bytes), piece_info)
                })
                .collect();

        // Create a file-like sector where non-occupied bytes are 0
        let sector_size = sealer.porep_config.sector_size.0 as usize;
        let mut staged_sector = vec![0u8; sector_size];

        let pieces: Vec<filecoin_proofs::PieceInfo> = sealer
            .create_sector(piece_infos, Cursor::new(&mut staged_sector))
            .unwrap()
            .into_iter()
            .map(|p| p.into())
            .collect();

        let pieces_commd =
            filecoin_proofs::compute_comm_d(sealer.porep_config.sector_size, &pieces).unwrap();
        let data_commd = compute_data_comm_d(sealer.porep_config.sector_size, &staged_sector);
        assert_eq!(data_commd, pieces_commd)
    }

    /// Generates a piece of `size` and a PieceInfo for it
    fn piece_with_random_data(
        size: PaddedBytesAmount,
    ) -> (Vec<u8>, primitives_commitment::piece::PieceInfo) {
        let rng = &mut XorShiftRng::from_seed(filecoin_proofs::TEST_SEED);

        let piece_size: UnpaddedBytesAmount = size.into();
        let mut piece_bytes = vec![0u8; piece_size.0 as usize];
        rng.fill_bytes(&mut piece_bytes);
        let piece_info =
            filecoin_proofs::generate_piece_commitment(Cursor::new(&mut piece_bytes), piece_size)
                .unwrap();

        (
            piece_bytes,
            primitives_commitment::piece::PieceInfo::from_filecoin_piece_info(
                piece_info,
                primitives_commitment::CommitmentKind::Piece,
            ),
        )
    }

    /// Computes CommD from the raw data, not from the pieces.
    fn compute_data_comm_d(
        sector_size: filecoin_proofs::SectorSize,
        data: &[u8],
    ) -> filecoin_proofs::Commitment {
        let data_tree: filecoin_proofs::DataTree =
            storage_proofs_core::merkle::create_base_merkle_tree::<filecoin_proofs::DataTree>(
                None,
                sector_size.0 as usize / storage_proofs_core::util::NODE_SIZE,
                data,
            )
            .expect("failed to create data tree");
        let comm_d_root: blstrs::Scalar = data_tree.root().into();
        filecoin_proofs::commitment_from_fr(comm_d_root)
    }
}
