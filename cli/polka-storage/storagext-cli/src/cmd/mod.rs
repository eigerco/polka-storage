pub mod market;
pub mod storage_provider;
pub mod system;

use storagext::runtime::{HashOfPsc, SubmissionResult};

use crate::OutputFormat;

pub(crate) fn display_submission_result<Event>(
    opt_result: Option<SubmissionResult<HashOfPsc, Event>>,
    _output_format: OutputFormat,
) -> Result<(), anyhow::Error>
where
    Event: subxt::events::StaticEvent,
{
    if let Some(result) = opt_result {
        // TODO(@neutrinoks,24.10.24): Check if we can return as root event instead to enable this
        // display possibility again.
        // let output = output_format.format(&result.event)?;
        // match output_format {
        //     OutputFormat::Plain => println!("[{}] {}", result.hash, output),
        //     OutputFormat::Json => println!("{}", output),
        // }
        println!(
            "[{}] {}::{}",
            result.hash[0],
            <Event as subxt::events::StaticEvent>::PALLET,
            <Event as subxt::events::StaticEvent>::EVENT
        );
    }

    Ok(())
}
