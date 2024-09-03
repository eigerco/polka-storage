use serde::ser::SerializeStruct;
impl serde::Serialize for pallet::SettledDealData<Runtime> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_struct("SettledDealData", 4)?;
        s.serialize_field("deal_id", &self.deal_id)?;
        s.serialize_field("client", &self.client)?;
        s.serialize_field("provider", &self.provider)?;
        s.serialize_field("amount", &self.amount)?;
        s.end()
    }
}
