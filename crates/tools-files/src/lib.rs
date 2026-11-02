//! File utilities: checksums, archives, comparison, inspection and
//! host-folder tools (sizes, duplicates, search, batch rename).

use dogubako_core::prelude::*;

mod archive;
mod compare;
mod hash;
mod inspect;
mod rename;
mod scan;
mod util;

#[cfg(test)]
mod tests;

/// All tools provided by this crate.
pub fn tools() -> Vec<Arc<dyn Tool>> {
    let mut v = Vec::new();
    v.extend(hash::tools());
    v.extend(inspect::tools());
    v.extend(compare::tools());
    v.extend(archive::tools());
    v.extend(scan::tools());
    v.extend(rename::tools());
    v
}
