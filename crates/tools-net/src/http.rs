//! HTTP & TCP tools: header/security analysis, favicon fetcher,
//! connectivity checks and a bounded port checker.

use crate::util::{connect_any, err_chain, manual_redirect_client, ms, split_host_port, table};
use dogubako_core::net::{check_url, resolve_checked};
use dogubako_core::prelude::*;
use futures::StreamExt;
use std::time::{Duration, Instant};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![http_headers(), favicon(), connectivity(), port_check()]
}

// ---------------------------------------------------------------------------
// Redirect chain

#[derive(Debug, Clone, serde::Serialize)]
pub struct Hop {
    pub url: String,
    pub status: u16,
    pub reason: String,
    pub location: Option<String>,
    pub time_ms: f64,
    pub http_version: String,
}

pub struct Chain {
    pub hops: Vec<Hop>,
    /// Headers of the final response (name, value) in order.
    pub headers: Vec<(String, String)>,
    pub final_url: String,
    pub error: Option<String>,
}

/// Follow redirects manually (each hop validated against the network
/// policy) and record every hop.
pub async fn http_chain(
    ctx: &ToolContext,
    start: &str,
    method: &str,
    max_redirects: usize,
    timeout: Duration,
) -> ToolResult<Chain> {
    let client = manual_redirect_client(ctx, timeout)?;
    let mut url = check_url(ctx.net_policy(), start)?;
    let mut hops = vec![];
    let mut seen = std::collections::HashSet::new();
    loop {
        ctx.check_cancelled()?;
        // Every hop is re-validated: IP literals and internal names are
        // rejected here, resolved addresses by the guarded resolver.
        let checked = check_url(ctx.net_policy(), url.as_str())?;
        let m = if method == "HEAD" {
            reqwest::Method::HEAD
        } else {
            reqwest::Method::GET
        };
        let t0 = Instant::now();
        let resp = match client
            .request(m, checked.clone())
            .header(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,*/*;q=0.8",
            )
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                let msg = err_chain(&e);
                if hops.is_empty() {
                    if msg.contains("blocked by network policy") {
                        return Err(ToolError::denied(msg));
                    }
                    return Err(ToolError::failed(format!(
                        "request to {checked} failed: {msg}"
                    )));
                }
                return Ok(Chain {
                    final_url: checked.to_string(),
                    hops,
                    headers: vec![],
                    error: Some(format!("request to {checked} failed: {msg}")),
                });
            }
        };
        let took = t0.elapsed();
        let status = resp.status();
        let location = resp
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let headers: Vec<(String, String)> = resp
            .headers()
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().to_string(),
                    String::from_utf8_lossy(v.as_bytes()).to_string(),
                )
            })
            .collect();
        hops.push(Hop {
            url: checked.to_string(),
            status: status.as_u16(),
            reason: status.canonical_reason().unwrap_or("").to_string(),
            location: location.clone(),
            time_ms: ms(took),
            http_version: format!("{:?}", resp.version()),
        });
        drop(resp);
        seen.insert(checked.to_string());
        if status.is_redirection() {
            if let Some(loc) = location {
                if hops.len() > max_redirects {
                    return Ok(Chain {
                        final_url: checked.to_string(),
                        hops,
                        headers,
                        error: Some(format!("stopped after {max_redirects} redirects")),
                    });
                }
                let next = checked.join(&loc).map_err(|e| {
                    ToolError::failed(format!("invalid redirect Location '{loc}': {e}"))
                })?;
                if let Err(e) = check_url(ctx.net_policy(), next.as_str()) {
                    return Ok(Chain {
                        final_url: checked.to_string(),
                        hops,
                        headers,
                        error: Some(format!("redirect to {next} refused: {e}")),
                    });
                }
                if seen.contains(next.as_str()) {
                    return Ok(Chain {
                        final_url: checked.to_string(),
                        hops,
                        headers,
                        error: Some(format!("redirect loop detected at {next}")),
                    });
                }
                url = next;
                continue;
            }
        }
        return Ok(Chain {
            final_url: checked.to_string(),
            hops,
            headers,
            error: None,
        });
    }
}

fn header<'a>(h: &'a [(String, String)], name: &str) -> Option<&'a str> {
    h.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Security header analysis → list of findings.
pub fn analyze_security(
    headers: &[(String, String)],
    final_url: &str,
    start_url: &str,
) -> (Vec<Value>, Vec<Value>, u32) {
    let https = final_url.starts_with("https://");
    let mut f = vec![];
    let mut score = 0u32;
    let mut push = |name: &str, status: &str, value: Option<&str>, detail: String| {
        if status == "good" {
            score += 1;
        }
        f.push(json!({ "header": name, "status": status, "value": value, "detail": detail }));
    };
    // HSTS
    match header(headers, "strict-transport-security") {
        Some(v) if https => {
            let lower = v.to_ascii_lowercase();
            let max_age = lower
                .split(';')
                .find_map(|p| {
                    p.trim()
                        .strip_prefix("max-age=")
                        .and_then(|x| x.trim_matches('"').parse::<u64>().ok())
                })
                .unwrap_or(0);
            if max_age < 15_552_000 {
                push("Strict-Transport-Security", "warn", Some(v), format!("max-age={max_age}s is short; use at least 15552000 (180 days), ideally 31536000"));
            } else {
                let mut d = format!("max-age {} days", max_age / 86400);
                if lower.contains("includesubdomains") {
                    d.push_str(", includeSubDomains");
                }
                if lower.contains("preload") {
                    d.push_str(", preload");
                }
                push("Strict-Transport-Security", "good", Some(v), d);
            }
        }
        Some(v) => push(
            "Strict-Transport-Security",
            "info",
            Some(v),
            "ignored by browsers over plain HTTP".into(),
        ),
        None if https => push(
            "Strict-Transport-Security",
            "missing",
            None,
            "add 'Strict-Transport-Security: max-age=31536000; includeSubDomains'".into(),
        ),
        None => push(
            "Strict-Transport-Security",
            "missing",
            None,
            "site is served over plain HTTP; serve it over HTTPS and enable HSTS".into(),
        ),
    }
    // CSP
    let csp = header(headers, "content-security-policy");
    match csp {
        Some(v) => {
            let l = v.to_ascii_lowercase();
            let mut issues = vec![];
            if l.contains("'unsafe-inline'")
                && !l.contains("'nonce-")
                && !l.contains("'strict-dynamic'")
            {
                issues.push("allows 'unsafe-inline'");
            }
            if l.contains("'unsafe-eval'") {
                issues.push("allows 'unsafe-eval'");
            }
            if l.split(';')
                .any(|d| d.split_whitespace().skip(1).any(|t| t == "*"))
            {
                issues.push("uses wildcard sources");
            }
            if !l.contains("default-src") && !l.contains("script-src") {
                issues.push("no default-src/script-src");
            }
            if issues.is_empty() {
                push("Content-Security-Policy", "good", Some(v), "present".into());
            } else {
                push(
                    "Content-Security-Policy",
                    "warn",
                    Some(v),
                    issues.join("; "),
                );
            }
        }
        None => match header(headers, "content-security-policy-report-only") {
            Some(v) => push(
                "Content-Security-Policy",
                "warn",
                Some(v),
                "only Report-Only is set; nothing is enforced".into(),
            ),
            None => push(
                "Content-Security-Policy",
                "missing",
                None,
                "define a Content-Security-Policy to mitigate XSS (start with default-src 'self')"
                    .into(),
            ),
        },
    }
    // X-Frame-Options / frame-ancestors
    let fa = csp
        .map(|c| c.to_ascii_lowercase().contains("frame-ancestors"))
        .unwrap_or(false);
    match header(headers, "x-frame-options") {
        Some(v)
            if matches!(
                v.trim().to_ascii_uppercase().as_str(),
                "DENY" | "SAMEORIGIN"
            ) =>
        {
            push(
                "X-Frame-Options",
                "good",
                Some(v),
                "clickjacking protection".into(),
            )
        }
        Some(v) => push(
            "X-Frame-Options",
            "warn",
            Some(v),
            "use DENY or SAMEORIGIN (ALLOW-FROM is obsolete)".into(),
        ),
        None if fa => push(
            "X-Frame-Options",
            "good",
            None,
            "covered by CSP frame-ancestors".into(),
        ),
        None => push(
            "X-Frame-Options",
            "missing",
            None,
            "add 'X-Frame-Options: DENY' or CSP frame-ancestors".into(),
        ),
    }
    match header(headers, "x-content-type-options") {
        Some(v) if v.trim().eq_ignore_ascii_case("nosniff") => push(
            "X-Content-Type-Options",
            "good",
            Some(v),
            "MIME sniffing disabled".into(),
        ),
        Some(v) => push(
            "X-Content-Type-Options",
            "warn",
            Some(v),
            "the only valid value is 'nosniff'".into(),
        ),
        None => push(
            "X-Content-Type-Options",
            "missing",
            None,
            "add 'X-Content-Type-Options: nosniff'".into(),
        ),
    }
    match header(headers, "referrer-policy") {
        Some(v)
            if v.to_ascii_lowercase().contains("unsafe-url")
                || v.trim().eq_ignore_ascii_case("no-referrer-when-downgrade") =>
        {
            push(
                "Referrer-Policy",
                "warn",
                Some(v),
                "leaks full URLs; prefer strict-origin-when-cross-origin".into(),
            )
        }
        Some(v) => push("Referrer-Policy", "good", Some(v), "set".into()),
        None => push(
            "Referrer-Policy",
            "missing",
            None,
            "add 'Referrer-Policy: strict-origin-when-cross-origin'".into(),
        ),
    }
    match header(headers, "permissions-policy") {
        Some(v) => push("Permissions-Policy", "good", Some(v), "set".into()),
        None => push("Permissions-Policy", "missing", None, "restrict powerful features, e.g. 'Permissions-Policy: camera=(), microphone=(), geolocation=()'".into()),
    }
    for (h, what) in [
        ("server", "server software"),
        ("x-powered-by", "framework"),
        ("x-aspnet-version", "ASP.NET version"),
    ] {
        if let Some(v) = header(headers, h) {
            if v.chars().any(|c| c.is_ascii_digit()) || h != "server" {
                push(
                    h,
                    "info",
                    Some(v),
                    format!("discloses {what}; consider removing version details"),
                );
            }
        }
    }
    if start_url.starts_with("http://") && !https {
        push(
            "HTTPS redirect",
            "warn",
            None,
            "plain-HTTP URL does not redirect to HTTPS".into(),
        );
    } else if start_url.starts_with("http://") && https {
        push(
            "HTTPS redirect",
            "good",
            None,
            "HTTP redirects to HTTPS".into(),
        );
    }
    // Cookies
    let mut cookies = vec![];
    for (k, v) in headers {
        if !k.eq_ignore_ascii_case("set-cookie") {
            continue;
        }
        let name = v
            .split(';')
            .next()
            .and_then(|nv| nv.split('=').next())
            .unwrap_or("")
            .trim()
            .to_string();
        let attrs: Vec<String> = v
            .split(';')
            .skip(1)
            .map(|a| a.trim().to_ascii_lowercase())
            .collect();
        let secure = attrs.iter().any(|a| a == "secure");
        let http_only = attrs.iter().any(|a| a == "httponly");
        let same_site = attrs
            .iter()
            .find_map(|a| a.strip_prefix("samesite=").map(|s| s.to_string()));
        let mut issues = vec![];
        if !secure && https {
            issues.push("missing Secure");
        }
        if !http_only {
            issues.push("missing HttpOnly (fine only if JavaScript must read it)");
        }
        if same_site.is_none() {
            issues.push("no SameSite (browsers default to Lax)");
        }
        if same_site.as_deref() == Some("none") && !secure {
            issues.push("SameSite=None requires Secure");
        }
        cookies.push(json!({ "name": name, "secure": secure, "http_only": http_only, "same_site": same_site, "issues": issues }));
    }
    (f, cookies, score)
}

fn http_headers() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.http_headers", "HTTP Headers & Security Check", Category::Network)
        .describe("Fetch a URL and show status, every redirect hop, response headers and a security-header analysis (HSTS, CSP, frame options, cookies…) with recommendations")
        .keywords(&["http", "headers", "redirect", "hsts", "csp", "security headers", "cookies", "curl -I", "status code"])
        .param(ParamSpec::text("url").label("URL").required().primary().placeholder("https://example.com"))
        .param(ParamSpec::choice_of("method", &["GET", "HEAD"]).default_str("GET"))
        .param(ParamSpec::integer("max_redirects").label("Max redirects to follow").range(0.0, 20.0).default_int(10))
        .param(ParamSpec::integer("timeout_secs").label("Timeout per request (s)").range(1.0, 30.0).default_int(10).advanced())
        .permission(Permission::Network)
        .timeout(120);
    async_tool(spec, |ctx, p| async move {
        let mut raw = p.str("url")?.trim().to_string();
        if !raw.contains("://") {
            raw = format!("https://{raw}");
        }
        let started = Instant::now();
        let chain = http_chain(
            &ctx,
            &raw,
            p.str_or("method", "GET"),
            p.int_or("max_redirects", 10) as usize,
            Duration::from_secs(p.int_or("timeout_secs", 10) as u64),
        )
        .await?;
        let (security, cookies, score) = analyze_security(&chain.headers, &chain.final_url, &raw);
        let last = chain.hops.last().cloned();
        let rows: Vec<Vec<Value>> = chain
            .headers
            .iter()
            .map(|(k, v)| vec![json!(k), json!(v)])
            .collect();
        let mut md = String::new();
        md.push_str("## Redirect chain\n\n| # | status | URL | time |\n|---|---|---|---|\n");
        for (i, h) in chain.hops.iter().enumerate() {
            md.push_str(&format!(
                "| {} | {} {} | {} | {} ms |\n",
                i + 1,
                h.status,
                h.reason,
                h.url,
                h.time_ms
            ));
        }
        if let Some(e) = &chain.error {
            md.push_str(&format!("\n**Stopped:** {e}\n"));
        }
        md.push_str(&format!("\n## Security headers ({score} good)\n\n"));
        for s in &security {
            let icon = match s["status"].as_str().unwrap_or("") {
                "good" => "✅",
                "warn" => "⚠️",
                "missing" => "❌",
                _ => "ℹ️",
            };
            md.push_str(&format!(
                "- {icon} **{}** — {}\n",
                s["header"].as_str().unwrap_or(""),
                s["detail"].as_str().unwrap_or("")
            ));
        }
        for c in &cookies {
            let issues: Vec<&str> = c["issues"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            if !issues.is_empty() {
                md.push_str(&format!(
                    "- 🍪 cookie `{}`: {}\n",
                    c["name"].as_str().unwrap_or(""),
                    issues.join(", ")
                ));
            }
        }
        let data = json!({
            "final_url": chain.final_url,
            "status": last.as_ref().map(|h| h.status),
            "http_version": last.as_ref().map(|h| h.http_version.clone()),
            "redirect_count": chain.hops.len().saturating_sub(1),
            "redirects": chain.hops,
            "stopped": chain.error,
            "security": security,
            "security_score": score,
            "cookies": cookies,
            "total_ms": ms(started.elapsed()),
            "table": table(&["header", "value"], rows),
        });
        Ok(ToolOutput::text_as(md, "text/markdown").with_data(data))
    })
}

// ---------------------------------------------------------------------------
// Favicon

/// Width/height of common icon formats, parsed from headers only.
pub fn image_dims(b: &[u8]) -> Option<(u32, u32, &'static str)> {
    if b.len() >= 24 && b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((
            u32::from_be_bytes(b[16..20].try_into().ok()?),
            u32::from_be_bytes(b[20..24].try_into().ok()?),
            "png",
        ));
    }
    if b.len() >= 10 && (b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a")) {
        return Some((
            u16::from_le_bytes([b[6], b[7]]) as u32,
            u16::from_le_bytes([b[8], b[9]]) as u32,
            "gif",
        ));
    }
    if b.len() >= 6 && b[0..4] == [0, 0, 1, 0] {
        // ICO: pick the largest entry; 0 means 256.
        let n = u16::from_le_bytes([b[4], b[5]]) as usize;
        let mut best = (0u32, 0u32);
        for i in 0..n.min(64) {
            let o = 6 + i * 16;
            if b.len() < o + 2 {
                break;
            }
            let w = if b[o] == 0 { 256 } else { b[o] as u32 };
            let h = if b[o + 1] == 0 { 256 } else { b[o + 1] as u32 };
            if w * h > best.0 * best.1 {
                best = (w, h);
            }
        }
        return Some((best.0, best.1, "ico"));
    }
    if b.len() >= 4 && b[0] == 0xff && b[1] == 0xd8 {
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xff {
                i += 1;
                continue;
            }
            let marker = b[i + 1];
            let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
            if (0xc0..=0xcf).contains(&marker) && marker != 0xc4 && marker != 0xc8 && marker != 0xcc
            {
                return Some((
                    u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32,
                    u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32,
                    "jpeg",
                ));
            }
            i += 2 + len;
        }
        return None;
    }
    if b.len() >= 30 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        match &b[12..16] {
            b"VP8X" => {
                return Some((
                    1 + u32::from_le_bytes([b[24], b[25], b[26], 0]),
                    1 + u32::from_le_bytes([b[27], b[28], b[29], 0]),
                    "webp",
                ))
            }
            b"VP8 " => {
                return Some((
                    u16::from_le_bytes([b[26], b[27]]) as u32 & 0x3fff,
                    u16::from_le_bytes([b[28], b[29]]) as u32 & 0x3fff,
                    "webp",
                ))
            }
            b"VP8L" if b.len() >= 25 => {
                let bits = u32::from_le_bytes([b[21], b[22], b[23], b[24]]);
                return Some(((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1, "webp"));
            }
            _ => {}
        }
    }
    None
}

fn is_svg(b: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&b[..b.len().min(1024)]).to_ascii_lowercase();
    head.contains("<svg")
}

struct IconRef {
    url: url::Url,
    source: String,
    sizes: Option<String>,
}

const MAX_ICONS: usize = 20;

fn favicon() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.favicon", "Favicon Fetcher", Category::Network)
        .describe("Find and download a website's favicons: <link rel=icon>, apple-touch-icon, web manifest icons and /favicon.ico")
        .keywords(&["favicon", "icon", "apple-touch-icon", "manifest", "logo", "website icon"])
        .param(ParamSpec::text("url").label("Website URL").required().primary().placeholder("https://example.com"))
        .param(ParamSpec::boolean("download").label("Download icon files").default_bool(true))
        .permission(Permission::Network)
        .output(OutputKind::Files)
        .progress()
        .timeout(120);
    async_tool(spec, |ctx, p| async move {
        let mut raw = p.str("url")?.trim().to_string();
        if !raw.contains("://") {
            raw = format!("https://{raw}");
        }
        let page = crate::util::fetch(
            &ctx,
            &raw,
            "text/html,application/xhtml+xml",
            3 * 1024 * 1024,
            Duration::from_secs(15),
        )
        .await?;
        let base =
            url::Url::parse(&page.final_url).map_err(|e| ToolError::failed(e.to_string()))?;
        let html = String::from_utf8_lossy(&page.body).to_string();
        let mut refs: Vec<IconRef> = vec![];
        let mut manifest_url = None;
        {
            let doc = scraper::Html::parse_document(&html);
            let base_href = scraper::Selector::parse("base[href]")
                .ok()
                .and_then(|s| {
                    doc.select(&s)
                        .next()
                        .and_then(|e| e.value().attr("href"))
                        .and_then(|h| base.join(h).ok())
                })
                .unwrap_or_else(|| base.clone());
            if let Ok(sel) = scraper::Selector::parse("link[rel][href]") {
                for el in doc.select(&sel) {
                    let rel = el.value().attr("rel").unwrap_or("").to_ascii_lowercase();
                    let href = el.value().attr("href").unwrap_or("").trim();
                    if href.is_empty() || href.starts_with("data:") {
                        continue;
                    }
                    let Ok(u) = base_href.join(href) else {
                        continue;
                    };
                    if rel.split_whitespace().any(|r| r == "manifest") {
                        manifest_url = Some(u);
                    } else if rel.split_whitespace().any(|r| {
                        matches!(
                            r,
                            "icon"
                                | "apple-touch-icon"
                                | "apple-touch-icon-precomposed"
                                | "mask-icon"
                                | "fluid-icon"
                        )
                    }) {
                        refs.push(IconRef {
                            url: u,
                            source: format!("link rel=\"{rel}\""),
                            sizes: el.value().attr("sizes").map(|s| s.to_string()),
                        });
                    }
                }
            }
        }
        let mut warnings = vec![];
        if let Some(mu) = manifest_url {
            match crate::util::fetch(
                &ctx,
                mu.as_str(),
                "application/manifest+json,application/json",
                512 * 1024,
                Duration::from_secs(10),
            )
            .await
            {
                Ok(m) => match serde_json::from_slice::<Value>(&m.body) {
                    Ok(v) => {
                        let mbase = url::Url::parse(&m.final_url).unwrap_or(mu.clone());
                        for icon in v["icons"].as_array().into_iter().flatten() {
                            if let Some(src) = icon["src"].as_str() {
                                if let Ok(u) = mbase.join(src) {
                                    refs.push(IconRef {
                                        url: u,
                                        source: "web manifest".into(),
                                        sizes: icon["sizes"].as_str().map(|s| s.to_string()),
                                    });
                                }
                            }
                        }
                    }
                    Err(e) => warnings.push(format!("manifest {mu} is not valid JSON: {e}")),
                },
                Err(e) => warnings.push(format!("manifest {mu}: {e}")),
            }
        }
        if let Ok(u) = base.join("/favicon.ico") {
            refs.push(IconRef {
                url: u,
                source: "/favicon.ico".into(),
                sizes: None,
            });
        }
        let mut seen = std::collections::HashSet::new();
        refs.retain(|r| {
            matches!(r.url.scheme(), "http" | "https") && seen.insert(r.url.to_string())
        });
        if refs.len() > MAX_ICONS {
            warnings.push(format!(
                "{} icons referenced; only the first {MAX_ICONS} are fetched",
                refs.len()
            ));
            refs.truncate(MAX_ICONS);
        }
        let download = p.bool("download");
        let mut rows = vec![];
        let mut artifacts = vec![];
        let total = refs.len().max(1);
        for (i, r) in refs.iter().enumerate() {
            ctx.check_cancelled()?;
            ctx.progress(i as f32 / total as f32, format!("fetching {}", r.url));
            let res = crate::util::fetch(
                &ctx,
                r.url.as_str(),
                "image/*,*/*;q=0.5",
                2 * 1024 * 1024,
                Duration::from_secs(10),
            )
            .await;
            match res {
                Ok(f) if (200..300).contains(&f.status) && !f.body.is_empty() => {
                    let dims = image_dims(&f.body);
                    let (fmt, dim_s) = match dims {
                        Some((w, h, fmt)) => (fmt.to_string(), format!("{w}×{h}")),
                        None if is_svg(&f.body) => ("svg".to_string(), "vector".to_string()),
                        None => (
                            dogubako_core::util::sniff_bytes(&f.body)
                                .map(|m| m.to_string())
                                .unwrap_or_else(|| "unknown".into()),
                            String::new(),
                        ),
                    };
                    let is_image = dims.is_some() || is_svg(&f.body);
                    if download && is_image {
                        let ext = if fmt == "jpeg" {
                            "jpg".to_string()
                        } else {
                            fmt.clone()
                        };
                        let name = format!(
                            "favicon-{}{}.{}",
                            i + 1,
                            if dim_s.contains('×') {
                                format!("-{}", dim_s.replace('×', "x"))
                            } else {
                                String::new()
                            },
                            ext
                        );
                        artifacts.push(ctx.write_output(&name, &f.body)?);
                    }
                    rows.push(vec![
                        json!(r.source),
                        json!(r.url.as_str()),
                        json!(r.sizes),
                        json!(f.content_type),
                        json!(fmt),
                        json!(dim_s),
                        json!(f.body.len()),
                        json!(if is_image { "ok" } else { "not an image" }),
                    ]);
                }
                Ok(f) => rows.push(vec![
                    json!(r.source),
                    json!(r.url.as_str()),
                    json!(r.sizes),
                    json!(f.content_type),
                    Value::Null,
                    Value::Null,
                    json!(f.body.len()),
                    json!(format!("HTTP {}", f.status)),
                ]),
                Err(e) => rows.push(vec![
                    json!(r.source),
                    json!(r.url.as_str()),
                    json!(r.sizes),
                    Value::Null,
                    Value::Null,
                    Value::Null,
                    Value::Null,
                    json!(e.to_string()),
                ]),
            }
        }
        ctx.progress(1.0, "done");
        let found = rows.iter().filter(|r| r[7] == "ok").count();
        let mut out = ToolOutput::files(artifacts)
            .with_text(format!("{found} icon(s) found for {}", page.final_url), "text/plain")
            .with_data(json!({ "page": page.final_url, "icons_found": found, "table": table(&["source", "url", "declared sizes", "content type", "format", "dimensions", "bytes", "status"], rows) }));
        for w in warnings {
            out = out.warn(w);
        }
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// Connectivity

async fn tcp_test(
    ctx: &ToolContext,
    host: &str,
    port: u16,
    attempts: usize,
    timeout: Duration,
) -> Value {
    let t0 = Instant::now();
    let addrs = match resolve_checked(ctx.net_policy(), host, port).await {
        Ok(a) => a,
        Err(e) => {
            return json!({ "target": format!("{host}:{port}"), "kind": "tcp", "ok": false, "error": e.to_string() })
        }
    };
    let dns_ms = ms(t0.elapsed());
    let mut times = vec![];
    let mut last_err = None;
    let mut addr_used = None;
    for i in 0..attempts {
        if ctx.is_cancelled() {
            break;
        }
        match connect_any(&addrs, timeout).await {
            Ok((s, a, d)) => {
                drop(s);
                addr_used = Some(a.to_string());
                times.push(ms(d));
            }
            Err(e) => last_err = Some(e.to_string()),
        }
        if i + 1 < attempts {
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }
    let ok = !times.is_empty();
    let (min, max, avg) = stats(&times);
    json!({
        "target": format!("{host}:{port}"), "kind": "tcp", "ok": ok, "address": addr_used,
        "dns_ms": dns_ms, "attempts": attempts, "successes": times.len(),
        "min_ms": min, "avg_ms": avg, "max_ms": max, "status": Value::Null, "error": if ok { Value::Null } else { json!(last_err) },
    })
}

fn stats(t: &[f64]) -> (Option<f64>, Option<f64>, Option<f64>) {
    if t.is_empty() {
        return (None, None, None);
    }
    let min = t.iter().cloned().fold(f64::MAX, f64::min);
    let max = t.iter().cloned().fold(0.0, f64::max);
    let avg = (t.iter().sum::<f64>() / t.len() as f64 * 10.0).round() / 10.0;
    (Some(min), Some(max), Some(avg))
}

async fn http_test(ctx: &ToolContext, url: &str, attempts: usize, timeout: Duration) -> Value {
    let client = match manual_redirect_client(ctx, timeout) {
        Ok(c) => c,
        Err(e) => {
            return json!({ "target": url, "kind": "http", "ok": false, "error": e.to_string() })
        }
    };
    let u = match check_url(ctx.net_policy(), url) {
        Ok(u) => u,
        Err(e) => {
            return json!({ "target": url, "kind": "http", "ok": false, "error": e.to_string() })
        }
    };
    let mut times = vec![];
    let mut status = None;
    let mut last_err = None;
    for _ in 0..attempts {
        if ctx.is_cancelled() {
            break;
        }
        let t0 = Instant::now();
        match client.get(u.clone()).send().await {
            Ok(r) => {
                status = Some(r.status().as_u16());
                times.push(ms(t0.elapsed()));
            }
            Err(e) => last_err = Some(err_chain(&e)),
        }
    }
    let (min, max, avg) = stats(&times);
    let ok = status.map(|s| s < 500).unwrap_or(false);
    json!({
        "target": url, "kind": "http", "ok": ok, "status": status, "attempts": attempts, "successes": times.len(),
        "min_ms": min, "avg_ms": avg, "max_ms": max, "error": if times.is_empty() { json!(last_err) } else { Value::Null },
    })
}

fn connectivity() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.connectivity", "Connectivity Check", Category::Network)
        .describe("Test TCP reachability (host:port) and HTTP(S) availability with latency over several attempts — no ICMP needed")
        .keywords(&["ping", "reachability", "tcp", "latency", "uptime", "connect", "port", "http check"])
        .param(ParamSpec::textarea("targets").label("Targets").describe("One per line: host:port or http(s):// URL (max 20)").required().primary().placeholder("example.com:443\nhttps://example.org"))
        .param(ParamSpec::integer("attempts").label("Attempts per target").range(1.0, 10.0).default_int(3))
        .param(ParamSpec::integer("timeout_ms").label("Timeout per attempt (ms)").range(100.0, 15000.0).default_int(3000).advanced())
        .permission(Permission::Network)
        .progress()
        .timeout(300);
    async_tool(spec, |ctx, p| async move {
        let targets: Vec<String> = p
            .text("targets")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|s| s.to_string())
            .collect();
        if targets.is_empty() {
            return Err(ToolError::invalid("targets", "enter at least one target"));
        }
        if targets.len() > 20 {
            return Err(ToolError::invalid("targets", "at most 20 targets per run"));
        }
        let attempts = p.int_or("attempts", 3) as usize;
        let timeout = Duration::from_millis(p.int_or("timeout_ms", 3000) as u64);
        let mut parsed = vec![];
        for t in &targets {
            if t.starts_with("http://") || t.starts_with("https://") {
                parsed.push((t.clone(), None));
            } else {
                let (h, port) = split_host_port(t, 0)?;
                if port == 0 {
                    return Err(ToolError::invalid(
                        "targets",
                        format!("'{t}' needs a port (host:port) or an http(s):// URL"),
                    ));
                }
                parsed.push((crate::util::clean_host("targets", &h)?, Some(port)));
            }
        }
        let total = parsed.len();
        let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let results: Vec<Value> = futures::stream::iter(parsed)
            .map(|(t, port)| {
                let ctx = ctx.clone();
                let done = done.clone();
                async move {
                    let v = match port {
                        Some(port) => tcp_test(&ctx, &t, port, attempts, timeout).await,
                        None => http_test(&ctx, &t, attempts, timeout).await,
                    };
                    let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    ctx.progress(n as f32 / total as f32, format!("{n}/{total}"));
                    v
                }
            })
            .buffered(8)
            .collect()
            .await;
        let rows: Vec<Vec<Value>> = results
            .iter()
            .map(|r| {
                vec![
                    r["target"].clone(),
                    r["kind"].clone(),
                    r["ok"].clone(),
                    r.get("status").cloned().unwrap_or(Value::Null),
                    json!(format!(
                        "{}/{}",
                        r.get("successes").and_then(|v| v.as_u64()).unwrap_or(0),
                        attempts
                    )),
                    r.get("min_ms").cloned().unwrap_or(Value::Null),
                    r.get("avg_ms").cloned().unwrap_or(Value::Null),
                    r.get("max_ms").cloned().unwrap_or(Value::Null),
                    r.get("error").cloned().unwrap_or(Value::Null),
                ]
            })
            .collect();
        let ok = results.iter().filter(|r| r["ok"] == true).count();
        Ok(ToolOutput::text(format!("{ok}/{} targets reachable", results.len())).with_data(json!({ "reachable": ok, "results": results, "table": table(&["target", "kind", "ok", "status", "successes", "min ms", "avg ms", "max ms", "error"], rows) })))
    })
}

