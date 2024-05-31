use rocksdb::{AsColumnFamilyRef, WriteBatchWithTransaction};
use serde::Serialize;

use super::PieceStoreError;

pub(crate) trait WriteBatchWithTransactionExt {
    /// Insert a CBOR serialized value with the provided key.
    fn put_cf_cbor<K, V>(
        &mut self,
        cf: &impl AsColumnFamilyRef,
        key: K,
        value: V,
    ) -> Result<(), PieceStoreError>
    where
        K: AsRef<[u8]>,
        V: Serialize;
}

impl<const TRANSACTION: bool> WriteBatchWithTransactionExt
    for WriteBatchWithTransaction<TRANSACTION>
{
    fn put_cf_cbor<K, V>(
        &mut self,
        cf: &impl AsColumnFamilyRef,
        key: K,
        value: V,
    ) -> Result<(), PieceStoreError>
    where
        K: AsRef<[u8]>,
        V: Serialize,
    {
        let mut serialized = vec![];
        match ciborium::into_writer(&value, &mut serialized) {
            Ok(_) => Ok(self.put_cf(cf, key, serialized)),
            Err(err) => Err(PieceStoreError::Serialization(err.to_string())),
        }
    }
}
