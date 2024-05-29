#![deny(unused_crate_dependencies)]

mod error;
mod result;

pub mod logger;

pub use error::Error;
pub use result::Result;
