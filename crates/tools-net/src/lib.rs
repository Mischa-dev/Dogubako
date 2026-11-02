//! Network & web tools: DNS, RDAP/WHOIS, HTTP, TLS, certificates, keys,
//! URLs and IP math.
//!
//! Every tool that touches the network declares `Permission::Network` and
//! goes through the policy-enforcing helpers in `dogubako_core::net` (or the
//! equivalent guarded client in [`util`]), so hosted deployments with
//! `NetPolicy::public_only()` cannot be used to reach private addresses.

use dogubako_core::prelude::*;

mod certs;
mod dns;
mod domain;
mod http;
mod ip;
mod keys;
mod registry;
mod tls;
mod urls;
pub mod util;

/// All tools provided by this crate.
pub fn tools() -> Vec<Arc<dyn Tool>> {
    let mut v = vec![];
    v.extend(urls::tools());
    v.extend(ip::tools());
    v.extend(certs::tools());
    v.extend(keys::tools());
    v.extend(dns::tools());
    v.extend(tls::tools());
    v.extend(http::tools());
    v.extend(registry::tools());
    v.extend(domain::tools());
    v
}
