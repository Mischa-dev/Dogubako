//! Registration data: RDAP (with IANA bootstrap) and classic WHOIS.

use crate::util::{connect_any, fetch, table};
use dogubako_core::net::resolve_checked;
use dogubako_core::prelude::*;
use ipnet::IpNet;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![rdap(), whois()]
}

// ---------------------------------------------------------------------------
// RDAP

const BOOTSTRAP_TTL: Duration = Duration::from_secs(24 * 3600);
const RDAP_FALLBACK: &str = "https://rdap.org/";

type BootstrapCache = Mutex<HashMap<String, (Instant, Arc<Value>)>>;

fn bootstrap_cache() -> &'static BootstrapCache {
    static C: OnceLock<BootstrapCache> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn bootstrap(ctx: &ToolContext, kind: &str) -> ToolResult<Arc<Value>> {
    if let Some((t, v)) = bootstrap_cache().lock().unwrap().get(kind) {
        if t.elapsed() < BOOTSTRAP_TTL {
            return Ok(v.clone());
        }
    }
    let url = format!("https://data.iana.org/rdap/{kind}.json");
    let f = fetch(
        ctx,
        &url,
        "application/json",
        4 * 1024 * 1024,
        Duration::from_secs(15),
    )
    .await?;
    if f.status != 200 {
        return Err(ToolError::failed(format!(
            "IANA bootstrap {url} returned HTTP {}",
            f.status
        )));
    }
    let v: Value = serde_json::from_slice(&f.body)
        .map_err(|e| ToolError::failed(format!("IANA bootstrap {url}: {e}")))?;
    let v = Arc::new(v);
    bootstrap_cache()
        .lock()
        .unwrap()
        .insert(kind.to_string(), (Instant::now(), v.clone()));
    Ok(v)
}

#[derive(Debug, Clone, PartialEq)]
pub enum RdapQuery {
    Domain(String),
    Ip(IpNet),
    Asn(u32),
}

impl RdapQuery {
    pub fn parse(s: &str) -> ToolResult<Self> {
        let t = s.trim();
        let upper = t.to_ascii_uppercase();
        if let Some(n) = upper.strip_prefix("AS").and_then(|n| n.parse::<u32>().ok()) {
            return Ok(RdapQuery::Asn(n));
        }
        if let Ok(n) = t.parse::<u32>() {
            return Ok(RdapQuery::Asn(n));
        }
        if let Ok(ip) = t.trim_matches(['[', ']']).parse::<IpAddr>() {
            return Ok(RdapQuery::Ip(IpNet::from(ip)));
        }
        if let Ok(n) = t.parse::<IpNet>() {
            return Ok(RdapQuery::Ip(n.trunc()));
        }
        let h = crate::util::clean_host("query", t)?;
        let ascii = idna::domain_to_ascii(&h).map_err(|_| {
            ToolError::invalid("query", format!("'{h}' is not a domain, IP, CIDR or ASN"))
        })?;
        if !ascii.contains('.') {
            return Err(ToolError::invalid(
                "query",
                format!("'{h}' is not a domain, IP, CIDR or ASN"),
            ));
        }
        Ok(RdapQuery::Domain(ascii.to_ascii_lowercase()))
    }

    fn kind(&self) -> &'static str {
        match self {
            RdapQuery::Domain(_) => "dns",
            RdapQuery::Ip(IpNet::V4(_)) => "ipv4",
            RdapQuery::Ip(IpNet::V6(_)) => "ipv6",
            RdapQuery::Asn(_) => "asn",
        }
    }

    fn path(&self) -> String {
        match self {
            RdapQuery::Domain(d) => format!("domain/{d}"),
            RdapQuery::Ip(n) if n.prefix_len() == n.max_prefix_len() => format!("ip/{}", n.addr()),
            RdapQuery::Ip(n) => format!("ip/{}/{}", n.network(), n.prefix_len()),
            RdapQuery::Asn(a) => format!("autnum/{a}"),
        }
    }
}

