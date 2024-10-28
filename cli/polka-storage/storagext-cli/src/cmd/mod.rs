pub mod market;
pub mod storage_provider;
pub mod system;

use storagext::runtime::{HashOfPsc, SubmissionResult};

use crate::OutputFormat;

pub(crate) fn display_submission_result<Event, Variant>(
    opt_result: Option<SubmissionResult<HashOfPsc, Event, Variant>>,
    output_format: OutputFormat,
) -> Result<(), anyhow::Error>
where
    Event: scale_decode::DecodeAsType + std::fmt::Display + serde::Serialize,
{
    if let Some(Ok(events)) = opt_result {
        let output = output_format.format(&events[0].event)?;
        match output_format {
            OutputFormat::Plain => println!("[{}] {}", events[0].hash, output),
            OutputFormat::Json => println!("{}", output),
        }
    }

    Ok(())
}