// ---------------------------------------------------------------------------
// Port check

pub const COMMON_PORTS: &[(u16, &str)] = &[
    (21, "ftp"),
    (22, "ssh"),
    (23, "telnet"),
    (25, "smtp"),
    (53, "dns"),
    (80, "http"),
    (110, "pop3"),
    (143, "imap"),
    (443, "https"),
    (465, "smtps"),
    (587, "submission"),
    (993, "imaps"),
    (995, "pop3s"),
    (1433, "mssql"),
    (1521, "oracle"),
    (2049, "nfs"),
    (3306, "mysql"),
    (3389, "rdp"),
    (5432, "postgresql"),
    (5900, "vnc"),
    (6379, "redis"),
    (8080, "http-alt"),
    (8443, "https-alt"),
    (9200, "elasticsearch"),
    (27017, "mongodb"),
];

pub const MAX_PORTS: usize = 50;

pub fn parse_ports(s: &str) -> ToolResult<Vec<u16>> {
    let mut out = vec![];
    for part in s
        .split([',', ' ', '\n', ';'])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (part, part),
        };
        let a: u16 = a
            .parse()
            .map_err(|_| ToolError::invalid("ports", format!("'{part}' is not a port")))?;
        let b: u16 = b
            .parse()
            .map_err(|_| ToolError::invalid("ports", format!("'{part}' is not a port")))?;
        if a == 0 || b < a {
            return Err(ToolError::invalid(
                "ports",
                format!("invalid range '{part}'"),
            ));
        }
        if (b - a) as usize >= MAX_PORTS {
            return Err(ToolError::invalid(
                "ports",
                format!("at most {MAX_PORTS} ports per run"),
            ));
        }
        for p in a..=b {
            if !out.contains(&p) {
                out.push(p);
            }
        }
        if out.len() > MAX_PORTS {
            return Err(ToolError::invalid(
                "ports",
                format!("at most {MAX_PORTS} ports per run"),
            ));
        }
    }
    if out.is_empty() {
        return Err(ToolError::invalid("ports", "list at least one port"));
    }
    Ok(out)
}

