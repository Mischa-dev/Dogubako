//! Pure URL tools: parse, normalize, edit query parameters.

use crate::util::table;
use dogubako_core::prelude::*;
use url::Url;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![url_parse(), url_normalize(), query_params()]
}

/// Parse a URL, assuming `https://` when the scheme is missing.
fn parse_lenient(param: &str, s: &str) -> ToolResult<(Url, bool)> {
    let s = s.trim();
    if s.is_empty() {
        return Err(ToolError::invalid(param, "is required"));
    }
    match Url::parse(s) {
        Ok(u) if u.has_host() || !s.contains('.') || s.contains("://") => Ok((u, false)),
        // "example.com:8080/x" parses as scheme "example.com" — treat as host.
        Ok(_) | Err(url::ParseError::RelativeUrlWithoutBase) => Url::parse(&format!("https://{s}"))
            .map(|u| (u, true))
            .map_err(|e| ToolError::invalid(param, format!("invalid URL: {e}"))),
        Err(e) => Err(ToolError::invalid(param, format!("invalid URL: {e}"))),
    }
}

fn url_parse() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.url_parse", "URL Parser", Category::Network)
        .describe("Break a URL into scheme, host, port, path, query parameters and fragment, with IDN/punycode forms")
        .keywords(&["url", "uri", "query string", "punycode", "idn", "decode", "parse"])
        .param(ParamSpec::text("url").label("URL").required().primary().placeholder("https://user@example.com:8080/a/b?x=1&y=%C3%A9#top"))
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let (u, assumed) = parse_lenient("url", p.text("url"))?;
        let host = u.host_str().map(|h| h.to_string());
        let host_type = match u.host() {
            Some(url::Host::Domain(_)) => "domain",
            Some(url::Host::Ipv4(_)) => "ipv4",
            Some(url::Host::Ipv6(_)) => "ipv6",
            None => "none",
        };
        let host_unicode = match u.host() {
            Some(url::Host::Domain(d)) => {
                let (s, r) = idna::domain_to_unicode(d);
                if r.is_ok() {
                    Some(s)
                } else {
                    None
                }
            }
            _ => None,
        };
        let params: Vec<(String, String)> = u
            .query_pairs()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let rows: Vec<Vec<Value>> = params
            .iter()
            .enumerate()
            .map(|(i, (k, v))| vec![json!(i + 1), json!(k), json!(v)])
            .collect();
        let segments: Vec<String> = u
            .path_segments()
            .map(|s| s.map(percent_decode).collect())
            .unwrap_or_default();
        let data = json!({
            "href": u.as_str(),
            "scheme": u.scheme(),
            "scheme_assumed": assumed,
            "username": if u.username().is_empty() { Value::Null } else { json!(percent_decode(u.username())) },
            "has_password": u.password().is_some(),
            "host": host,
            "host_type": host_type,
            "host_unicode": host_unicode,
            "port": u.port(),
            "effective_port": u.port_or_known_default(),
            "path": u.path(),
            "path_decoded": percent_decode(u.path()),
            "path_segments": segments,
            "query": u.query(),
            "fragment": u.fragment(),
            "origin": u.origin().ascii_serialization(),
            "query_param_count": params.len(),
            "table": table(&["#", "key", "value (decoded)"], rows),
        });
        let mut text = format!("{}\n", u.as_str());
        for k in [
            "scheme",
            "host",
            "host_unicode",
            "effective_port",
            "path_decoded",
            "query",
            "fragment",
            "origin",
        ] {
            if let Some(v) = data.get(k).filter(|v| !v.is_null()) {
                text.push_str(&format!(
                    "{k}: {}\n",
                    v.as_str()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| v.to_string())
                ));
            }
        }
        Ok(ToolOutput::text(text).with_data(data))
    })
}

pub fn percent_decode(s: &str) -> String {
    url::form_urlencoded::parse(
        format!(
            "x={}",
            s.replace('+', "%2B")
                .replace('&', "%26")
                .replace('=', "%3D")
        )
        .as_bytes(),
    )
    .next()
    .map(|(_, v)| v.to_string())
    .unwrap_or_else(|| s.to_string())
}

/// Tracking parameters removed by `url_normalize`.
pub fn is_tracking_param(k: &str) -> bool {
    let k = k.to_ascii_lowercase();
    k.starts_with("utm_")
        || matches!(
            k.as_str(),
            "fbclid"
                | "gclid"
                | "dclid"
                | "gbraid"
                | "wbraid"
                | "gclsrc"
                | "msclkid"
                | "mc_cid"
                | "mc_eid"
                | "yclid"
                | "igshid"
                | "_hsenc"
                | "_hsmi"
                | "mkt_tok"
                | "twclid"
                | "ttclid"
                | "li_fat_id"
                | "srsltid"
                | "_ga"
                | "_gl"
                | "oly_enc_id"
                | "oly_anon_id"
                | "vero_id"
                | "rb_clickid"
                | "s_cid"
        )
}

