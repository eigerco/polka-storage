//! Configuration for the the sealing pipeline.
//!
//! `clap` and `serde` compatible.
//!
//! Contains custom parsers for both `clap` and `serde`, the custom parsers ensure a better UX
//! for both error reporting as well as providing friendlier formats. The duration format is very
//! simple on purpose — for example, if the user needs second-level granularity, they should specify
//! the full value in seconds (90s) instead of some mix of values (1m30s); this makes it easy to
//! maintain for us and straightforward for the user to use.

use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};

/// Default amount of time to wait to seal an unfilled sector.
const fn default_wait_deals_delay() -> Duration {
    Duration::from_secs(6 * 60 * 60)
}

/// Returns the default fill percentage.
const fn default_fill_threshold() -> u8 {
    95
}

/// Default amount of time before the expiration of the sectors earliest deal.
const fn default_pre_commit_submission_slack() -> Duration {
    Duration::from_secs(60 * 60)
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

fn fill_threshold_deserializer<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    u8::deserialize(deserializer)
        .and_then(|percentage| validate_percentage(percentage).map_err(serde::de::Error::custom))
}

/// Parses a string in with the format `<number><prefix>`, where `<prefix>` is either
/// `s`, `m` or `h`, for seconds, minutes and hours, respectively.
fn duration_value_parser(src: &str) -> Result<Duration, String> {
    /// Returns the error string for the parsing function.
    // Scoped inside this function since it doesn't make sense outside of it.
    #[inline(always)]
    fn fmt_error(src: &str) -> String {
        format!(
            concat!(
                "Invalid duration value: {}.",
                "Only the \"<number><prefix>\" format is supported,",
                "valid prefixes are \"s\", \"m\" and \"h\".",
            ),
            src
        )
    }
    // In the case of clap, this trim is kinda useless unless the user is trying to mess with us;
    // in the case of serde, this trim is defensive as `str::parse<u64>` does not handle spaces.
    let src = src.trim();
    let split_index = src.len() - 1;
    // Try to parse the duration upfront
    let duration = src[..split_index]
        .parse::<u64>()
        .map_err(|_| fmt_error(src))?;

    // Look for the last character, match against the known prefixes, act accordingly.
    Ok(match &src[split_index..] {
        "s" => Duration::from_secs(duration),
        "m" => Duration::from_secs(duration * 60),
        "h" => Duration::from_secs(duration * 60 * 60),
        _ => return Err(fmt_error(src)),
    })
}

fn duration_deserializer<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    String::deserialize(deserializer)
        .and_then(|duration| duration_value_parser(&duration).map_err(serde::de::Error::custom))
}

/// Configuration for the sealing process.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SealingConfiguration {
    /// The percentage above which a sector is considered "full", defaults to 95% of the registered
    /// sector size.
    ///
    /// Lower values will inevitably waste space, however, for smaller sector values, this enables
    /// sectors not to wait long for "the perfect piece"; alternatively, keep this value at 100%
    /// while lowering [`wait_deals_delay`](`Self::wait_deals_delay`).
    #[serde(
        default = "default_fill_threshold",
        // The custom serializer enables custom validations
        deserialize_with = "fill_threshold_deserializer"
    )]
    pub fill_threshold: u8,

    /// The amount of time to wait before sealing an unfilled sector, defaults to 6 hours.
    #[serde(
        default = "default_wait_deals_delay",
        deserialize_with = "duration_deserializer"
    )]
    pub wait_deals_delay: Duration,

    /// The amount of time before a sector's earliest deal start; once hit, the sector is sealed &
    /// pre-committed.
    #[serde(
        default = "default_pre_commit_submission_slack",
        deserialize_with = "duration_deserializer"
    )]
    pub pre_commit_submission_slack: Duration,
}

// This default is implemented for when `sealing_configuration` is missing from the config file,
// this is necessary since `serde` behaves differently in the face of `{ sealing: {} }` and `{}`,
// the former is able to use the configuration defaults defined inside the structure while the
// latter is only able to use the `Default` implementation.
impl Default for SealingConfiguration {
    fn default() -> Self {
        Self {
            fill_threshold: default_fill_threshold(),
            wait_deals_delay: default_wait_deals_delay(),
            pre_commit_submission_slack: default_pre_commit_submission_slack(),
        }
    }
}
