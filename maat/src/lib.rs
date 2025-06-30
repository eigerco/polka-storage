use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};

use ed25519_dalek::{
    pkcs8::{spki::der::pem::LineEnding, EncodePrivateKey},
    SigningKey,
};
use sp_core::Pair;
use sp_runtime::{traits::Verify, MultiSignature as SpMultiSignature};
use storagext::{pair_signer::PairSigner, PolkaStorageConfig};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use zombienet_configuration::shared::node::{Buildable, Initial, NodeConfigBuilder};
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

/// Network's collator name. Used for logs and so on.
pub const COLLATOR_NAME: &str = "collator";

/// Find the the `polka_storage_node` in the current project.
///
/// If the feature `target-release` is enabled, this function will look for the `release` build,
/// likewise, if the `target-debug` feature is enabled it will look for the `debug` build.
/// If both are enabled, the `release` build takes priority, if none are enabled, this function
/// returns `None`, effectively failing.
pub fn find_polka_storage_node() -> Option<PathBuf> {
    // We're expecting the test binary to always be under /target/X/...
    let current_exe = std::env::current_exe()
        .unwrap()
        // canonicalize to ensure following paths are always canonical
        .canonicalize()
        .unwrap();

    let target_folder = current_exe
        .ancestors()
        .find(|parent| parent.ends_with("target"))
        .expect("no target/ directory found");

    if cfg!(feature = "target-release") {
        let release_polka_storage_node = target_folder.join("release").join("polka-storage-node");
        if release_polka_storage_node.exists() {
            tracing::info!("found {}, using it", release_polka_storage_node.display());
            return Some(release_polka_storage_node);
        }
    }

    if cfg!(feature = "target-debug") {
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
pub fn local_testnet_config(temp_dir_path: &std::path::Path) -> NetworkConfig {
    let binding = find_polka_storage_node()
        .expect("couldn't find the polka-storage-node binary")
        .display()
        .to_string();
    let polka_storage_node_binary_path = binding.as_str();
    let file_path = temp_dir_path.join("private_key.pem");
    generate_pem_file(&file_path);

    NetworkConfigBuilder::new()
        .with_relaychain(|relaychain| {
            relaychain
                .with_chain("rococo-local")
                .with_chain_spec_path("../zombienet/rococo-local.json")
                .with_node(|node| node.polkadot_node("relay-1"))
                .with_node(|node| node.polkadot_node("relay-2"))
        })
        .with_parachain(|parachain| {
            parachain
                .with_id(1000)
                .cumulus_based(true)
                .with_collator(|collator| {
                    collator
                        .polka_storage_collator(COLLATOR_NAME, polka_storage_node_binary_path)
                        .with_args(vec![
                            ("--pool-type", "fork-aware").into(),
                            ("-lruntime=trace,parachain=debug").into(),
                            ("--p2p-tcp-listen-address=/ip4/127.0.0.1/tcp/62649").into(),
                            ("--bootstrap-addresses=/ip4/127.0.0.1/tcp/1337").into(),
                            (format!("--p2p-key=@{}", file_path.display()).as_str()).into(),
                        ])
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

fn generate_pem_file<P: AsRef<Path>>(path: P) {
    let signing_key = SigningKey::from([0; 32]);
    let pem = signing_key.to_pkcs8_pem(LineEnding::default()).unwrap();
    let mut file = File::create(path).unwrap();
    write!(file, "{}", *pem).unwrap();
}