fn port_check() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.port_check", "Port Checker", Category::Network)
        .describe("Check which TCP ports are open on a host (common services or up to 50 listed ports) with short timeouts and bounded concurrency")
        .keywords(&["port", "open port", "tcp", "firewall", "port check", "service", "listening"])
        .param(ParamSpec::text("host").label("Host").required().primary().placeholder("example.com"))
        .param(ParamSpec::choice("preset", &[("common", "Common service ports (25)"), ("custom", "Custom list")]).default_str("common"))
        .param(ParamSpec::text("ports").label("Ports").describe("e.g. 22,80,443,8000-8010 (max 50)").show_if("preset", &["custom"]))
        .param(ParamSpec::integer("timeout_ms").label("Timeout per port (ms)").range(100.0, 5000.0).default_int(1500).advanced())
        .permission(Permission::Network)
        .progress()
        .timeout(120);
    async_tool(spec, |ctx, p| async move {
        let (host, _) = split_host_port(p.str("host")?, 0)?;
        let host = crate::util::clean_host("host", &host)?;
        let ports: Vec<u16> = if p.str_or("preset", "common") == "custom" {
            parse_ports(p.str("ports")?)?
        } else {
            COMMON_PORTS.iter().map(|(p, _)| *p).collect()
        };
        let timeout = Duration::from_millis(p.int_or("timeout_ms", 1500) as u64);
        // Resolve once with the policy applied: private targets are refused
        // when the instance only allows public addresses.
        let addrs = resolve_checked(ctx.net_policy(), &host, 0).await?;
        let ip = addrs[0].ip();
        let total = ports.len();
        let done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut results: Vec<(u16, &'static str, Option<f64>, String)> =
            futures::stream::iter(ports)
                .map(|port| {
                    let ctx = ctx.clone();
                    let done = done.clone();
                    async move {
                        let t0 = Instant::now();
                        let r = tokio::time::timeout(
                            timeout,
                            tokio::net::TcpStream::connect((ip, port)),
                        )
                        .await;
                        let (state, lat) = match r {
                            Ok(Ok(_)) => ("open", Some(ms(t0.elapsed()))),
                            Ok(Err(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                                ("closed", None)
                            }
                            Ok(Err(_)) => ("unreachable", None),
                            Err(_) => ("filtered", None),
                        };
                        let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        ctx.progress(n as f32 / total as f32, format!("{n}/{total} ports"));
                        let svc = COMMON_PORTS
                            .iter()
                            .find(|(p, _)| *p == port)
                            .map(|(_, s)| *s)
                            .unwrap_or("");
                        (port, svc, lat, state.to_string())
                    }
                })
                .buffer_unordered(16)
                .collect()
                .await;
        results.sort_by_key(|r| r.0);
        let open: Vec<u16> = results
            .iter()
            .filter(|r| r.3 == "open")
            .map(|r| r.0)
            .collect();
        let rows: Vec<Vec<Value>> = results
            .iter()
            .map(|(p, s, l, st)| vec![json!(p), json!(s), json!(st), json!(l)])
            .collect();
        Ok(ToolOutput::text(format!("{host} ({ip}): {} open — {}", open.len(), if open.is_empty() { "none".into() } else { open.iter().map(|p| p.to_string()).collect::<Vec<_>>().join(", ") }))
            .with_data(json!({ "host": host, "address": ip.to_string(), "open_ports": open, "table": table(&["port", "service", "state", "latency ms"], rows) })))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ports() {
        assert_eq!(
            parse_ports("22, 80,8000-8002").unwrap(),
            vec![22, 80, 8000, 8001, 8002]
        );
        assert!(parse_ports("1-100").is_err());
        assert!(parse_ports("0").is_err());
        assert!(parse_ports("abc").is_err());
    }

    #[test]
    fn dims() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&32u32.to_be_bytes());
        png.extend_from_slice(&16u32.to_be_bytes());
        assert_eq!(image_dims(&png), Some((32, 16, "png")));
        let ico = [
            0u8, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 32, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        assert_eq!(image_dims(&ico), Some((256, 256, "ico")));
    }

    #[test]
    fn security_analysis() {
        let h = vec![
            (
                "strict-transport-security".to_string(),
                "max-age=63072000; includeSubDomains; preload".to_string(),
            ),
            ("x-content-type-options".to_string(), "nosniff".to_string()),
            ("set-cookie".to_string(), "sid=1; Path=/".to_string()),
        ];
        let (f, cookies, score) = analyze_security(&h, "https://a.example/", "http://a.example/");
        assert!(score >= 3);
        assert!(f
            .iter()
            .any(|x| x["header"] == "Content-Security-Policy" && x["status"] == "missing"));
        assert_eq!(cookies[0]["secure"], false);
        assert!(cookies[0]["issues"].as_array().unwrap().len() >= 2);
    }
}
