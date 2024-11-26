use crate::runtime::proofs::Event;

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::PoRepVerifyingKeyChanged { .. } => {
                f.write_fmt(format_args!("PoRep verifying key changed"))
            }
            Event::PoStVerifyingKeyChanged { .. } => {
                f.write_fmt(format_args!("PoSt verifying key changed"))
            }
        }
    }
}
