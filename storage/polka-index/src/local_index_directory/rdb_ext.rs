use rocksdb::{AsColumnFamilyRef, WriteBatchWithTransaction};
use serde::Serialize;

use super::LidError;

pub(crate) trait WriteBatchWithTransactionExt {
    /// Insert a CBOR serialized value with the provided key.
    fn put_cf_cbor<K, V>(
        &mut self,
        cf: &impl AsColumnFamilyRef,
        key: K,
        value: V,
    ) -> Result<(), LidError>
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
    ) -> Result<(), LidError>
    where
        K: AsRef<[u8]>,
        V: Serialize,
    {
        let mut serialized = vec![];
        if let Err(err) = ciborium::into_writer(&value, &mut serialized) {
            return Err(LidError::Serialization(err.to_string()));
        }
        Ok(self.put_cf(cf, key, serialized))
    }
}
