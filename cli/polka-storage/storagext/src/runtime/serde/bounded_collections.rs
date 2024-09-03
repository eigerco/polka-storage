use serde::ser::SerializeSeq;

use crate::runtime::runtime_types::bounded_collections::{
    bounded_btree_set::BoundedBTreeSet, bounded_vec::BoundedVec,
};

impl<T> serde::Serialize for BoundedVec<T>
where
    T: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for elem in self.0.iter() {
            seq.serialize_element(elem)?;
        }
        seq.end()
    }
}

impl<T> serde::Serialize for BoundedBTreeSet<T>
where
    T: serde::Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for elem in self.0.iter() {
            seq.serialize_element(elem)?;
        }
        seq.end()
    }
}
