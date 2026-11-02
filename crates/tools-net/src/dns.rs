//! DNS tools via hickory-resolver: record lookup and reverse DNS.

use crate::util::{ms, table};
use dogubako_core::prelude::*;
use hickory_resolver::config::{ResolverConfig, CLOUDFLARE, GOOGLE, QUAD9};
use hickory_resolver::net::runtime::TokioRuntimeProvider;
use hickory_resolver::proto::rr::{RData, RecordType};
use hickory_resolver::TokioResolver;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::{Duration, Instant};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![dns_lookup(), reverse_dns()]
}

pub const RECORD_TYPES: &[&str] = &[
    "A", "AAAA", "CNAME", "MX", "TXT", "NS", "SOA", "CAA", "SRV", "PTR", "HTTPS", "DS", "DNSKEY",
    "TLSA", "NAPTR",
];

pub fn resolver_param() -> ParamSpec {
    ParamSpec::choice(
        "resolver",
        &[
            ("system", "System resolver"),
            ("cloudflare", "Cloudflare 1.1.1.1"),
            ("google", "Google 8.8.8.8"),
            ("quad9", "Quad9 9.9.9.9"),
            ("cloudflare-doh", "Cloudflare (DNS-over-HTTPS)"),
            ("google-doh", "Google (DNS-over-HTTPS)"),
            ("quad9-doh", "Quad9 (DNS-over-HTTPS)"),
        ],
    )
    .default_str("system")
}

/// Build a resolver. When the network policy forbids private addresses the
/// system resolver (which may answer internal names) is replaced by a public
/// one; the returned string names the resolver actually used.
pub fn build_resolver(
    ctx: &ToolContext,
    choice: &str,
) -> ToolResult<(TokioResolver, String, Option<String>)> {
    let mut note = None;
    let choice = if choice == "system" && !ctx.net_policy().allow_private {
        note = Some("system resolver is disabled by the network policy on this instance; used Cloudflare 1.1.1.1 instead".to_string());
        "cloudflare"
    } else {
        choice
    };
    let provider = TokioRuntimeProvider::default();
    let mut builder = match choice {
        "system" => TokioResolver::builder(provider)
            .map_err(|e| ToolError::failed(format!("cannot read system DNS configuration: {e}")))?,
        other => {
            let (group, doh) = match other {
                "cloudflare" => (CLOUDFLARE, false),
                "google" => (GOOGLE, false),
                "quad9" => (QUAD9, false),
                "cloudflare-doh" => (CLOUDFLARE, true),
                "google-doh" => (GOOGLE, true),
                "quad9-doh" => (QUAD9, true),
                o => {
                    return Err(ToolError::invalid(
                        "resolver",
                        format!("unknown resolver '{o}'"),
                    ))
                }
            };
            let cfg = if doh {
                ResolverConfig::https(&group)
            } else {
                ResolverConfig::udp_and_tcp(&group)
            };
            let mut b = TokioResolver::builder_with_config(cfg, provider);
            if doh {
                let mut roots = rustls::RootCertStore::empty();
                roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
                let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
                let mut tls = rustls::ClientConfig::builder_with_provider(provider)
                    .with_safe_default_protocol_versions()
                    .map_err(|e| ToolError::failed(format!("TLS config: {e}")))?
                    .with_root_certificates(roots)
                    .with_no_client_auth();
                tls.alpn_protocols = vec![b"h2".to_vec()];
                b = b.with_tls_config(tls);
            }
            b
        }
    };
    let o = builder.options_mut();
    o.timeout = Duration::from_secs(4);
    o.attempts = 2;
    o.cache_size = 0;
    o.edns0 = true;
    let r = builder
        .build()
        .map_err(|e| ToolError::failed(format!("cannot create resolver: {e}")))?;
    Ok((r, choice.to_string(), note))
}

/// One resolved record.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Rec {
    pub name: String,
    #[serde(rename = "type")]
    pub rtype: String,
    pub ttl: u32,
    pub value: String,
}

/// Query one record type. Ok(empty) means "no records" (NOERROR/NODATA or NXDOMAIN).
pub async fn query(
    r: &TokioResolver,
    name: &str,
    t: RecordType,
) -> Result<(Vec<Rec>, bool), String> {
    match r.lookup(name, t).await {
        Ok(l) => Ok((
            l.answers()
                .iter()
                .map(|rec| Rec {
                    name: rec.name.to_string(),
                    rtype: rec.record_type().to_string(),
                    ttl: rec.ttl,
                    value: rdata_text(&rec.data),
                })
                .collect(),
            false,
        )),
        Err(e) if e.is_nx_domain() => Ok((vec![], true)),
        Err(e) if e.is_no_records_found() => Ok((vec![], false)),
        Err(e) => Err(e.to_string()),
    }
}