/// Pick the RDAP base URL from a bootstrap registry (RFC 9224).
pub fn find_service(boot: &Value, q: &RdapQuery) -> Option<String> {
    let services = boot["services"].as_array()?;
    let mut best: Option<(usize, String)> = None;
    for svc in services {
        let entries = svc.get(0)?.as_array()?;
        let urls: Vec<&str> = svc
            .get(1)?
            .as_array()?
            .iter()
            .filter_map(|u| u.as_str())
            .collect();
        let url = urls
            .iter()
            .find(|u| u.starts_with("https://"))
            .or(urls.first())?
            .to_string();
        for e in entries.iter().filter_map(|e| e.as_str()) {
            let score = match q {
                RdapQuery::Domain(d) => {
                    let e = e.to_ascii_lowercase();
                    if d == &e || d.ends_with(&format!(".{e}")) {
                        Some(e.len())
                    } else {
                        None
                    }
                }
                RdapQuery::Ip(n) => e
                    .parse::<IpNet>()
                    .ok()
                    .filter(|net| net.contains(n))
                    .map(|net| net.prefix_len() as usize),
                RdapQuery::Asn(a) => {
                    let (lo, hi) = e.split_once('-').unwrap_or((e, e));
                    match (lo.parse::<u32>(), hi.parse::<u32>()) {
                        (Ok(lo), Ok(hi)) if (lo..=hi).contains(a) => {
                            Some(usize::MAX - (hi - lo) as usize)
                        }
                        _ => None,
                    }
                }
            };
            if let Some(s) = score {
                if best.as_ref().map(|b| s > b.0).unwrap_or(true) {
                    best = Some((s, url.clone()));
                }
            }
        }
    }
    best.map(|b| {
        if b.1.ends_with('/') {
            b.1
        } else {
            format!("{}/", b.1)
        }
    })
}

fn vcard_field(entity: &Value, field: &str) -> Option<String> {
    entity["vcardArray"]
        .get(1)?
        .as_array()?
        .iter()
        .find(|p| p.get(0).and_then(|n| n.as_str()) == Some(field))
        .and_then(|p| p.get(3))
        .map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Array(a) => a
                .iter()
                .filter_map(|x| x.as_str())
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(", "),
            other => other.to_string(),
        })
}

fn collect_entities<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    for e in v["entities"].as_array().into_iter().flatten() {
        out.push(e);
        collect_entities(e, out);
    }
}

