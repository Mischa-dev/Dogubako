//! Shared helpers for the network tools.

use dogubako_core::policy::NetPolicy;
use dogubako_core::prelude::*;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const USER_AGENT: &str = concat!(
    "Dogubako/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/mischa-dev/dogubako)"
);

/// DNS resolver for reqwest that applies the network policy to every
/// resolved address (same rules as `dogubako_core::net`'s guarded client).
struct GuardedResolver {
    policy: Arc<NetPolicy>,
}

impl reqwest::dns::Resolve for GuardedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let policy = self.policy.clone();
        let host = name.as_str().to_string();
        Box::pin(async move {
            type E = Box<dyn std::error::Error + Send + Sync>;
            policy
                .check_host(&host)
                .map_err(|e| Box::new(std::io::Error::other(e.to_string())) as E)?;
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let allowed: Vec<SocketAddr> = addrs
                .into_iter()
                .filter(|a| policy.ip_allowed(a.ip()))
                .collect();
            if allowed.is_empty() {
                return Err(Box::new(std::io::Error::other(format!(
                    "'{host}' resolves only to non-public addresses; blocked by network policy"
                ))) as E);
            }
            Ok(Box::new(allowed.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// A policy-guarded HTTP client that does NOT follow redirects, so tools can
/// display and individually validate every hop. Core's `ctx.http_client()`
/// follows redirects itself and offers no way to disable that, hence this
/// local equivalent. Callers must run `net::check_url` on every hop (IP
/// literal URLs bypass the resolver).
pub fn manual_redirect_client(ctx: &ToolContext, timeout: Duration) -> ToolResult<reqwest::Client> {
    if !ctx.has_permission(Permission::Network) {
        return Err(ToolError::denied("network access was not granted"));
    }
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(8).min(timeout))
        .timeout(timeout)
        .dns_resolver(Arc::new(GuardedResolver {
            policy: Arc::new(ctx.net_policy().clone()),
        }))
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| ToolError::failed(format!("http client: {e}")))
}

/// Read a response body, failing if it exceeds `limit` bytes.
pub async fn read_capped(mut resp: reqwest::Response, limit: usize) -> ToolResult<Vec<u8>> {
    if let Some(len) = resp.content_length() {
        if len as usize > limit {
            return Err(ToolError::limit(format!(
                "response is {} (limit {})",
                human_size(len),
                human_size(limit as u64)
            )));
        }
    }
    let mut buf = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| ToolError::failed(format!("reading response: {e}")))?
    {
        if buf.len() + chunk.len() > limit {
            return Err(ToolError::limit(format!(
                "response exceeds {}",
                human_size(limit as u64)
            )));
        }
        buf.extend_from_slice(&chunk);
    }
    Ok(buf)
}

/// Fetch a URL with the guarded client (follows policy-checked redirects),
/// returning status, final URL, content type and capped body.
pub async fn fetch(
    ctx: &ToolContext,
    url: &str,
    accept: &str,
    limit: usize,
    timeout: Duration,
) -> ToolResult<Fetched> {
    let u = dogubako_core::net::check_url(ctx.net_policy(), url)?;
    let client = ctx.http_client()?;
    let resp = client
        .get(u)
        .header(reqwest::header::ACCEPT, accept)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| ToolError::failed(format!("request to {url} failed: {}", err_chain(&e))))?;
    let status = resp.status().as_u16();
    let final_url = resp.url().to_string();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = read_capped(resp, limit).await?;
    Ok(Fetched {
        status,
        final_url,
        content_type,
        body,
    })
}

