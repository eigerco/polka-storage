use std::path::PathBuf;

use storagext::PolkaStorageConfig;
use subxt::{
    ext::{
        sp_core::Pair,
        sp_runtime::{traits::Verify, MultiSignature as SpMultiSignature},
    },
    tx::PairSigner,
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use zombienet_configuration::shared::node::{Buildable, Initial, NodeConfigBuilder};
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

/// Find the the `polka_storage_node` in the current project.
///
/// If the feature
pub fn find_polka_storage_node() -> Option<PathBuf> {
    // We're expecting the test binary to always be under /target/X/...
    let current_exe = std::env::current_exe()
        .unwrap()
        // canonicalize to ensure following paths are always canonical
        .canonicalize()
        .unwrap();

    let mut target_folder: Option<PathBuf> = None;
    for parent in current_exe.ancestors() {
        if parent.ends_with("target") {
            target_folder = Some(parent.to_path_buf());
            break;
        }
    }
    let target_folder = target_folder.expect("no target/ directory found");

    if cfg!(feature = "target-release") {
        let release_polka_storage_node = target_folder.join("release").join("polka-storage-node");
        if release_polka_storage_node.exists() {
            tracing::info!("found {}, using it", release_polka_storage_node.display());
            return Some(release_polka_storage_node);
        }
    }

    if cfg!(feature = "target-release") {
        let debug_polka_storage_node = target_folder.join("debug").join("polka-storage-node");
        if debug_polka_storage_node.exists() {
            tracing::info!("found {}, using it", debug_polka_storage_node.display());
            return Some(debug_polka_storage_node);
        }
    }

    return None;
}

pub trait NodeConfigBuilderExt {
    /// Build a node with the given name.
    fn polkadot_node(self, name: &str) -> NodeConfigBuilder<Buildable>;

    /// Build a Polka Storage collator with the given name.
    fn polka_storage_collator(self, name: &str, command: &str) -> NodeConfigBuilder<Buildable>;
}

impl NodeConfigBuilderExt for NodeConfigBuilder<Initial> {
    fn polkadot_node(self, name: &str) -> NodeConfigBuilder<Buildable> {
        self.with_name(name)
            .validator(true)
            .with_command("polkadot")
            // You can customize the log level for a given module using
            // -lpackage1=level1,package2=level2
            // You can read more about the available targets in:
            // https://wiki.polkadot.network/docs/build-node-management#monitoring-and-telemetry
            .with_args(vec!["-lparachain=trace,runtime=trace".into()])
    }

    fn polka_storage_collator(self, name: &str, command: &str) -> NodeConfigBuilder<Buildable> {
        self.with_name(name)
            .with_command(command)
            .with_args(vec![
                "--detailed-log-output".into(),
                "-lparachain=trace,runtime=trace".into(),
            ])
            .validator(true)
    }
}

/// This configuration is supposed to be a 1:1 copy of `zombienet/local-testnet.toml`.
///
/// We could use the TOML file if wasn't for not having the same requirements as this description,
/// for example, when reading the TOML file, [you need to explicitly set a timeout](https://github.com/paritytech/zombienet-sdk/issues/254)
pub fn local_testnet_config() -> NetworkConfig {
    let binding = find_polka_storage_node()
        .expect("couldn't find the polka-storage-node binary")
        .display()
        .to_string();
    let polka_storage_node_binary_path = binding.as_str();

    NetworkConfigBuilder::new()
        .with_relaychain(|relaychain| {
            relaychain
                .with_chain("rococo-local")
                .with_node(|node| node.polkadot_node("relay-1"))
                .with_node(|node| node.polkadot_node("relay-2"))
        })
        .with_parachain(|parachain| {
            parachain
                .with_id(1000)
                .cumulus_based(true)
                .with_collator(|collator| {
                    collator.polka_storage_collator("collator", polka_storage_node_binary_path)
                })
        })
        .build()
        .unwrap()
}

/// Setup logging for tests. Will panic if called multiple times!
pub fn setup_logging() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .from_env()
        .expect("valid level should be set");

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(env_filter))
        .init();
}

pub fn pair_signer_from_str<P>(s: &str) -> PairSigner<PolkaStorageConfig, P>
where
    P: Pair,
    <SpMultiSignature as Verify>::Signer: From<P::Public>,
{
    let keypair = Pair::from_string(s, None).unwrap();
    PairSigner::<PolkaStorageConfig, P>::new(keypair)
}
