use alloc::{collections::BTreeMap, vec, vec::Vec};

use cumulus_primitives_core::ParaId;
use parachains_common::AuraId;
use polka_storage_proofs::{Bls12, VerifyingKey};
use primitives::proofs::{RegisteredPoStProof, RegisteredSealProof};
use serde_json::Value;
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;

use crate::{
    AccountId, Balance, BalancesConfig, CollatorSelectionConfig, ParachainInfoConfig,
    PolkadotXcmConfig, ProofsConfig, RuntimeGenesisConfig, SessionConfig, SessionKeys, SudoConfig,
    EXISTENTIAL_DEPOSIT,
};

/// The default XCM version to set in genesis config.
const SAFE_XCM_VERSION: u32 = xcm::prelude::XCM_VERSION;

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn template_session_keys(keys: AuraId) -> SessionKeys {
    SessionKeys { aura: keys }
}

fn testnet_genesis(
    invulnerables: BTreeMap<AccountId, AuraId>,
    endowed_accounts: BTreeMap<AccountId, Balance>,
    root: AccountId,
) -> Value {
    let post_1gib_vk = include_bytes!("../../test-fixtures/keys/1GiB.post.vk.scale");
    let post_1gib_vk = VerifyingKey::<Bls12>::from_bytes(post_1gib_vk).unwrap();

    let porep_1gib_vk = include_bytes!("../../test-fixtures/keys/1GiB.porep.vk.scale");
    let porep_1gib_vk = VerifyingKey::<Bls12>::from_bytes(porep_1gib_vk).unwrap();

    let post_8mib_vk = include_bytes!("../../test-fixtures/keys/8MiB.post.vk.scale");
    let post_8mib_vk = VerifyingKey::<Bls12>::from_bytes(post_8mib_vk).unwrap();

    let porep_8mib_vk = include_bytes!("../../test-fixtures/keys/8MiB.porep.vk.scale");
    let porep_8mib_vk = VerifyingKey::<Bls12>::from_bytes(porep_8mib_vk).unwrap();

    let config = RuntimeGenesisConfig {
        balances: BalancesConfig {
            balances: endowed_accounts.into_iter().collect(),
        },
        parachain_info: ParachainInfoConfig {
            // There is no reasonable default here - but at least 1000 is taken by AssetHub, so it should
            // error if it's ever used in a real environment. Having a default is convenient for development.
            parachain_id: ParaId::new(1000),
            ..Default::default()
        },
        collator_selection: CollatorSelectionConfig {
            invulnerables: invulnerables.keys().cloned().collect::<Vec<_>>(),
            candidacy_bond: EXISTENTIAL_DEPOSIT * 16,
            ..Default::default()
        },
        session: SessionConfig {
            keys: invulnerables
                .into_iter()
                .map(|(acc, aura)| {
                    (
                        acc.clone(),                 // account id
                        acc,                         // validator id
                        template_session_keys(aura), // session keys
                    )
                })
                .collect::<Vec<_>>(),
            ..Default::default()
        },
        polkadot_xcm: PolkadotXcmConfig {
            safe_xcm_version: Some(SAFE_XCM_VERSION),
            ..Default::default()
        },
        sudo: SudoConfig { key: Some(root) },
        proofs: ProofsConfig {
            post_keys: [
                (RegisteredPoStProof::StackedDRGWindow1GiBV1, post_1gib_vk),
                (RegisteredPoStProof::StackedDRGWindow8MiBV1, post_8mib_vk),
            ]
            .into(),
            porep_keys: [
                (RegisteredSealProof::StackedDRG1GiBV1, porep_1gib_vk),
                (RegisteredSealProof::StackedDRG8MiBV1, porep_8mib_vk),
            ]
            .into(),
            ..Default::default()
        },
        ..Default::default()
    };

    serde_json::to_value(config).expect("Could not build genesis config.")
}

fn local_testnet_genesis() -> Value {
    testnet_genesis(
        // initial collators.
        [
            (
                Sr25519Keyring::Alice.to_account_id(),
                Sr25519Keyring::Alice.public().into(),
            ),
            (
                Sr25519Keyring::Bob.to_account_id(),
                Sr25519Keyring::Bob.public().into(),
            ),
        ]
        .into(),
        Sr25519Keyring::well_known()
            .map(|k| (k.to_account_id(), (1u128 << 60) as Balance))
            .chain(
                // This is a nice default for `maat/tests/real_world.rs`.
                // Add balance to Charlie - Storage Provider.
                // Collateral (12 500 000) + pre_commit_deposit (1)
                // 12 500 000 == deal.provider_collateral
                // 1 == pallets/storage-provider/lib.rs:calculate_pre_commit_deposit
                [
                    (
                        Sr25519Keyring::Alice.to_account_id(),
                        25_000_000_000 as Balance,
                    ),
                    (
                        Sr25519Keyring::Bob.to_account_id(),
                        12_500_000_001 as Balance,
                    ),
                    (
                        Sr25519Keyring::Charlie.to_account_id(),
                        12_500_000_001 as Balance,
                    ),
                ],
            )
            // Add funds to the pallet account as we're adding some balance by default to it in its genesis Config.
            .collect(),
        Sr25519Keyring::Alice.to_account_id(),
    )
}

fn development_config_genesis() -> Value {
    testnet_genesis(
        // initial collators.
        [
            (
                Sr25519Keyring::Alice.to_account_id(),
                Sr25519Keyring::Alice.public().into(),
            ),
            (
                Sr25519Keyring::Bob.to_account_id(),
                Sr25519Keyring::Bob.public().into(),
            ),
        ]
        .into(),
        Sr25519Keyring::well_known()
            .map(|k| (k.to_account_id(), (1u128 << 60) as Balance))
            .collect(),
        Sr25519Keyring::Alice.to_account_id(),
    )
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<vec::Vec<u8>> {
    let patch = match id.as_ref() {
        sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => local_testnet_genesis(),
        sp_genesis_builder::DEV_RUNTIME_PRESET => development_config_genesis(),
        _ => return None,
    };
    Some(
        serde_json::to_string(&patch)
            .expect("serialization to json is expected to work. qed.")
            .into_bytes(),
    )
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
    vec![
        PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
        PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
    ]
}

#[cfg(test)]
mod tests {
    // NOTE(@Jinxit,02/06/2025): Excluded because it is too slow to run coverage for on CI.
    #[cfg(not(tarpaulin))]
    #[test]
    fn check_presets() {
        let builder = sc_chain_spec::GenesisConfigBuilderRuntimeCaller::<()>::new(
            crate::WASM_BINARY.expect("wasm binary shall exists"),
        );
        assert!(builder
            .get_storage_for_named_preset(Some(&sp_genesis_builder::DEV_RUNTIME_PRESET.to_string()))
            .is_ok());
    }
}
