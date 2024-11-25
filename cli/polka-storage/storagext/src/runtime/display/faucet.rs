use crate::runtime::faucet::Event;

impl std::fmt::Display for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Event::Dripped { who, when } => f.write_fmt(format_args!(
                "Faucet Dripped: {{ account: {who}, block: {when} }}"
            )),
        }
    }
}

impl From<crate::runtime::faucet::events::Dripped> for Event {
    fn from(value: crate::runtime::faucet::events::Dripped) -> Self {
        Self::Dripped {
            who: value.who,
            when: value.when,
        }
    }
}
