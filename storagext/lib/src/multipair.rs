use std::fmt::Debug;

use subxt::{
    ext::sp_core::{
        crypto::Ss58Codec, ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair,
        sr25519::Pair as Sr25519Pair,
    },
    tx::PairSigner,
};

use crate::PolkaStorageConfig;

/// Similar to other `Multi` types from Polkadot, this one wraps over [`PairSigner`],
/// allowing keypairs to sign data.
#[derive(Clone)]
pub enum MultiPairSigner {
    Sr25519(PairSigner<PolkaStorageConfig, Sr25519Pair>),
    ECDSA(PairSigner<PolkaStorageConfig, ECDSAPair>),
    Ed25519(PairSigner<PolkaStorageConfig, Ed25519Pair>),
}

impl MultiPairSigner {
    /// Attempt to convert one of the possible keypairs into a [`MultiPairSigner`],
    /// conversion order is the same as the parameter order, if all parameters are `None`,
    /// this function returns `None`.
    pub fn new(
        sr25519_key: Option<Sr25519Pair>,
        ecdsa_key: Option<ECDSAPair>,
        ed25519_key: Option<Ed25519Pair>,
    ) -> Option<Self> {
        match (sr25519_key, ecdsa_key, ed25519_key) {
            (Some(key), _, _) => Some(Self::Sr25519(PairSigner::new(key))),
            (_, Some(key), _) => Some(Self::ECDSA(PairSigner::new(key))),
            (_, _, Some(key)) => Some(Self::Ed25519(PairSigner::new(key))),
            _ => None,
        }
    }
}

impl Debug for MultiPairSigner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sr25519(arg0) => f.debug_tuple("Sr25519").field(arg0.account_id()).finish(),
            Self::ECDSA(arg0) => f.debug_tuple("ECDSA").field(arg0.account_id()).finish(),
            Self::Ed25519(arg0) => f.debug_tuple("Ed25519").field(arg0.account_id()).finish(),
        }
    }
}

impl subxt::tx::Signer<PolkaStorageConfig> for MultiPairSigner {
    fn account_id(&self) -> <PolkaStorageConfig as subxt::Config>::AccountId {
        match self {
            Self::Sr25519(signer) => subxt::tx::Signer::account_id(signer),
            Self::ECDSA(signer) => subxt::tx::Signer::account_id(signer),
            Self::Ed25519(signer) => subxt::tx::Signer::account_id(signer),
        }
    }
    fn address(&self) -> <PolkaStorageConfig as subxt::Config>::Address {
        match self {
            Self::Sr25519(signer) => subxt::tx::Signer::address(signer),
            Self::ECDSA(signer) => subxt::tx::Signer::address(signer),
            Self::Ed25519(signer) => subxt::tx::Signer::address(signer),
        }
    }

    fn sign(&self, signer_payload: &[u8]) -> <PolkaStorageConfig as subxt::Config>::Signature {
        match self {
            Self::Sr25519(signer) => subxt::tx::Signer::sign(signer, signer_payload),
            Self::ECDSA(signer) => subxt::tx::Signer::sign(signer, signer_payload),
            Self::Ed25519(signer) => subxt::tx::Signer::sign(signer, signer_payload),
        }
    }
}

/// [`DebugPair`] is a wrapper over types implementing [`Pair`](subxt::ext::sp_core::Pair),
/// it provides a [`Debug`](std::fmt::Debug) which is required by `clap`.
#[derive(Clone, PartialEq, Eq)]
pub struct DebugPair<Pair>(pub(crate) Pair)
where
    Pair: subxt::ext::sp_core::Pair;

impl<Pair> std::fmt::Debug for DebugPair<Pair>
where
    Pair: subxt::ext::sp_core::Pair,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("DebugPair")
            .field(&self.0.public().to_ss58check())
            .finish()
    }
}

impl<Pair> DebugPair<Pair>
where
    Pair: subxt::ext::sp_core::Pair,
{
    /// `clap`'s custom parsing function.
    // NOTE(@jmg-duarte): not added to the `clap` module since we always want to test this
    #[cfg(any(feature = "clap", test))]
    pub fn value_parser(src: &str) -> Result<Self, String> {
        Ok(Self(Pair::from_string(&src, None).map_err(|err| {
            format!("failed to parse pair from string: {}", err)
        })?))
    }

    /// Consumes [`DebugPair`] and returns the inner `Pair`.
    // NOTE: implementing `Into<Pair>` for `DebugPair<Pair> where Pair: subxt::ext::sp_core::Pair`
    // does not work because it conflicts with `Into<U> for T where U: From<T>` even though using
    // `from` does not actually work...
    pub fn into_inner(self) -> Pair {
        self.0
    }
}