/// Extract the key fields of an RDAP response.
pub fn summarize_rdap(v: &Value) -> Value {
    let events: serde_json::Map<String, Value> = v["events"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|e| {
            Some((
                e["eventAction"].as_str()?.replace(' ', "_"),
                e["eventDate"].clone(),
            ))
        })
        .collect();
    let mut ents = vec![];
    collect_entities(v, &mut ents);
    let entities: Vec<Value> = ents
        .iter()
        .map(|e| {
            json!({
                "handle": e["handle"],
                "roles": e["roles"],
                "name": vcard_field(e, "fn"),
                "organization": vcard_field(e, "org"),
                "email": vcard_field(e, "email"),
                "country": e["vcardArray"].get(1).and_then(|a| a.as_array()).and_then(|a| a.iter().find(|p| p[0] == "adr")).and_then(|p| p[1]["cc"].as_str().map(|s| s.to_string())),
                "iana_id": e["publicIds"].as_array().and_then(|a| a.iter().find(|p| p["type"].as_str().map(|t| t.contains("IANA")).unwrap_or(false))).map(|p| p["identifier"].clone()),
            })
        })
        .collect();
    let registrar = entities
        .iter()
        .find(|e| {
            e["roles"]
                .as_array()
                .map(|r| r.iter().any(|x| x == "registrar"))
                .unwrap_or(false)
        })
        .cloned();
    let nameservers: Vec<Value> = v["nameservers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| n["ldhName"].as_str().map(|s| json!(s.to_ascii_lowercase())))
        .collect();
    let mut m = json!({
        "object_class": v["objectClassName"],
        "handle": v["handle"],
        "name": v["ldhName"].as_str().or(v["name"].as_str()),
        "unicode_name": v["unicodeName"],
        "status": v["status"],
        "events": events,
        "registrar": registrar.as_ref().and_then(|r| r["name"].as_str().map(|s| s.to_string())),
        "registrar_iana_id": registrar.as_ref().map(|r| r["iana_id"].clone()),
        "nameservers": nameservers,
        "dnssec_signed": v["secureDNS"]["delegationSigned"],
        "entities": entities,
        "port43": v["port43"],
    });
    if v["objectClassName"] == "ip network" {
        m["start_address"] = v["startAddress"].clone();
        m["end_address"] = v["endAddress"].clone();
        m["ip_version"] = v["ipVersion"].clone();
        m["network_type"] = v["type"].clone();
        m["country"] = v["country"].clone();
        m["parent_handle"] = v["parentHandle"].clone();
        m["cidrs"] = json!(v["cidr0_cidrs"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| format!(
                "{}/{}",
                c["v4prefix"]
                    .as_str()
                    .or(c["v6prefix"].as_str())
                    .unwrap_or(""),
                c["length"]
            ))
            .collect::<Vec<_>>());
    }
    if v["objectClassName"] == "autnum" {
        m["start_autnum"] = v["startAutnum"].clone();
        m["end_autnum"] = v["endAutnum"].clone();
        m["network_type"] = v["type"].clone();
        m["country"] = v["country"].clone();
    }
    m
}

/// Look up `q` via the IANA bootstrap (fallback rdap.org). Returns
/// (server used, summary, raw JSON).
pub async fn rdap_lookup(ctx: &ToolContext, q: &RdapQuery) -> ToolResult<(String, Value, Value)> {
    let mut notes = vec![];
    let base = match bootstrap(ctx, q.kind()).await {
        Ok(b) => match find_service(&b, q) {
            Some(u) => u,
            None => {
                notes.push(
                    "no RDAP service listed in the IANA bootstrap; using rdap.org".to_string(),
                );
                RDAP_FALLBACK.to_string()
            }
        },
        Err(e) => {
            notes.push(format!("IANA bootstrap unavailable ({e}); using rdap.org"));
            RDAP_FALLBACK.to_string()
        }
    };
    let url = format!("{base}{}", q.path());
    let f = fetch(
        ctx,
        &url,
        "application/rdap+json, application/json",
        4 * 1024 * 1024,
        Duration::from_secs(20),
    )
    .await?;
    if f.status == 404 {
        return Err(ToolError::failed(format!("not found in RDAP ({url})")));
    }
    let v: Value = serde_json::from_slice(&f.body).map_err(|_| {
        ToolError::failed(format!(
            "RDAP server {url} returned HTTP {} with a non-JSON body",
            f.status
        ))
    })?;
    if f.status >= 400 {
        let msg = v["title"]
            .as_str()
            .or(v["description"].get(0).and_then(|d| d.as_str()))
            .unwrap_or("error");
        return Err(ToolError::failed(format!(
            "RDAP server returned HTTP {}: {msg}",
            f.status
        )));
    }
    let mut s = summarize_rdap(&v);
    s["server"] = json!(f.final_url);
    if !notes.is_empty() {
        s["notes"] = json!(notes);
    }
    Ok((f.final_url, s, v))
}

fn rdap() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.rdap", "RDAP Lookup", Category::Network)
        .describe("Registration data for domains, IP networks and ASNs via RDAP (the modern WHOIS), routed through the IANA bootstrap registry")
        .keywords(&["rdap", "whois", "registrar", "domain expiry", "registration", "asn", "ip owner", "network owner"])
        .param(ParamSpec::text("query").label("Domain, IP, CIDR or ASN").required().primary().placeholder("example.com, 8.8.8.8, AS13335"))
        .permission(Permission::Network)
        .text_output("application/json")
        .timeout(60);
    async_tool(spec, |ctx, p| async move {
        let q = RdapQuery::parse(p.str("query")?)?;
        if let RdapQuery::Domain(d) = &q {
            ctx.net_policy().check_host(d)?;
        }
        let (_, mut summary, raw) = rdap_lookup(&ctx, &q).await?;
        let rows: Vec<Vec<Value>> = summary["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| {
                vec![
                    json!(e["roles"]
                        .as_array()
                        .map(|r| r
                            .iter()
                            .filter_map(|x| x.as_str())
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default()),
                    e["name"].clone(),
                    e["organization"].clone(),
                    e["email"].clone(),
                    e["country"].clone(),
                    e["handle"].clone(),
                ]
            })
            .collect();
        summary["table"] = table(
            &[
                "roles",
                "name",
                "organization",
                "email",
                "country",
                "handle",
            ],
            rows,
        );
        Ok(
            ToolOutput::text_as(serde_json::to_string_pretty(&raw)?, "application/json")
                .with_data(summary),
        )
    })
}

