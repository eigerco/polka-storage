use codec::Encode;
use sp_inherents::{InherentIdentifier, IsFatalError};
use sp_runtime::RuntimeString;

/// BABE VRF Inherent Identifier
pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"babe_vrf";

#[derive(Encode)]
#[cfg_attr(feature = "std", derive(Debug, codec::Decode))]
pub enum InherentError {
    Other(RuntimeString),
}

impl IsFatalError for InherentError {
    fn is_fatal_error(&self) -> bool {
        true
    }
}

impl InherentError {
    /// Try to create an instance ouf of the given identifier and data.
    #[cfg(feature = "std")]
    pub fn try_from(id: &InherentIdentifier, data: &[u8]) -> Option<Self> {
        if id == &INHERENT_IDENTIFIER {
            <InherentError as codec::Decode>::decode(&mut &*data).ok()
        } else {
            None
        }
    }
}

#[cfg(feature = "std")]
pub struct InherentDataProvider;

#[cfg(feature = "std")]
#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for InherentDataProvider {
    async fn provide_inherent_data(
        &self,
        inherent_data: &mut sp_inherents::InherentData,
    ) -> Result<(), sp_inherents::Error> {
        inherent_data.put_data(INHERENT_IDENTIFIER, &())
    }

    async fn try_handle_error(
        &self,
        _identifier: &InherentIdentifier,
        _error: &[u8],
    ) -> Option<Result<(), sp_inherents::Error>> {
        // Most substrate inherents return None
        None
    }
}
