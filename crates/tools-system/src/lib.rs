//! System information tools: OS, CPU, memory, disks, battery, hardware,
//! network interfaces, processes, sensors and environment diagnostics.
//!
//! Every value comes from the host via `sysinfo` / `starship-battery` (or
//! readable sysfs files); when something is unavailable the field is `null`
//! with an explanatory note — nothing is estimated or fabricated. When the
//! engine runs hosted (`Environment::Server`) results describe the server.

use dogubako_core::prelude::*;

mod battery;
mod info;
mod util;

/// All tools provided by this crate.
pub fn tools() -> Vec<Arc<dyn Tool>> {
    let mut v = info::tools();
    v.extend(battery::tools());
    v
}
