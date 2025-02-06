//! Configuration for the the sealing pipeline.
//!
//! `clap` and `serde` compatible.
//!
//! Contains custom parsers for both `clap` and `serde`, the custom parsers ensure a better UX
//! for both error reporting as well as providing friendlier formats. The duration format is very
//! simple on purpose — for example, if the user needs second-level granularity, they should specify
//! the full value in seconds (90s) instead of some mix of values (1m30s); this makes it easy to
//! maintain for us and straightforward for the user to use.

use serde::{Deserialize, Deserializer, Serialize};

/// Returns the default fill percentage.
const fn default_fill_percentage() -> u8 {
    95
}

fn validate_percentage(percentage: u8) -> Result<u8, String> {
    if percentage > 100 {
        return Err(format!(
            "percentage value must be between 0 and 100, got {} instead",
            percentage
        ));
    }
    Ok(percentage)
}

fn fill_percentage_deserializer<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    u8::deserialize(deserializer)
        .and_then(|percentage| validate_percentage(percentage).map_err(serde::de::Error::custom))
}

/// Configuration for the sealing process.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[cfg_attr(feature = "clap", derive(::clap::Args))]
pub struct SealingConfiguration {
    /// The percentage above which a sector is considered "full", defaults to 95% of the registered
    /// sector size.
    ///
    /// Lower values will inevitably waste space, however, for smaller sector values, this enables
    /// sectors not to wait long for "the perfect piece"; alternatively, keep this value at 100%
    /// while lowering [`wait_deals_delay`](`Self::wait_deals_delay`).
    #[serde(
        default = "default_fill_percentage",
        // The custom serializer enables custom validations
        deserialize_with = "fill_percentage_deserializer"
    )]
    #[cfg_attr(feature = "clap", arg(
        long,
        default_value_t = default_fill_percentage(),
        value_parser = fill_percentage_parser
    ))]
    pub fill_percentage: u8,
}

// This default is implemented for when `sealing_configuration` is missing from the config file,
// this is necessary since `serde` behaves differently in the face of `{ sealing: {} }` and `{}`,
// the former is able to use the configuration defaults defined inside the structure while the
// latter is only able to use the `Default` implementation.
impl Default for SealingConfiguration {
    fn default() -> Self {
        Self {
            fill_percentage: default_fill_percentage(),
        }
    }
}

/// Parses an integer between 0 and 100, values outside the range are considered invalid
/// and return an error.
#[cfg(feature = "clap")]
fn fill_percentage_parser(src: &str) -> Result<u8, String> {
    src.trim()
        .parse::<u8>()
        .map_err(|err| err.to_string())
        .and_then(validate_percentage)
}