pub struct Fetched {
    pub status: u16,
    pub final_url: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

/// Full error chain as a single line (reqwest hides the useful cause).
pub fn err_chain(e: &dyn std::error::Error) -> String {
    let mut s = e.to_string();
    let mut cur = e.source();
    while let Some(c) = cur {
        let cs = c.to_string();
        if !s.contains(&cs) {
            s.push_str(": ");
            s.push_str(&cs);
        }
        cur = c.source();
    }
    s
}

/// Connect to the first reachable address with a timeout.
pub async fn connect_any(
    addrs: &[SocketAddr],
    timeout: Duration,
) -> ToolResult<(tokio::net::TcpStream, SocketAddr, Duration)> {
    let mut last = String::from("no addresses");
    for a in addrs {
        let start = Instant::now();
        match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(a)).await {
            Ok(Ok(s)) => return Ok((s, *a, start.elapsed())),
            Ok(Err(e)) => last = format!("{a}: {e}"),
            Err(_) => last = format!("{a}: connection timed out after {} ms", timeout.as_millis()),
        }
    }
    Err(ToolError::failed(format!("could not connect: {last}")))
}

/// Split `host[:port]`, `[v6]:port`, or a URL into host and port.
pub fn split_host_port(input: &str, default_port: u16) -> ToolResult<(String, u16)> {
    let s = input.trim();
    if s.is_empty() {
        return Err(ToolError::invalid("host", "is required"));
    }
    if s.contains("://") {
        let u = url::Url::parse(s)
            .map_err(|e| ToolError::invalid("host", format!("invalid URL: {e}")))?;
        let host = u
            .host_str()
            .ok_or_else(|| ToolError::invalid("host", "URL has no host"))?
            .trim_matches(['[', ']'])
            .to_string();
        let port = u.port_or_known_default().unwrap_or(default_port);
        return Ok((host, port));
    }
    if let Some(rest) = s.strip_prefix('[') {
        let (h, after) = rest
            .split_once(']')
            .ok_or_else(|| ToolError::invalid("host", "unterminated IPv6 bracket"))?;
        let port = match after.strip_prefix(':') {
            Some(p) => p
                .parse()
                .map_err(|_| ToolError::invalid("host", format!("invalid port '{p}'")))?,
            None => default_port,
        };
        return Ok((h.to_string(), port));
    }
    if s.parse::<std::net::Ipv6Addr>().is_ok() {
        return Ok((s.to_string(), default_port));
    }
    match s.rsplit_once(':') {
        Some((h, p)) => {
            let port = p
                .parse()
                .map_err(|_| ToolError::invalid("host", format!("invalid port '{p}'")))?;
            Ok((h.to_string(), port))
        }
        None => Ok((s.to_string(), default_port)),
    }
}

/// Validate a hostname-ish string (no spaces, no path).
pub fn clean_host(param: &str, s: &str) -> ToolResult<String> {
    let h = s.trim().trim_end_matches('.');
    if h.is_empty() {
        return Err(ToolError::invalid(param, "is required"));
    }
    if h.len() > 253 || h.chars().any(|c| c.is_whitespace() || "/\\?#@".contains(c)) {
        return Err(ToolError::invalid(
            param,
            format!("'{h}' is not a valid host name"),
        ));
    }
    Ok(h.to_string())
}

pub fn ms(d: Duration) -> f64 {
    (d.as_secs_f64() * 1000.0 * 10.0).round() / 10.0
}

/// Colon-separated upper-case hex (fingerprint style).
pub fn colon_hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

pub fn sha256(b: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    sha2::Sha256::digest(b).to_vec()
}

pub fn sha1(b: &[u8]) -> Vec<u8> {
    use sha1::Digest;
    sha1::Sha1::digest(b).to_vec()
}

pub fn b64(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(b)
}

pub fn b64_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let clean: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(clean.as_bytes())
        .ok()
}

/// Build a `data.table` value.
pub fn table(columns: &[&str], rows: Vec<Vec<Value>>) -> Value {
    json!({ "columns": columns, "rows": rows })
}

/// One PEM block.
pub struct PemBlock {
    pub label: String,
    pub headers: Vec<(String, String)>,
    pub der: Vec<u8>,
}