#[cfg(feature = "clap")]
mod clap {
    use super::{DebugPair, ECDSAPair, Ed25519Pair, MultiPairSigner, Sr25519Pair};

    #[derive(Debug, Clone, clap::Args)]
    pub struct MultiPairArgs {
        /// Sr25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
        ///
        /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
        #[arg(long, value_parser = DebugPair::<Sr25519Pair>::value_parser)]
        pub sr25519_key: Option<DebugPair<Sr25519Pair>>,

        /// ECDSA keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
        ///
        /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
        #[arg(long, value_parser = DebugPair::<ECDSAPair>::value_parser)]
        pub ecdsa_key: Option<DebugPair<ECDSAPair>>,

        /// Ed25519 keypair, encoded as hex, BIP-39 or a dev phrase like `//Alice`.
        ///
        /// See `sp_core::crypto::Pair::from_string_with_seed` for more information.
        #[arg(long, value_parser = DebugPair::<Ed25519Pair>::value_parser)]
        pub ed25519_key: Option<DebugPair<Ed25519Pair>>,
    }

    impl From<MultiPairArgs> for Option<MultiPairSigner> {
        fn from(value: MultiPairArgs) -> Self {
            MultiPairSigner::new(
                value.sr25519_key.map(DebugPair::into_inner),
                value.ecdsa_key.map(DebugPair::into_inner),
                value.ed25519_key.map(DebugPair::into_inner),
            )
        }
    }
}

// Export this as part of the current module if `clap` is enabled
#[cfg(feature = "clap")]
pub use self::clap::*;

#[cfg(test)]
mod test {
    //! These tests basically ensure that the underlying parsers aren't broken without warning.

    use subxt::ext::sp_core::{
        ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair, sr25519::Pair as Sr25519Pair,
    };

    use crate::multipair::DebugPair;

    #[track_caller]
    fn assert_debug_pair<P>(s: &str)
    where
        P: subxt::ext::sp_core::Pair,
    {
        let result_pair = DebugPair::<P>::value_parser(s).unwrap();
        let expect_pair = P::from_string(s, None).unwrap();

        assert_eq!(result_pair.0.to_raw_vec(), expect_pair.to_raw_vec());
    }

    #[test]
    fn deserialize_debug_pair_sr25519() {
        assert_debug_pair::<Sr25519Pair>("//Alice");
        // https://docs.substrate.io/reference/glossary/#dev-phrase
        // link visited on 23/7/2024 (you never know when Substrate's docs will become stale)
        assert_debug_pair::<Sr25519Pair>(
            "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
        );
        // secret seed for testing purposes
        assert_debug_pair::<Sr25519Pair>(
            "0xd045270857659c84705fbb367fd9644e5ab9b0c668f37c0bf28c6e72a120dd1f",
        );
    }

    #[test]
    fn deserialize_debug_pair_ecdsa() {
        assert_debug_pair::<ECDSAPair>("//Alice");
        // https://docs.substrate.io/reference/glossary/#dev-phrase
        // link visited on 23/7/2024 (you never know when Substrate's docs will become stale)
        assert_debug_pair::<ECDSAPair>(
            "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
        );
        // secret seed for testing purposes
        assert_debug_pair::<ECDSAPair>(
            "0xd045270857659c84705fbb367fd9644e5ab9b0c668f37c0bf28c6e72a120dd1f",
        );
    }

    #[test]
    fn deserialize_debug_pair_ed25519() {
        assert_debug_pair::<Ed25519Pair>("//Alice");
        // https://docs.substrate.io/reference/glossary/#dev-phrase
        // link visited on 23/7/2024 (you never know when Substrate's docs will become stale)
        assert_debug_pair::<Ed25519Pair>(
            "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
        );
        // secret seed for testing purposes
        assert_debug_pair::<Ed25519Pair>(
            "0xd045270857659c84705fbb367fd9644e5ab9b0c668f37c0bf28c6e72a120dd1f",
        );
    }
}
