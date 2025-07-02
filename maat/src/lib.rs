use sp_core::Pair;
use sp_runtime::{traits::Verify, MultiSignature as SpMultiSignature};
use storagext::{pair_signer::PairSigner, PolkaStorageConfig};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use zombienet_configuration::shared::node::{Buildable, Initial, NodeConfigBuilder};
use zombienet_sdk::{NetworkConfig, NetworkConfigBuilder};

/// Network's collator name. Used for logs and so on.
pub const COLLATOR_NAME: &str = "collator";

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
                .with_chain_spec_path("../chain_spec.json")
                .with_collator(|collator| {
                    collator
                        .polka_storage_collator(COLLATOR_NAME, "polkadot-omni-node")
                        .with_args(vec![
                            ("--pool-type", "fork-aware").into(),
                            ("-lruntime=trace,parachain=debug").into(),
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
