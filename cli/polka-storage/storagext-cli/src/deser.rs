//! Types in this module are defined to enable deserializing them from the CLI arguments or similar.

use std::fmt::Debug;

use cid::Cid;
use storagext::{BlockNumber, Currency, PolkaStorageConfig};
use subxt::ext::sp_core::crypto::Ss58Codec;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DebugPair<Pair>(pub(crate) Pair)
where
    Pair: subxt::ext::sp_core::Pair;

impl<Pair> Debug for DebugPair<Pair>
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
    pub fn value_parser(src: &str) -> Result<Self, String> {
        Ok(Self(Pair::from_string(&src, None).map_err(|err| {
            format!("failed to parse pair from string: {}", err)
        })?))
    }
}

/// CID doesn't deserialize from a string, hence we need our work wrapper.
///
/// <https://github.com/multiformats/rust-cid/issues/162>
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CidWrapper(Cid);

// The CID has some issues that require a workaround for strings.
// For more details, see: <https://github.com/multiformats/rust-cid/issues/162>
impl<'de> serde::de::Deserialize<'de> for CidWrapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let cid = Cid::try_from(s.as_str()).map_err(|e| {
            serde::de::Error::custom(format!(
                "failed to parse CID, check that the input is a valid CID: {e:?}"
            ))
        })?;
        Ok(Self(cid))
    }
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq, Eq)]
pub(crate) struct DealProposal {
    pub piece_cid: CidWrapper,
    pub piece_size: u64,
    pub client: <PolkaStorageConfig as subxt::Config>::AccountId,
    pub provider: <PolkaStorageConfig as subxt::Config>::AccountId,
    pub label: String,
    pub start_block: BlockNumber,
    pub end_block: BlockNumber,
    pub storage_price_per_block: Currency,
    pub provider_collateral: Currency,
    pub state: storagext::runtime::runtime_types::pallet_market::pallet::DealState<BlockNumber>,
}

impl Into<storagext::DealProposal> for DealProposal {
    fn into(self) -> storagext::DealProposal {
        storagext::DealProposal {
            piece_cid: self.piece_cid.0,
            piece_size: self.piece_size,
            client: self.client,
            provider: self.provider,
            label: self.label,
            start_block: self.start_block,
            end_block: self.end_block,
            storage_price_per_block: self.storage_price_per_block,
            provider_collateral: self.provider_collateral,
            state: self.state.into(),
        }
    }
}

#[cfg(test)]
mod test {
    //! These tests basically ensure that the underlying parsers aren't broken without warning.

    use std::str::FromStr;

    use cid::Cid;
    use storagext::PolkaStorageConfig;
    use subxt::ext::sp_core::{
        ecdsa::Pair as ECDSAPair, ed25519::Pair as Ed25519Pair, sr25519::Pair as Sr25519Pair,
    };

    use super::{CidWrapper, DealProposal, DebugPair};

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

    #[test]
    fn deserialize_cid_json_string() {
        let result_cid = serde_json::from_str::<CidWrapper>(
            "\"bafybeih5zgcgqor3dv6kfdtv3lshv3yfkfewtx73lhedgihlmvpcmywmua\"",
        )
        .unwrap();
        let expect_cid =
            Cid::from_str("bafybeih5zgcgqor3dv6kfdtv3lshv3yfkfewtx73lhedgihlmvpcmywmua").unwrap();
        assert_eq!(result_cid.0, expect_cid);
    }

    #[test]
    fn deserialize_deal_proposal_json_object() {
        let json = r#"
        {
            "piece_cid": "bafkreibme22gw2h7y2h7tg2fhqotaqjucnbc24deqo72b6mkl2egezxhvy",
            "piece_size": 1,
            "client": "5GvHnpY1433RytXW66r77iL4CyewAAErDU6fAouoaPKvcvLU",
            "provider": "5GvHnpY1433RytXW66r77iL4CyewAAErDU6fAouoaPKvcvLU",
            "label": "heyyy",
            "start_block": 30,
            "end_block": 55,
            "storage_price_per_block": 1,
            "provider_collateral": 1,
            "state": "Published"
        }
        "#;
        let result_deal_proposal = serde_json::from_str::<DealProposal>(json).unwrap();

        let piece_cid = CidWrapper(
            Cid::from_str("bafkreibme22gw2h7y2h7tg2fhqotaqjucnbc24deqo72b6mkl2egezxhvy").unwrap(),
        );
        let expect_deal_proposal = DealProposal {
            piece_cid,
            piece_size: 1,
            client: <PolkaStorageConfig as subxt::Config>::AccountId::from_str(
                "5GvHnpY1433RytXW66r77iL4CyewAAErDU6fAouoaPKvcvLU",
            )
            .unwrap(),
            provider: <PolkaStorageConfig as subxt::Config>::AccountId::from_str(
                "5GvHnpY1433RytXW66r77iL4CyewAAErDU6fAouoaPKvcvLU",
            )
            .unwrap(),
            label: "heyyy".to_string(),
            start_block: 30,
            end_block: 55,
            storage_price_per_block: 1,
            provider_collateral: 1,
            state: storagext::DealState::Published,
        };

        assert_eq!(result_deal_proposal, expect_deal_proposal);
    }
}
