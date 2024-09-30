use std::path::Path;

use bellperson::groth16;
use blstrs::Bls12;
use filecoin_hashers::Domain;
use filecoin_proofs::{
    add_piece, as_safe_commitment, parameters::setup_params, DefaultBinaryTree, DefaultPieceDomain,
    DefaultPieceHasher, PoRepConfig, SealCommitPhase1Output, SealPreCommitOutput,
    SealPreCommitPhase1Output, UnpaddedBytesAmount,
};
use primitives_proofs::{RegisteredSealProof, SectorNumber};
use storage_proofs_core::{compound_proof, compound_proof::CompoundProof};
use storage_proofs_porep::stacked::{self, StackedCompound, StackedDrg};

use super::{seal_to_config, PoRepError};
use crate::types::{Commitment, PieceInfo, ProverId, Ticket};

pub struct Sealer {
    porep_config: PoRepConfig,
}

impl Sealer {
    pub fn new(seal_proof: RegisteredSealProof) -> Self {
        Self {
            porep_config: seal_to_config(seal_proof),
        }
    }

    // TODO(@th7nder,#420,02/10/2024): this ain't working properly. it only works when pieces.len() == 1 and piece_size == 2032 == sector_size
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
    ) -> Result<(), PoRepError> {
        if pieces.len() == 0 {
            return Err(PoRepError::EmptySector);
        }

        let mut piece_lengths: Vec<UnpaddedBytesAmount> = Vec::new();
        if pieces.len() > 1
            || UnpaddedBytesAmount(pieces[0].1.size as u64) != self.porep_config.sector_size.into()
        {
            todo!(
                "known bug: issue#420 piece_size {} != 2032",
                pieces[0].1.size
            );
        }

        for (idx, (reader, piece)) in pieces.into_iter().enumerate() {
            let piece: filecoin_proofs::PieceInfo = piece.into();
            let (calculated_piece_info, _) =
                add_piece(reader, &mut unsealed_sector, piece.size, &piece_lengths).unwrap();

            piece_lengths.push(piece.size);

            if piece.commitment != calculated_piece_info.commitment {
                return Err(PoRepError::InvalidPieceCid(
                    idx,
                    piece.commitment,
                    calculated_piece_info.commitment,
                ));
            }
        }

        Ok(())
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
            .map(|p| p.clone().into())
            .collect::<Vec<filecoin_proofs::PieceInfo>>();

        let p1_output: SealPreCommitPhase1Output<DefaultBinaryTree> =
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
            .map(|p| p.clone().into())
            .collect::<Vec<filecoin_proofs::PieceInfo>>();

        let scp1: filecoin_proofs::SealCommitPhase1Output<filecoin_proofs::DefaultBinaryTree> =
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
            <StackedCompound<DefaultBinaryTree, DefaultPieceHasher> as CompoundProof<
                StackedDrg<'_, DefaultBinaryTree, DefaultPieceHasher>,
                _,
            >>::setup(&compound_setup_params)?;

        let groth_proofs =
            StackedCompound::<DefaultBinaryTree, DefaultPieceHasher>::circuit_proofs(
                &public_inputs,
                vanilla_proofs,
                &compound_public_params.vanilla_params,
                proving_parameters,
                compound_public_params.priority,
            )?;

        Ok(groth_proofs)
    }
}

/// Public inputs for a PoRep.
/// References:
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/storage-proofs-porep/src/stacked/vanilla/params.rs#L832>]
/// * <https://github.com/filecoin-project/rust-fil-proofs/blob/266acc39a3ebd6f3d28c6ee335d78e2b7cea06bc/filecoin-proofs/src/types/mod.rs#L53>
#[derive(Debug, Clone)]
pub struct PreCommitOutput {
    pub comm_r: Commitment,
    pub comm_d: Commitment,
}
