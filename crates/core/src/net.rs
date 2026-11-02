//! Network helpers enforcing [`NetPolicy`] (SSRF protection).
//!
//! * Every DNS resolution is filtered: when private addresses are not
//!   allowed, hosts resolving to loopback/private/link-local/CGNAT ranges are
//!   rejected (this also covers DNS-rebinding, since the filtered addresses
//!   are the ones actually connected to).
//! * IP-literal URLs are checked before the request and on every redirect.
//! * Redirects are limited.

use crate::error::{ToolError, ToolResult};
use crate::policy::NetPolicy;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

struct GuardedResolver {
    policy: Arc<NetPolicy>,
}

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let policy = self.policy.clone();
        let host = name.as_str().to_string();
        Box::pin(async move {
            policy.check_host(&host).map_err(|e| {
                Box::new(std::io::Error::other(e.to_string()))
                    as Box<dyn std::error::Error + Send + Sync>
            })?;
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let allowed: Vec<SocketAddr> = addrs
                .into_iter()
                .filter(|a| policy.ip_allowed(a.ip()))
                .collect();
            if allowed.is_empty() {
                return Err(Box::new(std::io::Error::other(format!(
                    "'{host}' resolves only to non-public addresses; blocked by network policy"
                )))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            Ok(Box::new(allowed.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Build an HTTP client honoring the policy.
pub fn guarded_client(policy: NetPolicy) -> ToolResult<reqwest::Client> {
    let policy = Arc::new(policy);
    let redirect_policy = policy.clone();
    reqwest::Client::builder()
        .user_agent(concat!(
            "Dogubako/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/mischa-dev/dogubako)"
        ))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .dns_resolver(Arc::new(GuardedResolver {
            policy: policy.clone(),
        }))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 5 {
                return attempt.error("too many redirects");
            }
            let url = attempt.url().clone();
            match url.host_str() {
                Some(h) if redirect_policy.check_host(h).is_ok() => {
                    if !matches!(url.scheme(), "http" | "https") {
                        return attempt.error("redirect to non-HTTP scheme");
                    }
                    attempt.follow()
                }
                _ => attempt.error("redirect target blocked by network policy"),
            }
        }))
        .build()
        .map_err(|e| ToolError::failed(format!("http client: {e}")))
}

/// Validate a user-supplied URL before fetching it.
pub fn check_url(policy: &NetPolicy, raw: &str) -> ToolResult<url::Url> {
    let url = url::Url::parse(raw.trim())
        .map_err(|e| ToolError::invalid("url", format!("invalid URL: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ToolError::invalid(
            "url",
            "only http and https URLs are supported",
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| ToolError::invalid("url", "URL has no host"))?;
    policy.check_host(host)?;
    Ok(url)
}

/// Resolve `host:port` applying the policy; for raw TCP/TLS tools.
pub async fn resolve_checked(
    policy: &NetPolicy,
    host: &str,
    port: u16,
) -> ToolResult<Vec<SocketAddr>> {
    policy.check_host(host)?;
    let h = host.trim_matches(['[', ']']);
    let addrs: Vec<SocketAddr> = if let Ok(ip) = h.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        tokio::time::timeout(Duration::from_secs(10), tokio::net::lookup_host((h, port)))
            .await
            .map_err(|_| ToolError::failed(format!("DNS lookup for '{host}' timed out")))?
            .map_err(|e| ToolError::failed(format!("cannot resolve '{host}': {e}")))?
            .collect()
    };
    let allowed: Vec<SocketAddr> = addrs
        .into_iter()
        .filter(|a| policy.ip_allowed(a.ip()))
        .collect();
    if allowed.is_empty() {
        return Err(ToolError::denied(format!(
            "'{host}' resolves only to non-public addresses; blocked by network policy"
        )));
    }
    Ok(allowed)
}
