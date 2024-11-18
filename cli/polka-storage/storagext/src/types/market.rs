use cid::Cid;
use codec::Encode;
use sha2::Digest;
use subxt::{ext::sp_runtime::MultiSignature, tx::Signer, utils::Static};

use crate::{
    runtime::{
        bounded_vec::IntoBoundedByteVec,
        runtime_types::pallet_market::pallet::{
            ClientDealProposal as RuntimeClientDealProposal, DealProposal as RuntimeDealProposal,
            DealState as RuntimeDealState,
        },
    },
    BlockNumber, Currency, PolkaStorageConfig,
};

// Reference: <https://github.com/multiformats/multicodec/blob/master/table.csv>
const SHA2_256_MULTICODEC_CODE: u64 = 0x12;
const JSON_MULTICODEC_CODE: u64 = 0x0200;

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error(transparent)]
    Cid(#[from] cid::Error),

    #[error(transparent)]
    Utf8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    Multihash(#[from] cid::multihash::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Doppelganger of `RuntimeDealProposal` but with more ergonomic types and no generics.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct DealProposal {
    #[serde(deserialize_with = "crate::types::deserialize_string_to_cid")]
    #[serde(serialize_with = "crate::types::serialize_cid_to_string")]
    pub piece_cid: Cid,
    pub piece_size: u64,
    pub client: <PolkaStorageConfig as subxt::Config>::AccountId,
    pub provider: <PolkaStorageConfig as subxt::Config>::AccountId,
    pub label: String,
    pub start_block: BlockNumber,
    pub end_block: BlockNumber,
    pub storage_price_per_block: Currency,
    pub provider_collateral: Currency,
    pub state: RuntimeDealState<BlockNumber>,
}

impl From<DealProposal>
    for RuntimeDealProposal<subxt::ext::subxt_core::utils::AccountId32, Currency, BlockNumber>
{
    fn from(value: DealProposal) -> Self {
        Self {
            piece_cid: value.piece_cid.into_bounded_byte_vec(),
            piece_size: value.piece_size,
            client: value.client.into(),
            provider: value.provider.into(),
            label: value.label.into_bounded_byte_vec(),
            start_block: value.start_block,
            end_block: value.end_block,
            storage_price_per_block: value.storage_price_per_block,
            provider_collateral: value.provider_collateral,
            state: value.state,
        }
    }
}

impl TryFrom<RuntimeDealProposal<subxt::ext::subxt_core::utils::AccountId32, Currency, BlockNumber>>
    for DealProposal
{
    type Error = ConversionError;

    fn try_from(
        value: RuntimeDealProposal<
            subxt::ext::subxt_core::utils::AccountId32,
            Currency,
            BlockNumber,
        >,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            piece_cid: Cid::read_bytes(value.piece_cid.0.as_slice())?,
            piece_size: value.piece_size,
            client: <PolkaStorageConfig as subxt::Config>::AccountId::new(value.client.0),
            provider: <PolkaStorageConfig as subxt::Config>::AccountId::new(value.provider.0),
            label: String::from_utf8(value.label.0)?,
            start_block: value.start_block,
            end_block: value.end_block,
            storage_price_per_block: value.storage_price_per_block,
            provider_collateral: value.provider_collateral,
            state: value.state,
        })
    }
}

impl DealProposal {
    /// Consumes the [`DealProposal`], signs it using the provided keypair
    /// and returns a deal proposal ready to be submitted.
    pub(crate) fn sign<Keypair>(
        self,
        keypair: &Keypair,
    ) -> RuntimeClientDealProposal<
        subxt::ext::subxt_core::utils::AccountId32,
        Currency,
        BlockNumber,
        Static<MultiSignature>,
    >
    where
        Keypair: Signer<PolkaStorageConfig>,
    {
        let proposal: RuntimeDealProposal<_, _, _> = self.into();
        let encoded = &proposal.encode();
        tracing::trace!("deal_proposal: encoded proposal: {}", hex::encode(&encoded));
        let client_signature = Static(keypair.sign(encoded));

        RuntimeClientDealProposal {
            proposal,
            client_signature,
        }
    }

    pub fn sign_serializable<Keypair>(self, keypair: &Keypair) -> ClientDealProposal
    where
        Keypair: Signer<PolkaStorageConfig>,
    {
        // I know this performs a big ass roundtrip but with `into` consuming `self`,
        // I'd need to encode the values individually to squeeze more performance
        // for now, this will do more than ok
        let proposal: RuntimeDealProposal<_, _, _> = self.into();
        let encoded = &proposal.encode();

        tracing::trace!("deal_proposal: encoded proposal: {}", hex::encode(&encoded));
        let client_signature = keypair.sign(encoded);

        ClientDealProposal {
            deal_proposal: proposal
                .try_into()
                .expect("`self` should have been previously validated"),
            client_signature,
        }
    }

    /// Get the CID of this deal proposal, as serialized into JSON.
    pub fn json_cid(&self) -> Result<cid::Cid, ConversionError> {
        let deal_proposal_json = serde_json::to_string(self)?;
        let deal_proposal_sha256 = sha2::Sha256::digest(&deal_proposal_json);
        let deal_proposal_multihash =
            cid::multihash::Multihash::wrap(SHA2_256_MULTICODEC_CODE, &deal_proposal_sha256)?;
        Ok(Cid::new_v1(JSON_MULTICODEC_CODE, deal_proposal_multihash))
    }
}

/// A client-signed [`DealProposal`].
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ClientDealProposal {
    /// The deal proposal contents.
    pub deal_proposal: DealProposal,

    /// The signature of the [`DealProposal`].
    #[serde(alias = "signature")]
    pub client_signature: subxt::ext::sp_runtime::MultiSignature,
}

impl From<ClientDealProposal>
    for RuntimeClientDealProposal<
        subxt::ext::subxt_core::utils::AccountId32,
        Currency,
        BlockNumber,
        Static<subxt::ext::sp_runtime::MultiSignature>,
    >
{
    fn from(value: ClientDealProposal) -> Self {
        Self {
            proposal: value.deal_proposal.into(),
            client_signature: Static(value.client_signature),
        }
    }
}
