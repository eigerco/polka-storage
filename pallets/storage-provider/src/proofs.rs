use codec::{Decode, Encode};
use frame_support::{
    pallet_prelude::{ConstU32, RuntimeDebug},
    sp_runtime::BoundedVec,
};
use primitives::{
    proofs::RegisteredPoStProof, PartitionNumber, MAX_PARTITIONS_PER_DEADLINE,
    MAX_POST_PROOFS_PER_BLOCK, MAX_POST_PROOF_BYTES,
};
use scale_info::TypeInfo;

/// Proof of Spacetime data stored on chain.
#[derive(RuntimeDebug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct PoStProof {
    /// The proof type, currently only one type is supported.
    pub post_proof: RegisteredPoStProof,
    /// The proof submission, to be checked by [`ProofVerification::verify_post`], usually [`pallet_proofs`].
    pub proof_bytes: BoundedVec<u8, ConstU32<MAX_POST_PROOF_BYTES>>,
}

/// Parameter type for `submit_windowed_post` extrinsic.
// In filecoind the proof is an array of proofs, one per distinct registered proof type present in the sectors being proven.
// Reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/miner/src/types.rs#L114-L115>
// We differ here from Filecoin and do not support registration of different proof types.
#[derive(RuntimeDebug, Decode, Encode, TypeInfo, PartialEq, Eq, Clone)]
pub struct SubmitWindowedPoStParams {
    /// The deadline index which the submission targets.
    pub deadline: u64,
    /// The partition being proven.
    pub partitions: BoundedVec<PartitionNumber, ConstU32<MAX_PARTITIONS_PER_DEADLINE>>,
    /// The proof submission.
    pub proofs: BoundedVec<PoStProof, ConstU32<MAX_POST_PROOFS_PER_BLOCK>>,
}
