pub mod market;
pub mod storage_provider;

use serde::de::Deserialize;

// The CID has some issues that require a workaround for strings.
// For more details, see: <https://github.com/multiformats/rust-cid/issues/162>
fn deserialize_string_to_cid<'de, D>(deserializer: D) -> Result<cid::Cid, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let cid = cid::Cid::try_from(s.as_str()).map_err(|e| {
        serde::de::Error::custom(format!(
            "failed to parse CID, check that the input is a valid CID: {e:?}"
        ))
    })?;
    Ok(cid)
}