// ---------------------------------------------------------------------------
// WHOIS (TCP 43)

const WHOIS_LIMIT: usize = 256 * 1024;

async fn whois_query(
    ctx: &ToolContext,
    server: &str,
    query: &str,
    timeout: Duration,
) -> ToolResult<String> {
    let addrs = resolve_checked(ctx.net_policy(), server, 43).await?;
    let (mut s, _, _) = connect_any(&addrs, timeout).await?;
    // Verisign thin registries need the "domain" keyword to avoid partial matches.
    let q = if matches!(server, "whois.verisign-grs.com" | "whois.crsnic.net")
        && !query.contains(' ')
        && query.contains('.')
    {
        format!("domain {query}")
    } else {
        query.to_string()
    };
    tokio::time::timeout(timeout, s.write_all(format!("{q}\r\n").as_bytes()))
        .await
        .map_err(|_| ToolError::failed(format!("{server}: write timed out")))??;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 8192];
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(ToolError::failed(format!("{server}: response timed out")));
        }
        let n = tokio::time::timeout(left, s.read(&mut chunk))
            .await
            .map_err(|_| ToolError::failed(format!("{server}: response timed out")))??;
        if n == 0 {
            break;
        }
        if buf.len() + n > WHOIS_LIMIT {
            buf.extend_from_slice(&chunk[..WHOIS_LIMIT - buf.len()]);
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(String::from_utf8_lossy(&buf).replace("\r\n", "\n"))
}