/// Lenient PEM parser: returns all well-formed blocks.
pub fn parse_pem(text: &str) -> Vec<PemBlock> {
    let mut out = vec![];
    let mut lines = text.lines().map(|l| l.trim());
    while let Some(line) = lines.next() {
        let Some(label) = line
            .strip_prefix("-----BEGIN ")
            .and_then(|l| l.strip_suffix("-----"))
        else {
            continue;
        };
        let label = label.to_string();
        let end = format!("-----END {label}-----");
        let mut headers = vec![];
        let mut body = String::new();
        let mut closed = false;
        for l in lines.by_ref() {
            if l == end {
                closed = true;
                break;
            }
            if body.is_empty() && l.contains(':') {
                if let Some((k, v)) = l.split_once(':') {
                    headers.push((k.trim().to_string(), v.trim().to_string()));
                    continue;
                }
            }
            body.push_str(l);
        }
        if !closed {
            break;
        }
        if let Some(der) = b64_decode(&body) {
            out.push(PemBlock {
                label,
                headers,
                der,
            });
        }
    }
    out
}

pub fn pem_encode(label: &str, der: &[u8]) -> String {
    let b = b64(der);
    let mut s = format!("-----BEGIN {label}-----\n");
    for chunk in b.as_bytes().chunks(64) {
        s.push_str(std::str::from_utf8(chunk).unwrap_or(""));
        s.push('\n');
    }
    s.push_str(&format!("-----END {label}-----\n"));
    s
}

// ---------------------------------------------------------------------------
// Minimal DER reader/writer (for key structures x509-parser doesn't cover).

pub struct Tlv<'a> {
    pub tag: u8,
    pub content: &'a [u8],
}

/// Parse one TLV; returns it and the remaining input.
pub fn der_read(input: &[u8]) -> Option<(Tlv<'_>, &[u8])> {
    let (&tag, rest) = input.split_first()?;
    if tag & 0x1f == 0x1f {
        return None; // high tag numbers not needed here
    }
    let (&l0, rest) = rest.split_first()?;
    let (len, rest) = if l0 < 0x80 {
        (l0 as usize, rest)
    } else {
        let n = (l0 & 0x7f) as usize;
        if n == 0 || n > 4 || rest.len() < n {
            return None;
        }
        let mut len = 0usize;
        for b in &rest[..n] {
            len = (len << 8) | *b as usize;
        }
        (len, &rest[n..])
    };
    if rest.len() < len {
        return None;
    }
    Some((
        Tlv {
            tag,
            content: &rest[..len],
        },
        &rest[len..],
    ))
}

/// All TLVs inside a constructed value.
pub fn der_children(content: &[u8]) -> Option<Vec<Tlv<'_>>> {
    let mut out = vec![];
    let mut cur = content;
    while !cur.is_empty() {
        let (t, rest) = der_read(cur)?;
        out.push(t);
        cur = rest;
    }
    Some(out)
}

pub fn der_write(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = content.len();
    if len < 0x80 {
        out.push(len as u8);
    } else {
        let bytes: Vec<u8> = len
            .to_be_bytes()
            .iter()
            .copied()
            .skip_while(|b| *b == 0)
            .collect();
        out.push(0x80 | bytes.len() as u8);
        out.extend(bytes);
    }
    out.extend_from_slice(content);
    out
}

pub fn der_seq(parts: &[Vec<u8>]) -> Vec<u8> {
    der_write(0x30, &parts.concat())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_port_split() {
        assert_eq!(
            split_host_port("example.com", 443).unwrap(),
            ("example.com".into(), 443)
        );
        assert_eq!(
            split_host_port("example.com:8443", 443).unwrap(),
            ("example.com".into(), 8443)
        );
        assert_eq!(
            split_host_port("[::1]:22", 443).unwrap(),
            ("::1".into(), 22)
        );
        assert_eq!(split_host_port("::1", 443).unwrap(), ("::1".into(), 443));
        assert_eq!(
            split_host_port("https://a.b/c", 1).unwrap(),
            ("a.b".into(), 443)
        );
        assert!(split_host_port("a:b", 1).is_err());
    }

    #[test]
    fn der_roundtrip() {
        let big = vec![7u8; 300];
        let enc = der_write(0x04, &big);
        let (t, rest) = der_read(&enc).unwrap();
        assert_eq!(t.tag, 0x04);
        assert_eq!(t.content.len(), 300);
        assert!(rest.is_empty());
    }
}
