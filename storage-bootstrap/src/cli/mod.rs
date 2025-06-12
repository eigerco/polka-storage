use crate::cli::{bootstrap::Bootstrap, query::Query};

pub mod bootstrap;
pub mod query;

#[derive(Debug, Clone, clap::Parser)]
pub enum App {
    #[command()]
    Run(Bootstrap),

    #[command()]
    Query(Query),
}