fn referral(text: &str) -> Option<String> {
    for line in text.lines() {
        let l = line.trim();
        let lower = l.to_ascii_lowercase();
        for key in [
            "refer:",
            "whois:",
            "registrar whois server:",
            "referralserver:",
        ] {
            if let Some(v) = lower.strip_prefix(key) {
                let v = v
                    .trim()
                    .trim_start_matches("whois://")
                    .trim_end_matches('/');
                let v = v.split(':').next().unwrap_or(v);
                if !v.is_empty() && v.contains('.') && !v.contains(' ') {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

/// Pull common fields out of WHOIS text.
pub fn parse_whois(text: &str) -> Value {
    let mut m = serde_json::Map::new();
    let keys: &[(&str, &[&str])] = &[
        ("domain", &["domain name", "domain"]),
        (
            "registrar",
            &["registrar", "sponsoring registrar", "registrar name"],
        ),
        (
            "created",
            &[
                "creation date",
                "created",
                "registered on",
                "registration time",
                "created on",
            ],
        ),
        (
            "updated",
            &[
                "updated date",
                "last updated",
                "last-update",
                "changed",
                "last modified",
            ],
        ),
        (
            "expires",
            &[
                "registry expiry date",
                "registrar registration expiration date",
                "expiry date",
                "expiration date",
                "expires",
                "paid-till",
                "expire",
            ],
        ),
        (
            "organization",
            &[
                "registrant organization",
                "org-name",
                "orgname",
                "organisation",
                "org",
                "owner",
            ],
        ),
        ("country", &["registrant country", "country"]),
        ("netrange", &["netrange", "inetnum", "inet6num"]),
        ("cidr", &["cidr", "route"]),
        ("netname", &["netname"]),
    ];
    let mut status = vec![];
    let mut ns = vec![];
    for line in text.lines() {
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim();
        if v.is_empty() || line.starts_with('%') || line.starts_with('#') {
            continue;
        }
        if k == "domain status" || k == "status" || k == "state" {
            status.push(v.split_whitespace().next().unwrap_or(v).to_string());
            continue;
        }
        if k == "name server" || k == "nserver" || k == "nameservers" {
            let n = v
                .split_whitespace()
                .next()
                .unwrap_or(v)
                .to_ascii_lowercase();
            if !ns.contains(&n) {
                ns.push(n);
            }
            continue;
        }
        for (field, names) in keys {
            if names.contains(&k.as_str()) && !m.contains_key(*field) {
                m.insert(field.to_string(), json!(v));
            }
        }
    }
    status.dedup();
    m.insert("status".into(), json!(status));
    m.insert("name_servers".into(), json!(ns));
    Value::Object(m)
}

fn whois() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.whois", "WHOIS Lookup", Category::Network)
        .describe("Classic WHOIS over TCP port 43, following referrals from whois.iana.org to the registry and registrar servers")
        .keywords(&["whois", "domain", "registrar", "expiry", "owner", "ip whois", "asn"])
        .param(ParamSpec::text("query").label("Domain, IP or ASN").required().primary().placeholder("example.com"))
        .param(ParamSpec::text("server").label("WHOIS server (skip IANA referral)").advanced())
        .param(ParamSpec::boolean("follow_referrals").label("Follow registrar referrals").default_bool(true))
        .permission(Permission::Network)
        .text_output("text/plain")
        .timeout(60);
    async_tool(spec, |ctx, p| async move {
        let raw = p.str("query")?.trim();
        if raw.len() > 255 || raw.chars().any(|c| c.is_control()) {
            return Err(ToolError::invalid("query", "invalid query"));
        }
        let query = match RdapQuery::parse(raw)? {
            RdapQuery::Domain(d) => {
                ctx.net_policy().check_host(&d)?;
                d
            }
            RdapQuery::Asn(a) => format!("AS{a}"),
            RdapQuery::Ip(n) if n.prefix_len() == n.max_prefix_len() => n.addr().to_string(),
            RdapQuery::Ip(n) => n.to_string(),
        };
        let timeout = Duration::from_secs(10);
        let mut chain = vec![];
        let mut server = match p.opt_str("server") {
            Some(s) => crate::util::clean_host("server", s)?,
            None => "whois.iana.org".to_string(),
        };
        let mut responses: Vec<(String, String)> = vec![];
        for _ in 0..3 {
            let text = match whois_query(&ctx, &server, &query, timeout).await {
                Ok(t) => t,
                Err(e) if !responses.is_empty() => {
                    chain.push(json!({ "server": server, "error": e.to_string() }));
                    break;
                }
                Err(e) => return Err(e),
            };
            chain.push(json!({ "server": server, "bytes": text.len() }));
            let next = referral(&text).filter(|n| {
                !n.eq_ignore_ascii_case(&server)
                    && !chain
                        .iter()
                        .any(|c| c["server"].as_str() == Some(n.as_str()))
            });
            responses.push((server.clone(), text));
            match next {
                Some(n)
                    if p.bool("follow_referrals")
                        || responses.len() == 1 && p.opt_str("server").is_none() =>
                {
                    server = n
                }
                _ => break,
            }
        }
        let (last_server, last_text) = responses.last().cloned().unwrap_or_default();
        // Merge parsed fields: the most specific (last) response wins.
        let mut parsed = serde_json::Map::new();
        for (_, t) in &responses {
            if let Value::Object(m) = parse_whois(t) {
                for (k, v) in m {
                    let empty = v.as_array().map(|a| a.is_empty()).unwrap_or(false);
                    if !empty {
                        parsed.insert(k, v);
                    }
                }
            }
        }
        let full = responses
            .iter()
            .map(|(s, t)| format!("# {s}\n{t}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut out = ToolOutput::text(full).with_data(
            json!({ "query": query, "server": last_server, "chain": chain, "parsed": parsed }),
        );
        if last_text.len() >= WHOIS_LIMIT {
            out = out.warn(format!("response truncated at {} KiB", WHOIS_LIMIT / 1024));
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rdap_query_parse() {
        assert_eq!(RdapQuery::parse("AS13335").unwrap(), RdapQuery::Asn(13335));
        assert_eq!(
            RdapQuery::parse("Example.COM").unwrap(),
            RdapQuery::Domain("example.com".into())
        );
        assert!(matches!(
            RdapQuery::parse("8.8.8.8").unwrap(),
            RdapQuery::Ip(_)
        ));
        assert_eq!(
            RdapQuery::parse("10.1.2.3/8").unwrap().path(),
            "ip/10.0.0.0/8"
        );
        assert_eq!(
            RdapQuery::parse("bücher.de").unwrap(),
            RdapQuery::Domain("xn--bcher-kva.de".into())
        );
        assert!(RdapQuery::parse("nodot").is_err());
    }

    #[test]
    fn bootstrap_matching() {
        let dns = json!({"services": [[["com", "net"], ["https://rdap.verisign.com/com/v1/"]], [["uk"], ["http://x/", "https://rdap.nominet.uk/uk/"]], [["co.uk"], ["https://special/"]]]});
        assert_eq!(
            find_service(&dns, &RdapQuery::Domain("a.example.com".into())).unwrap(),
            "https://rdap.verisign.com/com/v1/"
        );
        assert_eq!(
            find_service(&dns, &RdapQuery::Domain("bbc.co.uk".into())).unwrap(),
            "https://special/"
        );
        assert_eq!(
            find_service(&dns, &RdapQuery::Domain("x.uk".into())).unwrap(),
            "https://rdap.nominet.uk/uk/"
        );
        assert!(find_service(&dns, &RdapQuery::Domain("x.org".into())).is_none());
        let v4 = json!({"services": [[["8.0.0.0/8"], ["https://rdap.arin.net/registry"]], [["8.8.0.0/16"], ["https://more/"]]]});
        assert_eq!(
            find_service(&v4, &RdapQuery::parse("8.8.8.8").unwrap()).unwrap(),
            "https://more/"
        );
        let asn =
            json!({"services": [[["1-1876", "13312-18431"], ["https://rdap.arin.net/registry/"]]]});
        assert_eq!(
            find_service(&asn, &RdapQuery::Asn(13335)).unwrap(),
            "https://rdap.arin.net/registry/"
        );
    }

    #[test]
    fn whois_parse() {
        let t = "Domain Name: EXAMPLE.COM\nRegistrar: RESERVED-Internet Assigned Numbers Authority\nCreation Date: 1995-08-14T04:00:00Z\nRegistry Expiry Date: 2026-08-13T04:00:00Z\nDomain Status: clientDeleteProhibited https://icann.org/epp#clientDeleteProhibited\nName Server: A.IANA-SERVERS.NET\nName Server: B.IANA-SERVERS.NET\n";
        let p = parse_whois(t);
        assert_eq!(p["expires"], "2026-08-13T04:00:00Z");
        assert_eq!(p["status"][0], "clientDeleteProhibited");
        assert_eq!(p["name_servers"].as_array().unwrap().len(), 2);
        assert_eq!(
            referral("refer:        whois.verisign-grs.com\n"),
            Some("whois.verisign-grs.com".into())
        );
        assert_eq!(
            referral("   Registrar WHOIS Server: whois.markmonitor.com\n"),
            Some("whois.markmonitor.com".into())
        );
    }
}
