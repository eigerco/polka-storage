use cumulus_pallet_parachain_system::{RelayChainStateProof, RelayStateProof, ValidationData};
use sp_core::{hex2array, Get};

use crate::ParachainInfo;

/// Storage Key for BABE's [`AuthorVrfRandomness`][1] — taken from the Polkadot UI for Relay Chain's
/// version 8 and `pallet-babe` version `28.0.0`.
///
/// For more information on fetching runtime storage values, see:
/// * <https://docs.substrate.io/build/runtime-storage/#accessing-storage-items>
///
/// [1]: https://github.com/paritytech/polkadot-sdk/blob/5b04b4598cc7b2c8e817a6304c7cdfaf002c1fee/substrate/frame/babe/src/lib.rs#L268-L273
pub(crate) const AUTHOR_VRF_STORAGE_KEY: [u8; 32] =
    hex2array!("1cb6f36e027abb2091cfb5110ab5087fd077dfdb8adb10f78f10a5df8742c545");

/// Only callable after `set_validation_data` is called which forms this proof the same way
pub(crate) fn relay_chain_state_proof<Runtime>() -> RelayChainStateProof
where
    Runtime: cumulus_pallet_parachain_system::Config,
{
    let relay_storage_root = ValidationData::<Runtime>::get()
        .expect("set in `set_validation_data`")
        .relay_parent_storage_root;
    let relay_chain_state =
        RelayStateProof::<Runtime>::get().expect("set in `set_validation_data`");
    RelayChainStateProof::new(ParachainInfo::get(), relay_storage_root, relay_chain_state)
        .expect("Invalid relay chain state proof, already constructed in `set_validation_data`")
}

pub struct BabeDataGetter<Runtime>(sp_std::marker::PhantomData<Runtime>);

impl<Runtime> pallet_randomness::GetAuthorVrf<Runtime::Hash> for BabeDataGetter<Runtime>
where
    Runtime: cumulus_pallet_parachain_system::Config,
{
    // Tolerate panic here because only ever called in inherent (so can be omitted)
    fn get_author_vrf() -> Option<Runtime::Hash> {
        if cfg!(feature = "runtime-benchmarks") {
            // storage reads as per actual reads
            let _relay_storage_root = ValidationData::<Runtime>::get();
            let _relay_chain_state = RelayStateProof::<Runtime>::get();
            return Some(Default::default());
        }
        relay_chain_state_proof::<Runtime>()
            .read_optional_entry(&AUTHOR_VRF_STORAGE_KEY)
            .ok()
            .flatten()
            .expect("expected to be able to read epoch index from relay chain state proof")
    }
}
