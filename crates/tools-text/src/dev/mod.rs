//! `dev.*` tools: formatters, encoders, hashes, generators and inspectors.

use dogubako_core::prelude::*;

mod crypto;
mod encoders;
pub(crate) mod formatters;
mod misc;
pub(crate) mod query;
mod regex_tool;
pub(crate) mod time;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    let mut v = vec![];
    v.extend(formatters::tools());
    v.extend(encoders::tools());
    v.extend(crypto::tools());
    v.extend(time::tools());
    v.extend(regex_tool::tools());
    v.extend(query::tools());
    v.extend(misc::tools());
    v
}
