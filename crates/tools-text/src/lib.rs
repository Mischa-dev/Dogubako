//! Text, developer and data utilities.

use dogubako_core::prelude::*;

mod text;

/// All tools provided by this crate.
pub fn tools() -> Vec<Arc<dyn Tool>> {
    let mut v = Vec::new();
    v.extend(text::tools());
    v
}
