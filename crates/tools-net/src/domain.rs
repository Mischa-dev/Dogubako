//! `network.domain_inspect`: one-shot domain report (DNS, RDAP, TLS, HTTP).

use crate::dns::{build_resolver, check_name, query, resolver_param};
use crate::util::table;
use dogubako_core::prelude::*;
use hickory_resolver::proto::rr::RecordType;
use std::time::Duration;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![domain_inspect()]
}

async fn dns_part(ctx: &ToolContext, domain: &str, resolver: &str) -> Value {
    let (r, used, note) = match build_resolver(ctx, resolver) {
        Ok(x) => x,
        Err(e) => return json!({ "ok": false, "error": e.to_string() }),
    };
    let types = [
        ("A", RecordType::A),
        ("AAAA", RecordType::AAAA),
        ("CNAME", RecordType::CNAME),
        ("MX", RecordType::MX),
        ("NS", RecordType::NS),
        ("TXT", RecordType::TXT),
        ("CAA", RecordType::CAA),
        ("SOA", RecordType::SOA),
    ];
    let futs = types.iter().map(|(n, t)| {
        let r = &r;
        async move { (*n, query(r, domain, *t).await) }
    });
    let res = futures::future::join_all(futs).await;
    let mut records = serde_json::Map::new();
    let mut errors = serde_json::Map::new();
    let mut nx = false;
    for (n, r) in res {
        match r {
            Ok((recs, is_nx)) => {
                nx |= is_nx;
                records.insert(
                    n.into(),
                    json!(recs
                        .iter()
                        .map(|r| json!({ "value": r.value, "ttl": r.ttl }))
                        .collect::<Vec<_>>()),
                );
            }
            Err(e) => {
                errors.insert(n.into(), json!(e));
            }
        }
    }
    let txt: Vec<String> = records
        .get("TXT")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x["value"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let spf = txt
        .iter()
        .find(|t| t.to_ascii_lowercase().starts_with("v=spf1"))
        .cloned();
    // DMARC lives at _dmarc.<domain>
    let dmarc = match query(&r, &format!("_dmarc.{domain}"), RecordType::TXT).await {
        Ok((recs, _)) => recs
            .into_iter()
            .map(|r| r.value)
            .find(|v| v.to_ascii_lowercase().starts_with("v=dmarc1")),
        Err(_) => None,
    };
    json!({ "ok": errors.len() < types.len(), "resolver": used, "note": note, "nxdomain": nx, "records": records, "spf": spf, "dmarc": dmarc, "errors": errors })
}

fn domain_inspect() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.domain_inspect", "Domain Inspector", Category::Network)
        .describe("All-in-one domain report: DNS records (incl. SPF/DMARC), RDAP registration (registrar, dates, status), TLS certificate and HTTP redirect chain")
        .keywords(&["domain", "report", "dns", "whois", "rdap", "ssl", "certificate", "expiry", "registrar", "spf", "dmarc", "website check"])
        .param(ParamSpec::text("domain").label("Domain").required().primary().placeholder("example.com"))
        .param(resolver_param().advanced())
        .permission(Permission::Network)
        .progress()
        .timeout(120);
    async_tool(spec, |ctx, p| async move {
        let mut raw = p.str("domain")?.trim().to_string();
        if raw.contains("://") {
            raw = url::Url::parse(&raw)
                .ok()
                .and_then(|u| u.host_str().map(|s| s.to_string()))
                .ok_or_else(|| ToolError::invalid("domain", "invalid URL"))?;
        }
        let domain = check_name(&ctx, "domain", &raw)?.to_ascii_lowercase();
        let resolver = p.str_or("resolver", "system").to_string();
        ctx.progress(0.05, "querying DNS, RDAP, TLS and HTTP");
        let part_timeout = Duration::from_secs(30);
        let to = |v: Result<Value, tokio::time::error::Elapsed>| {
            v.unwrap_or_else(|_| json!({ "ok": false, "error": "timed out" }))
        };
        let dns_f = tokio::time::timeout(part_timeout, dns_part(&ctx, &domain, &resolver));
        let rdap_f = tokio::time::timeout(part_timeout, async {
            let q = crate::registry::RdapQuery::Domain(domain.clone());
            match crate::registry::rdap_lookup(&ctx, &q).await {
                Ok((_, s, _)) => {
                    let mut s = s;
                    s["ok"] = json!(true);
                    s
                }
                Err(e) => json!({ "ok": false, "error": e.to_string() }),
            }
        });
        let tls_f = tokio::time::timeout(part_timeout, async {
            match crate::tls::tls_probe(&ctx, &domain, 443, None, Duration::from_secs(10)).await {
                Ok(d) => {
                    let c = &d["certificate"];
                    json!({
                        "ok": true,
                        "protocol": d["protocol"], "cipher_suite": d["cipher_suite"], "alpn": d["alpn"],
                        "trusted": d["trust"]["trusted"], "trust_error": d["trust"]["error"],
                        "subject": c["subject"], "issuer": c["issuer"], "not_after": c["not_after"], "days_remaining": c["days_remaining"],
                        "subject_alt_names": c["subject_alt_names"], "key": format!("{} {}", c["public_key"]["algorithm"].as_str().unwrap_or(""), c["public_key"]["bits"]),
                        "chain_length": d["chain"].as_array().map(|a| a.len()),
                    })
                }
                Err(e) => json!({ "ok": false, "error": e.to_string() }),
            }
        });
        let http_f = tokio::time::timeout(part_timeout, async {
            let start = format!("http://{domain}/");
            match crate::http::http_chain(&ctx, &start, "GET", 10, Duration::from_secs(10)).await {
                Ok(c) => {
                    let (security, _, score) =
                        crate::http::analyze_security(&c.headers, &c.final_url, &start);
                    let missing: Vec<Value> = security
                        .iter()
                        .filter(|s| s["status"] == "missing")
                        .map(|s| s["header"].clone())
                        .collect();
                    json!({
                        "ok": c.error.is_none(),
                        "final_url": c.final_url,
                        "final_status": c.hops.last().map(|h| h.status),
                        "redirects": c.hops,
                        "https_redirect": c.final_url.starts_with("https://"),
                        "security_score": score,
                        "missing_security_headers": missing,
                        "error": c.error,
                    })
                }
                Err(e) => json!({ "ok": false, "error": e.to_string() }),
            }
        });
        let (dns, rdap, tls, http) = tokio::join!(dns_f, rdap_f, tls_f, http_f);
        let (dns, rdap, tls, http) = (to(dns), to(rdap), to(tls), to(http));
        ctx.progress(1.0, "done");

        let mut rows: Vec<Vec<Value>> = vec![];
        let mut row =
            |section: &str, item: &str, v: Value| rows.push(vec![json!(section), json!(item), v]);
        if dns["ok"] == true {
            for t in ["A", "AAAA", "MX", "NS"] {
                let vals: Vec<String> = dns["records"][t]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x["value"].as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                row("DNS", t, json!(vals.join(", ")));
            }
            row("DNS", "SPF", dns["spf"].clone());
            row("DNS", "DMARC", dns["dmarc"].clone());
        } else {
            row("DNS", "error", dns["error"].clone());
        }
        if rdap["ok"] == true {
            row("Registration", "registrar", rdap["registrar"].clone());
            row(
                "Registration",
                "created",
                rdap["events"]["registration"].clone(),
            );
            row(
                "Registration",
                "expires",
                rdap["events"]["expiration"].clone(),
            );
            row(
                "Registration",
                "status",
                json!(rdap["status"].as_array().map(|a| a
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(", "))),
            );
        } else {
            row("Registration", "error", rdap["error"].clone());
        }
        if tls["ok"] == true {
            row(
                "TLS",
                "certificate",
                json!(format!(
                    "{} (issuer {})",
                    tls["subject"].as_str().unwrap_or(""),
                    tls["issuer"].as_str().unwrap_or("")
                )),
            );
            row(
                "TLS",
                "expires",
                json!(format!(
                    "{} ({} days)",
                    tls["not_after"].as_str().unwrap_or(""),
                    tls["days_remaining"]
                )),
            );
            row("TLS", "trusted", tls["trusted"].clone());
            row(
                "TLS",
                "protocol",
                json!(format!(
                    "{} {}",
                    tls["protocol"].as_str().unwrap_or(""),
                    tls["cipher_suite"].as_str().unwrap_or("")
                )),
            );
        } else {
            row("TLS", "error", tls["error"].clone());
        }
        if http.get("final_url").is_some() {
            row(
                "HTTP",
                "final URL",
                json!(format!(
                    "{} ({})",
                    http["final_url"].as_str().unwrap_or(""),
                    http["final_status"]
                )),
            );
            row(
                "HTTP",
                "redirects",
                json!(http["redirects"]
                    .as_array()
                    .map(|a| a.len().saturating_sub(1))),
            );
            row("HTTP", "HTTPS redirect", http["https_redirect"].clone());
            row(
                "HTTP",
                "missing security headers",
                json!(http["missing_security_headers"].as_array().map(|a| a
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(", "))),
            );
        } else {
            row("HTTP", "error", http["error"].clone());
        }
        let ok_parts = [&dns, &rdap, &tls, &http]
            .iter()
            .filter(|v| v["ok"] == true)
            .count();
        Ok(
            ToolOutput::text(format!("{domain}: {ok_parts}/4 checks completed")).with_data(json!({
                "domain": domain,
                "dns": dns,
                "registration": rdap,
                "tls": tls,
                "http": http,
                "table": table(&["section", "item", "value"], rows),
            })),
        )
    })
}