/// Decode percent-escapes of unreserved characters and upper-case the rest
/// (RFC 3986 §6.2.2 normalization).
fn normalize_escapes(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && b[i + 1].is_ascii_hexdigit()
            && b[i + 2].is_ascii_hexdigit()
        {
            let v = u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap_or(0);
            let c = v as char;
            if c.is_ascii_alphanumeric() || "-._~".contains(c) {
                out.push(v);
            } else {
                out.push(b'%');
                out.push(b[i + 1].to_ascii_uppercase());
                out.push(b[i + 2].to_ascii_uppercase());
            }
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub struct NormalizeOpts {
    pub remove_tracking: bool,
    pub remove_fragment: bool,
    pub sort_query: bool,
    pub remove_www: bool,
    pub strip_trailing_slash: bool,
    pub remove_empty_params: bool,
}

pub fn normalize(u: &Url, o: &NormalizeOpts) -> (Url, Vec<String>) {
    // Url::parse already lower-cases scheme/host, removes default ports and
    // resolves dot segments; we re-serialize and apply the rest.
    let mut u = u.clone();
    let mut removed = vec![];
    if o.remove_www {
        if let Some(h) = u
            .host_str()
            .and_then(|h| h.strip_prefix("www."))
            .map(|s| s.to_string())
        {
            let _ = u.set_host(Some(&h));
        }
    }
    let path = normalize_escapes(u.path());
    let mut path = if path.is_empty() {
        "/".to_string()
    } else {
        path
    };
    if o.strip_trailing_slash && path.len() > 1 && path.ends_with('/') {
        path = path.trim_end_matches('/').to_string();
        if path.is_empty() {
            path = "/".into();
        }
    }
    u.set_path(&path);
    if let Some(q) = u.query().map(|q| q.to_string()) {
        let mut pairs: Vec<(String, Option<String>)> = vec![];
        for part in q.split('&').filter(|p| !p.is_empty()) {
            let (k, v) = match part.split_once('=') {
                Some((k, v)) => (k.to_string(), Some(v.to_string())),
                None => (part.to_string(), None),
            };
            let dk = percent_decode(&k);
            if o.remove_tracking && is_tracking_param(&dk) {
                removed.push(dk);
                continue;
            }
            if o.remove_empty_params && v.as_deref().map(|v| v.is_empty()).unwrap_or(true) {
                removed.push(dk);
                continue;
            }
            pairs.push((normalize_escapes(&k), v.map(|v| normalize_escapes(&v))));
        }
        if o.sort_query {
            pairs.sort_by(|a, b| a.0.cmp(&b.0)); // stable: keeps order of repeated keys
        }
        if pairs.is_empty() {
            u.set_query(None);
        } else {
            let q: Vec<String> = pairs
                .into_iter()
                .map(|(k, v)| match v {
                    Some(v) => format!("{k}={v}"),
                    None => k,
                })
                .collect();
            u.set_query(Some(&q.join("&")));
        }
    }
    if o.remove_fragment || u.fragment() == Some("") {
        u.set_fragment(None);
    }
    (u, removed)
}

fn url_normalize() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.url_normalize", "URL Normalizer", Category::Network)
        .describe("Canonicalize URLs: lowercase scheme/host, drop default ports, resolve dot segments, sort query, strip tracking parameters")
        .keywords(&["url", "canonical", "clean", "utm", "tracking", "fbclid", "gclid", "normalize"])
        .param(ParamSpec::textarea("urls").label("URL(s)").describe("One URL per line").required().primary())
        .param(ParamSpec::boolean("remove_tracking").label("Remove tracking parameters (utm_*, fbclid, gclid…)").default_bool(true))
        .param(ParamSpec::boolean("sort_query").label("Sort query parameters").default_bool(true))
        .param(ParamSpec::boolean("remove_fragment").label("Remove #fragment"))
        .param(ParamSpec::boolean("remove_www").label("Remove leading www.").advanced())
        .param(ParamSpec::boolean("strip_trailing_slash").label("Strip trailing slash from path").advanced())
        .param(ParamSpec::boolean("remove_empty_params").label("Remove parameters with empty values").advanced())
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let o = NormalizeOpts {
            remove_tracking: p.bool("remove_tracking"),
            remove_fragment: p.bool("remove_fragment"),
            sort_query: p.bool("sort_query"),
            remove_www: p.bool("remove_www"),
            strip_trailing_slash: p.bool("strip_trailing_slash"),
            remove_empty_params: p.bool("remove_empty_params"),
        };
        let mut out = vec![];
        let mut rows = vec![];
        for line in p
            .text("urls")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            match parse_lenient("urls", line) {
                Ok((u, _)) => {
                    let (n, removed) = normalize(&u, &o);
                    rows.push(vec![
                        json!(line),
                        json!(n.as_str()),
                        json!(removed.join(", ")),
                        json!(line != n.as_str()),
                    ]);
                    out.push(n.to_string());
                }
                Err(e) => {
                    rows.push(vec![
                        json!(line),
                        Value::Null,
                        json!(e.to_string()),
                        json!(false),
                    ]);
                    out.push(format!("# invalid: {line}"));
                }
            }
        }
        if out.is_empty() {
            return Err(ToolError::invalid("urls", "no URLs given"));
        }
        Ok(ToolOutput::text(out.join("\n")).with_data(
            json!({ "table": table(&["input", "normalized", "removed / error", "changed"], rows) }),
        ))
    })
}

