use std::path::PathBuf;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use zombienet_configuration::shared::node::{Buildable, Initial, NodeConfigBuilder};
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

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

    let release_polka_storage_node = target_folder.join("release").join("polka-storage-node");
    tracing::debug!(
        "searching for polka-storage-node in {}",
        release_polka_storage_node.display()
    );
    if release_polka_storage_node.exists() {
        return Some(release_polka_storage_node);
    }

    let debug_polka_storage_node = target_folder.join("debug").join("polka-storage-node");
    tracing::debug!(
        "searching for polka-storage-node in {}",
        debug_polka_storage_node.display()
    );
    if debug_polka_storage_node.exists() {
        return Some(debug_polka_storage_node);
    }

    return None;
}

pub trait NodeConfigBuilderExt {
    fn polkadot_node(self, name: &str) -> NodeConfigBuilder<Buildable>;
}

impl NodeConfigBuilderExt for NodeConfigBuilder<Initial> {
    /// Build a node with the given name.
    fn polkadot_node(self, name: &str) -> NodeConfigBuilder<Buildable> {
        self.with_name(name)
            .validator(true)
            .with_command("polkadot")
            .with_args(vec!["-lparachain=trace,runtime=trace".into()])
    }
}

pub fn local_testnet_config() -> NetworkConfig {
    // No comments... Just parity being parity...
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
                    collator
                        .with_name("collator")
                        .with_command(polka_storage_node_binary_path)
                        .with_args(vec![
                            "--detailed-log-output".into(),
                            "-lparachain=trace,runtime=trace".into(),
                        ])
                        .validator(true)
                })
        })
        .build()
        .unwrap()
}

pub fn setup_logging() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::DEBUG.into())
        .from_env()
        .expect("valid level should be set");
    // let module_filter = filter_fn(|metadata| {
    //     metadata
    //         .module_path()
    //         .map(|path| {
    //             path.starts_with("storagext") || path.starts_with("maat")
    //             // || path.starts_with("zombienet")
    //         })
    //         .unwrap_or(true)
    // });

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_filter(env_filter))
        .init();
}
