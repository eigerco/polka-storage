pub mod market;
pub mod storage_provider;
pub mod system;

use storagext::runtime::{HashOfPsc, SubmissionResult};

use crate::OutputFormat;

pub(crate) fn display_submission_result<Variant: std::fmt::Debug>(
    opt_result: Option<SubmissionResult<HashOfPsc, Variant>>,
    output_format: OutputFormat,
) -> Result<(), anyhow::Error> {
    if opt_result.is_none() {
        return Ok(());
    }
    let result = opt_result.expect("expect some, checked before");

    match result {
        Ok(events) => {
            events
                .iter()
                .for_each(|e| println!("[{}] {:?}", e.hash, e.variant));
            match &events[0].event {
                storagext::runtime::Event::Market(e) => {
                    display::<_>(events[0].hash, e, output_format)?
                }
                storagext::runtime::Event::StorageProvider(e) => {
                    display::<_>(events[0].hash, e, output_format)?
                }
                _ => return Ok(()),
            }
        }
        Err(_) => {
            println!("Extrinsic failed: {}!", std::any::type_name::<Variant>());
        }
    }

    Ok(())
}

fn display<E>(hash: HashOfPsc, event: E, output_format: OutputFormat) -> Result<(), anyhow::Error>
where
    E: std::fmt::Display + serde::Serialize,
{
    let output = output_format.format(&event)?;
    match output_format {
        OutputFormat::Plain => println!("[{}] {}", hash, output),
        OutputFormat::Json => println!("{}", output),
    }
    Ok(())
}
