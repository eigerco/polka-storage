pub mod clients;
pub mod multipair;
pub mod runtime;
pub mod types;

pub use crate::{
    clients::{MarketClientExt, StorageProviderClientExt, SystemClientExt},
    runtime::{bounded_vec::IntoBoundedByteVec, client::Client},
};

/// Currency as specified by the SCALE-encoded runtime.
pub type Currency = u128;

/// BlockNumber as specified by the SCALE-encoded runtime.
pub type BlockNumber = u64;

/// Parachain configuration for subxt.
#[derive(Debug)]
pub enum PolkaStorageConfig {}

// Types are fully qualified ON PURPOSE!
// It's not fun to find out where in your config a type comes from subxt or frame_support
// going up and down, in and out the files, this helps!
impl subxt::Config for PolkaStorageConfig {
    type Hash = subxt::utils::H256;
    type AccountId = subxt::ext::sp_core::crypto::AccountId32;
    type Address = subxt::config::polkadot::MultiAddress<Self::AccountId, u32>;
    type Signature = subxt::ext::sp_runtime::MultiSignature;
    type Hasher = subxt::config::substrate::BlakeTwo256;
    type Header = subxt::config::substrate::SubstrateHeader<
        BlockNumber,
        subxt::config::substrate::BlakeTwo256,
    >;
    type ExtrinsicParams = subxt::config::DefaultExtrinsicParams<Self>;
    type AssetId = u32;
}