fn rdata_text(d: &RData) -> String {
    match d {
        RData::TXT(t) => t
            .txt_data
            .iter()
            .map(|b| String::from_utf8_lossy(b).to_string())
            .collect::<Vec<_>>()
            .join(""),
        RData::CAA(c) => format!(
            "{} {} \"{}\"",
            if c.issuer_critical { 128 } else { 0 },
            c.tag,
            String::from_utf8_lossy(&c.value)
        ),
        other => other.to_string(),
    }
}

/// Validate a DNS name for lookup and apply the policy (internal names).
pub fn check_name(ctx: &ToolContext, param: &str, raw: &str) -> ToolResult<String> {
    let n = crate::util::clean_host(param, raw)?;
    let ascii = idna::domain_to_ascii(&n)
        .map_err(|_| ToolError::invalid(param, format!("'{n}' is not a valid domain name")))?;
    ctx.net_policy().check_host(&ascii)?;
    Ok(ascii)
}

fn dns_lookup() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.dns_lookup", "DNS Lookup", Category::Network)
        .describe("Query DNS records (A, AAAA, CNAME, MX, TXT, NS, SOA, CAA, SRV, PTR, HTTPS…) with TTLs via the system or a public resolver (optionally DNS-over-HTTPS)")
        .keywords(&["dns", "dig", "nslookup", "mx", "txt", "spf", "dmarc", "ns", "soa", "caa", "records", "resolver", "doh"])
        .param(ParamSpec::text("name").label("Domain name or IP").required().primary().placeholder("example.com"))
        .param(
            ParamSpec::multi_choice("types", &RECORD_TYPES.iter().map(|t| (*t, *t)).collect::<Vec<_>>())
                .label("Record types")
                .default_value(json!(["A", "AAAA", "CNAME", "MX", "TXT", "NS", "SOA", "CAA"])),
        )
        .param(resolver_param())
        .permission(Permission::Network)
        .timeout(60);
    async_tool(spec, |ctx, p| async move {
        let raw = p.str("name")?.trim().to_string();
        let mut types = p.list("types");
        let (name, is_ip) = match crate::ip::parse_ip("name", &raw) {
            Ok(ip) if raw.parse::<IpAddr>().is_ok() => {
                if !ctx.net_policy().ip_allowed(ip) {
                    return Err(ToolError::denied(format!(
                        "reverse lookups of non-public address {ip} are blocked by network policy"
                    )));
                }
                (crate::ip::reverse_pointer(ip), true)
            }
            _ => (check_name(&ctx, "name", &raw)?, false),
        };
        if is_ip {
            types = vec!["PTR".into()];
        }
        if types.is_empty() {
            return Err(ToolError::invalid(
                "types",
                "select at least one record type",
            ));
        }
        let (resolver, used, note) = build_resolver(&ctx, p.str_or("resolver", "system"))?;
        let resolver = Arc::new(resolver);
        let started = Instant::now();
        let futs = types.iter().map(|t| {
            let r = resolver.clone();
            let name = name.clone();
            let t = t.clone();
            async move {
                let rt = RecordType::from_str(&t).unwrap_or(RecordType::A);
                let s = Instant::now();
                let res = query(&r, &name, rt).await;
                (t, res, s.elapsed())
            }
        });
        let results = futures::future::join_all(futs).await;
        let mut rows = vec![];
        let mut summary = vec![];
        let mut errors = serde_json::Map::new();
        let mut nxdomain = false;
        let mut by_type = serde_json::Map::new();
        for (t, res, took) in results {
            match res {
                Ok((recs, nx)) => {
                    nxdomain |= nx;
                    summary.push(json!({ "type": t, "count": recs.len(), "ms": ms(took) }));
                    by_type.insert(
                        t.clone(),
                        json!(recs.iter().map(|r| r.value.clone()).collect::<Vec<_>>()),
                    );
                    for r in recs {
                        rows.push(vec![
                            json!(r.name),
                            json!(r.rtype),
                            json!(r.ttl),
                            json!(r.value),
                        ]);
                    }
                }
                Err(e) => {
                    errors.insert(t.clone(), json!(e));
                    summary.push(json!({ "type": t, "error": e, "ms": ms(took) }));
                }
            }
        }
        if rows.is_empty() && errors.len() == types.len() {
            return Err(ToolError::failed(format!(
                "all DNS queries failed via {used}: {}",
                errors
                    .values()
                    .next()
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
            )));
        }
        let text = if rows.is_empty() {
            if nxdomain {
                format!("{name}: NXDOMAIN (domain does not exist)")
            } else {
                format!("{name}: no records of the requested types")
            }
        } else {
            rows.iter()
                .map(|r| {
                    format!(
                        "{}\t{}\t{}\t{}",
                        r[0].as_str().unwrap_or(""),
                        r[2],
                        r[1].as_str().unwrap_or(""),
                        r[3].as_str().unwrap_or("")
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let mut out = ToolOutput::text(text).with_data(json!({
            "query": name,
            "resolver": used,
            "nxdomain": nxdomain,
            "records": by_type,
            "queries": summary,
            "errors": errors,
            "total_ms": ms(started.elapsed()),
            "table": table(&["name", "type", "ttl", "value"], rows),
        }));
        if let Some(n) = note {
            out = out.warn(n);
        }
        Ok(out)
    })
}

fn reverse_dns() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.reverse_dns", "Reverse DNS", Category::Network)
        .describe("Look up PTR host names for IP addresses and check forward-confirmation (FCrDNS)")
        .keywords(&[
            "reverse dns",
            "ptr",
            "rdns",
            "hostname",
            "fcrdns",
            "ip to hostname",
        ])
        .param(
            ParamSpec::textarea("ips")
                .label("IP addresses")
                .describe("One per line (max 50)")
                .required()
                .primary(),
        )
        .param(resolver_param())
        .permission(Permission::Network)
        .timeout(90);
    async_tool(spec, |ctx, p| async move {
        let mut ips = vec![];
        for line in p
            .text("ips")
            .split(|c: char| c == '\n' || c == ',' || c.is_whitespace())
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            ips.push(crate::ip::parse_ip("ips", line)?);
        }
        if ips.is_empty() {
            return Err(ToolError::invalid("ips", "enter at least one IP address"));
        }
        if ips.len() > 50 {
            return Err(ToolError::invalid("ips", "at most 50 addresses per run"));
        }
        for ip in &ips {
            if !ctx.net_policy().ip_allowed(*ip) {
                return Err(ToolError::denied(format!(
                    "reverse lookups of non-public address {ip} are blocked by network policy"
                )));
            }
        }
        let (resolver, used, note) = build_resolver(&ctx, p.str_or("resolver", "system"))?;
        let resolver = Arc::new(resolver);
        use futures::StreamExt;
        let results: Vec<(IpAddr, Value)> = futures::stream::iter(ips.clone())
            .map(|ip| {
                let r = resolver.clone();
                async move {
                    let ptr = crate::ip::reverse_pointer(ip);
                    let v = match query(&r, &ptr, RecordType::PTR).await {
                        Ok((recs, _)) => {
                            let names: Vec<String> = recs.into_iter().filter(|r| r.rtype == "PTR").map(|r| r.value).collect();
                            let mut confirmed = false;
                            for n in &names {
                                let t = if ip.is_ipv4() { RecordType::A } else { RecordType::AAAA };
                                if let Ok((f, _)) = query(&r, n, t).await {
                                    if f.iter().any(|x| x.value.parse::<IpAddr>().ok() == Some(ip)) {
                                        confirmed = true;
                                    }
                                }
                            }
                            json!({ "ptr": names, "forward_confirmed": if names.is_empty() { Value::Null } else { json!(confirmed) } })
                        }
                        Err(e) => json!({ "error": e }),
                    };
                    (ip, v)
                }
            })
            .buffered(8)
            .collect()
            .await;
        let rows: Vec<Vec<Value>> = results
            .iter()
            .map(|(ip, v)| {
                vec![
                    json!(ip.to_string()),
                    json!(v
                        .get("ptr")
                        .and_then(|p| p.as_array())
                        .map(|a| a
                            .iter()
                            .filter_map(|x| x.as_str())
                            .collect::<Vec<_>>()
                            .join(", "))
                        .unwrap_or_default()),
                    v.get("forward_confirmed").cloned().unwrap_or(Value::Null),
                    v.get("error").cloned().unwrap_or(Value::Null),
                ]
            })
            .collect();
        let text = rows
            .iter()
            .map(|r| {
                format!(
                    "{}\t{}",
                    r[0].as_str().unwrap_or(""),
                    r[1].as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("(no PTR)")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let mut out = ToolOutput::text(text).with_data(json!({ "resolver": used, "table": table(&["ip", "ptr", "forward confirmed", "error"], rows) }));
        if let Some(n) = note {
            out = out.warn(n);
        }
        Ok(out)
    })
}
