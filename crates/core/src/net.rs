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
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

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
    build_guarded(policy, false)
}

/// Like [`guarded_client`], but redirects must stay on the original origin,
/// so request headers (credentials) never reach another host.
pub fn guarded_client_same_origin(policy: NetPolicy) -> ToolResult<reqwest::Client> {
    build_guarded(policy, true)
}

fn build_guarded(policy: NetPolicy, same_origin: bool) -> ToolResult<reqwest::Client> {
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
            if same_origin
                && attempt
                    .previous()
                    .first()
                    .is_some_and(|first| first.origin() != url.origin())
            {
                return attempt
                    .error("cross-origin redirect refused: the request sends manifest headers");
            }
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

/// A local HTTP proxy for external programs (`--proxy`): every CONNECT tunnel
/// and plain-HTTP request, redirects and media downloads included, is resolved
/// with [`resolve_checked`] and connected only to the addresses it allows.
/// Stops when dropped.
pub struct PolicyProxy {
    addr: SocketAddr,
    task: tokio::task::JoinHandle<()>,
}

impl PolicyProxy {
    pub async fn start(policy: NetPolicy) -> ToolResult<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(|e| ToolError::failed(format!("network proxy: {e}")))?;
        let addr = listener
            .local_addr()
            .map_err(|e| ToolError::failed(format!("network proxy: {e}")))?;
        let policy = Arc::new(policy);
        let task = tokio::spawn(async move {
            let mut conns = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    r = listener.accept() => match r {
                        Ok((s, _)) => {
                            let p = policy.clone();
                            conns.spawn(async move {
                                let _ = proxy_conn(s, &p).await;
                            });
                        }
                        Err(_) => tokio::time::sleep(Duration::from_millis(50)).await,
                    },
                    Some(_) = conns.join_next(), if !conns.is_empty() => {}
                }
            }
        });
        Ok(PolicyProxy { addr, task })
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}

impl Drop for PolicyProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn proxy_reply(c: &mut TcpStream, status: &str, body: &str) -> std::io::Result<()> {
    let msg = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    c.write_all(msg.as_bytes()).await?;
    c.shutdown().await
}

async fn proxy_conn(mut client: TcpStream, policy: &NetPolicy) -> std::io::Result<()> {
    let mut buf = Vec::with_capacity(4096);
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > 64 * 1024 {
            return proxy_reply(&mut client, "431 Request Header Fields Too Large", "").await;
        }
        let mut chunk = [0u8; 4096];
        let n = tokio::time::timeout(Duration::from_secs(30), client.read(&mut chunk))
            .await
            .map_err(|_| std::io::Error::other("proxy request timed out"))??;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let rest = &buf[head_end..];
    let mut lines = head.split("\r\n");
    let mut parts = lines.next().unwrap_or_default().split_whitespace();
    let (Some(method), Some(target), Some(version)) = (parts.next(), parts.next(), parts.next())
    else {
        return proxy_reply(&mut client, "400 Bad Request", "malformed request").await;
    };
    let (host, port, forward) = if method.eq_ignore_ascii_case("CONNECT") {
        match target
            .rsplit_once(':')
            .and_then(|(h, p)| Some((h, p.parse::<u16>().ok()?)))
        {
            Some((h, p)) => (h.trim_matches(['[', ']']).to_string(), p, None),
            None => return proxy_reply(&mut client, "400 Bad Request", "bad CONNECT target").await,
        }
    } else {
        let url = match url::Url::parse(target) {
            Ok(u) if u.scheme() == "http" && u.host_str().is_some() => u,
            _ => {
                return proxy_reply(
                    &mut client,
                    "400 Bad Request",
                    "expected an absolute http:// URL",
                )
                .await
            }
        };
        let mut path = url.path().to_string();
        if let Some(q) = url.query() {
            path.push('?');
            path.push_str(q);
        }
        // One request per upstream connection, so every host gets checked.
        let mut out = format!("{method} {path} {version}\r\n");
        for l in lines.filter(|l| !l.is_empty()) {
            let name = l
                .split(':')
                .next()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase();
            if !matches!(
                name.as_str(),
                "connection" | "proxy-connection" | "proxy-authorization" | "keep-alive"
            ) {
                out.push_str(l);
                out.push_str("\r\n");
            }
        }
        out.push_str("Connection: close\r\n\r\n");
        let host = url
            .host_str()
            .unwrap_or_default()
            .trim_matches(['[', ']'])
            .to_string();
        (host, url.port_or_known_default().unwrap_or(80), Some(out))
    };
    let addrs = match resolve_checked(policy, &host, port).await {
        Ok(a) => a,
        Err(e @ ToolError::PermissionDenied { .. }) => {
            return proxy_reply(&mut client, "403 Blocked by network policy", &e.to_string()).await
        }
        Err(e) => return proxy_reply(&mut client, "502 Bad Gateway", &e.to_string()).await,
    };
    let mut upstream = None;
    let mut last_err = String::new();
    for a in addrs {
        match tokio::time::timeout(Duration::from_secs(10), TcpStream::connect(a)).await {
            Ok(Ok(s)) => {
                upstream = Some(s);
                break;
            }
            Ok(Err(e)) => last_err = e.to_string(),
            Err(_) => last_err = "timed out".into(),
        }
    }
    let Some(mut upstream) = upstream else {
        let msg = format!("cannot connect to {host}:{port}: {last_err}");
        return proxy_reply(&mut client, "502 Bad Gateway", &msg).await;
    };
    match forward {
        None => {
            client
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await?
        }
        Some(h) => upstream.write_all(h.as_bytes()).await?,
    }
    upstream.write_all(rest).await?;
    tokio::io::copy_bidirectional(&mut client, &mut upstream).await?;
    Ok(())
}