fn query_params() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.query_params", "Query Parameter Editor", Category::Network)
        .describe("Set, replace or remove URL query parameters and rebuild the URL")
        .keywords(&["url", "query", "parameters", "querystring", "edit", "builder"])
        .param(ParamSpec::text("url").label("URL").required().primary())
        .param(
            ParamSpec::textarea("set")
                .label("Set parameters")
                .describe("JSON object ({\"q\": \"x\", \"tag\": [\"a\",\"b\"], \"drop\": null}) or one key=value per line; replaces existing values")
                .placeholder("page=2\nsort=desc"),
        )
        .param(ParamSpec::text("remove").label("Remove parameters").describe("Comma-separated keys; supports a trailing * wildcard (utm_*)"))
        .param(ParamSpec::boolean("clear").label("Remove all existing parameters first").advanced())
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let (mut u, _) = parse_lenient("url", p.text("url"))?;
        let mut pairs: Vec<(String, String)> = if p.bool("clear") {
            vec![]
        } else {
            u.query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        };
        // removals
        let removes: Vec<String> = p
            .text("remove")
            .split([',', '\n'])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        pairs.retain(|(k, _)| {
            !removes.iter().any(|r| match r.strip_suffix('*') {
                Some(prefix) => k.starts_with(prefix),
                None => k == r,
            })
        });
        // sets
        let set_text = p.text("set").trim();
        let mut sets: Vec<(String, Option<Vec<String>>)> = vec![];
        if set_text.starts_with('{') {
            let v: Value = serde_json::from_str(set_text)
                .map_err(|e| ToolError::invalid("set", format!("invalid JSON: {e}")))?;
            for (k, v) in v.as_object().into_iter().flatten() {
                let vals = match v {
                    Value::Null => None,
                    Value::Array(a) => Some(a.iter().map(scalar).collect()),
                    other => Some(vec![scalar(other)]),
                };
                sets.push((k.clone(), vals));
            }
        } else {
            for line in set_text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                let (k, v) = line
                    .split_once('=')
                    .map(|(k, v)| (k.trim(), v.trim()))
                    .unwrap_or((line, ""));
                if k.is_empty() {
                    return Err(ToolError::invalid(
                        "set",
                        format!("line '{line}' has no key"),
                    ));
                }
                // Repeated keys within `set` accumulate.
                if let Some(e) = sets.iter_mut().find(|(ek, _)| ek == k) {
                    e.1.get_or_insert_with(Vec::new).push(v.to_string());
                } else {
                    sets.push((k.to_string(), Some(vec![v.to_string()])));
                }
            }
        }
        for (k, vals) in sets {
            let pos = pairs.iter().position(|(pk, _)| *pk == k);
            pairs.retain(|(pk, _)| *pk != k);
            if let Some(vals) = vals {
                let at = pos.unwrap_or(pairs.len()).min(pairs.len());
                for (i, v) in vals.into_iter().enumerate() {
                    pairs.insert(at + i, (k.clone(), v));
                }
            }
        }
        if pairs.is_empty() {
            u.set_query(None);
        } else {
            u.query_pairs_mut().clear().extend_pairs(pairs.iter());
        }
        let rows: Vec<Vec<Value>> = pairs
            .iter()
            .map(|(k, v)| vec![json!(k), json!(v)])
            .collect();
        Ok(ToolOutput::text(u.to_string())
            .with_data(json!({ "url": u.as_str(), "table": table(&["key", "value"], rows) })))
    })
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> NormalizeOpts {
        NormalizeOpts {
            remove_tracking: true,
            remove_fragment: false,
            sort_query: true,
            remove_www: false,
            strip_trailing_slash: false,
            remove_empty_params: false,
        }
    }

    #[test]
    fn normalize_basics() {
        let u =
            Url::parse("HTTP://Example.COM:80/a/./b/../c/%7euser?utm_source=x&b=2&a=1&fbclid=z#")
                .unwrap();
        let (n, removed) = normalize(&u, &opts());
        assert_eq!(n.as_str(), "http://example.com/a/c/~user?a=1&b=2");
        assert_eq!(removed, vec!["utm_source", "fbclid"]);
        let u = Url::parse("https://example.com:443/x%2fy?q=%e2%82%ac#frag").unwrap();
        let (n, _) = normalize(
            &u,
            &NormalizeOpts {
                remove_fragment: true,
                ..opts()
            },
        );
        assert_eq!(n.as_str(), "https://example.com/x%2Fy?q=%E2%82%AC");
    }

    #[test]
    fn escapes() {
        assert_eq!(normalize_escapes("%41%2f%7E%"), "A%2F~%");
        assert_eq!(normalize_escapes("abc%4"), "abc%4");
    }
}
