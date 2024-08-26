use std::fmt::Debug;

use storagext::PolkaStorageConfig;
use subxt::{
    ext::sp_core::{
        crypto::Ss58Codec, ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair,
        sr25519::Pair as Sr25519Pair,
    },
    tx::PairSigner,
};

/// [`DebugPair`] is a wrapper over types implementing [`Pair`](subxt::ext::sp_core::Pair),
/// it provides a [`Debug`](std::fmt::Debug) which is required by `clap`.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DebugPair<Pair>(pub(crate) Pair)
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
    pub fn value_parser(src: &str) -> Result<Self, String> {
        Ok(Self(Pair::from_string(&src, None).map_err(|err| {
            format!("failed to parse pair from string: {}", err)
        })?))
    }

    /// Consumes [`DebugPair`] and returns the inner `Pair`.
    pub fn into_inner(self) -> Pair {
        self.0
    }
}

/// Similar to other `Multi` types from Polkadot, this one wraps over [`PairSigner`],
/// allowing keypairs to sign data.
pub(crate) enum MultiPairSigner {
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
