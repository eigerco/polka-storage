//! All utility functions needed by PoRep and PoSt related methods.

use std::path::PathBuf;

use blstrs::Scalar as Fr;
use ff::Field;
use filecoin_hashers::Hasher;
use merkletree::store::StoreConfig;
use rand_xorshift::XorShiftRng;
use storage_proofs_core::{cache_key::CacheKey, merkle::MerkleTreeTrait};

/// Currently, this method returns the path for temporary files.
/// Might be adapted later to a meaningful path, i.e. of the stored data.
pub fn cache_dir() -> PathBuf {
    tempfile::tempdir().expect("expect tempdir()").into_path()
}

/// Method returns a merkletree::StoreConfig struct.
pub fn store_config() -> StoreConfig {
    StoreConfig::new(&cache_dir(), CacheKey::CommDTree.to_string(), 0)
}

/// Method returns the path of replicas.
pub fn replica_path(store_config: &StoreConfig) -> PathBuf {
    let path = &store_config.path;
    path.join("replicate-path")
}

/// Method generates a random 32-Byte ID.
pub fn generate_random_id<TTree: MerkleTreeTrait>(rng: &mut XorShiftRng) -> [u8; 32] {
    let mut id = [0u8; 32];
    let fr: <<TTree as MerkleTreeTrait>::Hasher as Hasher>::Domain = Fr::random(rng).into();
    id.copy_from_slice(AsRef::<[u8]>::as_ref(&fr));
    id
}
