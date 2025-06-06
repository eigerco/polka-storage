use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;

use crate::deals::deal_proposal::DealProposal;

/// After Storage Client has successfully negotiated with the Storage Provider, they prepare a DealProposal,
/// sign it with their signature and send to the Storage Provider.
/// Storage Provider only after successful file transfer and verification of the data, calls an extrinsic `market.publish_storage_deals`.
/// The extrinsic call is signed by the Storage Provider and Storage Client's signature is in the message.
/// Based on that, Market Pallet can verify the signature and lock appropriate funds.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ClientDealProposal<Address, Currency, BlockNumber, OffchainSignature> {
    pub proposal: DealProposal<Address, Currency, BlockNumber>,
    pub client_signature: OffchainSignature,
}
