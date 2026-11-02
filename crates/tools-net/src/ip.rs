//! Pure IP address tools: address info, CIDR calculator, VLSM planner.

use crate::util::table;
use dogubako_core::policy::is_public_ip;
use dogubako_core::prelude::*;
use ipnet::{IpNet, Ipv4Net, Ipv6Net};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![ip_info(), cidr(), subnet_plan()]
}

/// Parse an IP address, tolerating brackets, zone ids and plain integers.
pub fn parse_ip(param: &str, s: &str) -> ToolResult<IpAddr> {
    let t = s.trim().trim_matches(['[', ']']);
    let t = t.split('%').next().unwrap_or(t);
    if let Ok(ip) = t.parse::<IpAddr>() {
        return Ok(ip);
    }
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        if let Ok(n) = u128::from_str_radix(h, 16) {
            return Ok(int_to_ip(n));
        }
    }
    if let Ok(n) = t.parse::<u128>() {
        return Ok(int_to_ip(n));
    }
    Err(ToolError::invalid(
        param,
        format!("'{}' is not an IPv4 or IPv6 address", s.trim()),
    ))
}

fn int_to_ip(n: u128) -> IpAddr {
    if n <= u32::MAX as u128 {
        IpAddr::V4(Ipv4Addr::from(n as u32))
    } else {
        IpAddr::V6(Ipv6Addr::from(n))
    }
}

/// Human classification tags for an address.
pub fn classify(ip: IpAddr) -> Vec<&'static str> {
    let mut t = vec![];
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            if v4.is_unspecified() {
                t.push("unspecified");
            }
            if v4.is_loopback() {
                t.push("loopback");
            }
            if v4.is_private() {
                t.push("private (RFC 1918)");
            }
            if v4.is_link_local() {
                t.push("link-local");
            }
            if o[0] == 100 && (o[1] & 0xc0) == 64 {
                t.push("shared address space / CGNAT (RFC 6598)");
            }
            if v4.is_multicast() {
                t.push("multicast");
            }
            if v4.is_broadcast() {
                t.push("broadcast");
            }
            if v4.is_documentation() {
                t.push("documentation (RFC 5737)");
            }
            if o[0] == 198 && (o[1] & 0xfe) == 18 {
                t.push("benchmarking (RFC 2544)");
            }
            if o[0] == 192 && o[1] == 0 && o[2] == 0 {
                t.push("IETF protocol assignments");
            }
            if o[0] >= 240 && !v4.is_broadcast() {
                t.push("reserved (class E)");
            }
            if o[0] == 0 && !v4.is_unspecified() {
                t.push("\"this network\"");
            }
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            if v6.is_unspecified() {
                t.push("unspecified");
            }
            if v6.is_loopback() {
                t.push("loopback");
            }
            if v6.to_ipv4_mapped().is_some() {
                t.push("IPv4-mapped");
            }
            if (s[0] & 0xfe00) == 0xfc00 {
                t.push("unique local (fc00::/7)");
            }
            if (s[0] & 0xffc0) == 0xfe80 {
                t.push("link-local");
            }
            if v6.is_multicast() {
                t.push("multicast");
            }
            if s[0] == 0x2001 && s[1] == 0x0db8 {
                t.push("documentation (2001:db8::/32)");
            }
            if s[0] == 0x2002 {
                t.push("6to4");
            }
            if s[0] == 0x2001 && s[1] == 0 {
                t.push("Teredo");
            }
            if s[0] == 0x64 && s[1] == 0xff9b {
                t.push("NAT64");
            }
        }
    }
    if is_public_ip(ip) {
        t.push("global (publicly routable)");
    }
    t
}

pub fn reverse_pointer(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            format!("{}.{}.{}.{}.in-addr.arpa", o[3], o[2], o[1], o[0])
        }
        IpAddr::V6(v6) => {
            let hex: String = v6.octets().iter().map(|b| format!("{b:02x}")).collect();
            let mut parts: Vec<String> = hex.chars().rev().map(|c| c.to_string()).collect();
            parts.push("ip6.arpa".into());
            parts.join(".")
        }
    }
}

fn ip_info() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.ip_info", "IP Address Info", Category::Network)
        .describe("Classify an IPv4/IPv6 address (private, loopback, CGNAT, global…) and show its binary, hex, integer and reverse-DNS forms")
        .keywords(&["ip", "ipv4", "ipv6", "address", "private", "binary", "integer", "ptr", "arpa"])
        .param(ParamSpec::text("ip").label("IP address").describe("IPv4, IPv6 or an integer").required().primary().placeholder("192.168.1.10"))
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let ip = parse_ip("ip", p.text("ip"))?;
        let tags = classify(ip);
        let data = match ip {
            IpAddr::V4(v4) => {
                let n = u32::from(v4);
                let class = match v4.octets()[0] {
                    0..=127 => "A",
                    128..=191 => "B",
                    192..=223 => "C",
                    224..=239 => "D (multicast)",
                    _ => "E (reserved)",
                };
                json!({
                    "address": v4.to_string(),
                    "version": 4,
                    "classification": tags,
                    "is_public": is_public_ip(ip),
                    "integer": n,
                    "hex": format!("0x{n:08X}"),
                    "binary": v4.octets().iter().map(|o| format!("{o:08b}")).collect::<Vec<_>>().join("."),
                    "octal": v4.octets().iter().map(|o| format!("{o:04o}")).collect::<Vec<_>>().join("."),
                    "legacy_class": class,
                    "ipv4_mapped_ipv6": v4.to_ipv6_mapped().to_string(),
                    "six_to_four_prefix": format!("2002:{:02x}{:02x}:{:02x}{:02x}::/48", v4.octets()[0], v4.octets()[1], v4.octets()[2], v4.octets()[3]),
                    "reverse_pointer": reverse_pointer(ip),
                })
            }
            IpAddr::V6(v6) => {
                let n = u128::from(v6);
                let expanded = v6
                    .segments()
                    .iter()
                    .map(|s| format!("{s:04x}"))
                    .collect::<Vec<_>>()
                    .join(":");
                let embedded_v4 = v6.to_ipv4_mapped().map(|v| v.to_string()).or_else(|| {
                    let s = v6.segments();
                    if s[0] == 0x2002 {
                        Some(
                            Ipv4Addr::new(
                                (s[1] >> 8) as u8,
                                s[1] as u8,
                                (s[2] >> 8) as u8,
                                s[2] as u8,
                            )
                            .to_string(),
                        )
                    } else if s[0] == 0x64 && s[1] == 0xff9b {
                        Some(
                            Ipv4Addr::new(
                                (s[6] >> 8) as u8,
                                s[6] as u8,
                                (s[7] >> 8) as u8,
                                s[7] as u8,
                            )
                            .to_string(),
                        )
                    } else {
                        None
                    }
                });
                json!({
                    "address": v6.to_string(),
                    "expanded": expanded,
                    "version": 6,
                    "classification": tags,
                    "is_public": is_public_ip(ip),
                    "integer": n.to_string(),
                    "hex": format!("0x{n:032X}"),
                    "binary": v6.segments().iter().map(|s| format!("{s:016b}")).collect::<Vec<_>>().join(":"),
                    "embedded_ipv4": embedded_v4,
                    "network_prefix_64": Ipv6Net::new(v6, 64).map(|n| n.trunc().to_string()).ok(),
                    "interface_id": format!("{:04x}:{:04x}:{:04x}:{:04x}", v6.segments()[4], v6.segments()[5], v6.segments()[6], v6.segments()[7]),
                    "reverse_pointer": reverse_pointer(ip),
                })
            }
        };
        let text = format!("{} — {}", ip, tags.join(", "));
        Ok(ToolOutput::text(text).with_data(data))
    })
}

/// Parse "a.b.c.d/nn", "a.b.c.d 255.255.255.0", "addr" (host route) or v6.
pub fn parse_net(param: &str, s: &str) -> ToolResult<(IpNet, bool)> {
    let t = s.trim();
    if let Some((a, m)) = t.split_once(|c: char| c.is_whitespace()) {
        let a: Ipv4Addr = a
            .trim()
            .parse()
            .map_err(|_| ToolError::invalid(param, format!("'{t}' is not a valid network")))?;
        let m: Ipv4Addr = m
            .trim()
            .parse()
            .map_err(|_| ToolError::invalid(param, format!("'{t}': invalid netmask")))?;
        let net = Ipv4Net::with_netmask(a, m).map_err(|_| {
            ToolError::invalid(param, format!("'{}' is not a contiguous netmask", m))
        })?;
        let host_bits = net.trunc() != net;
        return Ok((IpNet::V4(net.trunc()), host_bits));
    }
    let net: IpNet = if t.contains('/') {
        t.parse()
            .map_err(|_| ToolError::invalid(param, format!("'{t}' is not valid CIDR notation")))?
    } else {
        IpNet::from(parse_ip(param, t)?)
    };
    let host_bits = net.trunc() != net;
    Ok((net.trunc(), host_bits))
}

fn addr_count(net: &IpNet) -> u128 {
    let bits = (net.max_prefix_len() - net.prefix_len()) as u32;
    if bits >= 128 {
        u128::MAX
    } else {
        1u128 << bits
    }
}

fn count_str(net: &IpNet) -> String {
    let bits = (net.max_prefix_len() - net.prefix_len()) as u32;
    if bits >= 128 {
        "340282366920938463463374607431768211456".into()
    } else {
        (1u128 << bits).to_string()
    }
}

pub fn net_info(net: &IpNet) -> Value {
    match net {
        IpNet::V4(n) => {
            let total = 1u64 << (32 - n.prefix_len());
            let (first, last, usable) = match n.prefix_len() {
                32 => (n.network(), n.network(), 1),
                31 => (n.network(), n.broadcast(), 2),
                _ => (
                    Ipv4Addr::from(u32::from(n.network()) + 1),
                    Ipv4Addr::from(u32::from(n.broadcast()) - 1),
                    total - 2,
                ),
            };
            json!({
                "cidr": n.to_string(),
                "version": 4,
                "network": n.network().to_string(),
                "broadcast": n.broadcast().to_string(),
                "netmask": n.netmask().to_string(),
                "wildcard": n.hostmask().to_string(),
                "prefix_length": n.prefix_len(),
                "first_host": first.to_string(),
                "last_host": last.to_string(),
                "total_addresses": total,
                "usable_hosts": usable,
                "netmask_binary": n.netmask().octets().iter().map(|o| format!("{o:08b}")).collect::<Vec<_>>().join("."),
                "classification": classify(IpAddr::V4(n.network())),
            })
        }
        IpNet::V6(n) => {
            json!({
                "cidr": n.to_string(),
                "version": 6,
                "network": n.network().to_string(),
                "last_address": n.broadcast().to_string(),
                "netmask": n.netmask().to_string(),
                "prefix_length": n.prefix_len(),
                "first_host": n.network().to_string(),
                "last_host": n.broadcast().to_string(),
                "total_addresses": count_str(net),
                "subnets_64": if n.prefix_len() <= 64 { json!(if 64 - n.prefix_len() >= 64 { "18446744073709551616".to_string() } else { (1u128 << (64 - n.prefix_len())).to_string() }) } else { Value::Null },
                "classification": classify(IpAddr::V6(n.network())),
            })
        }
    }
}

const MAX_SPLIT_LIST: usize = 4096;

fn cidr() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.cidr", "CIDR Calculator", Category::Network)
        .describe("Subnet calculator for IPv4/IPv6: network, broadcast, netmask, host range; contains-check, split into subnets, aggregate CIDR lists")
        .keywords(&["cidr", "subnet", "netmask", "calculator", "ipv4", "ipv6", "supernet", "aggregate", "summarize", "wildcard"])
        .param(ParamSpec::textarea("cidr").label("CIDR block(s)").describe("One per line, e.g. 10.0.0.0/22, 2001:db8::/48 or 192.168.1.0 255.255.255.0").required().primary())
        .param(ParamSpec::choice("action", &[("info", "Subnet details"), ("contains", "Does it contain an address?"), ("split", "Split into subnets"), ("summarize", "Summarize / aggregate list")]).default_str("info"))
        .param(ParamSpec::text("address").label("Address or CIDR to test").show_if("action", &["contains"]))
        .param(ParamSpec::integer("new_prefix").label("New prefix length").range(0.0, 128.0).default_int(26).show_if("action", &["split"]))
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let mut nets = vec![];
        let mut notes = vec![];
        for line in p
            .text("cidr")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
        {
            let (n, host_bits) = parse_net("cidr", line)?;
            if host_bits {
                notes.push(format!("'{line}' has host bits set; using network {n}"));
            }
            nets.push(n);
        }
        if nets.is_empty() {
            return Err(ToolError::invalid("cidr", "enter at least one CIDR block"));
        }
        let mut out = match p.str_or("action", "info") {
            "info" => {
                if nets.len() == 1 {
                    let d = net_info(&nets[0]);
                    let mut text = String::new();
                    for (k, v) in d.as_object().unwrap() {
                        if k != "classification" {
                            text.push_str(&format!(
                                "{k}: {}\n",
                                v.as_str()
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| v.to_string())
                            ));
                        }
                    }
                    ToolOutput::text(text).with_data(d)
                } else {
                    let rows: Vec<Vec<Value>> = nets
                        .iter()
                        .map(|n| {
                            let d = net_info(n);
                            vec![
                                d["cidr"].clone(),
                                d["network"].clone(),
                                d.get("broadcast")
                                    .cloned()
                                    .unwrap_or(d["last_address"].clone()),
                                d["netmask"].clone(),
                                d["first_host"].clone(),
                                d["last_host"].clone(),
                                json!(d["total_addresses"].to_string().trim_matches('"')),
                            ]
                        })
                        .collect();
                    ToolOutput::data(
                        json!({ "table": table(&["cidr", "network", "broadcast / last", "netmask", "first host", "last host", "addresses"], rows) }),
                    )
                }
            }
            "contains" => {
                let a = p.str("address")?;
                let (target, _) = parse_net("address", a)?;
                let rows: Vec<Vec<Value>> = nets
                    .iter()
                    .map(|n| vec![json!(n.to_string()), json!(n.contains(&target))])
                    .collect();
                let any = nets.iter().any(|n| n.contains(&target));
                ToolOutput::text(format!("{a} is {}in {}", if any { "" } else { "NOT " }, nets.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ")))
                    .with_data(json!({ "address": a, "contained": any, "table": table(&["network", "contains"], rows) }))
            }
            "split" => {
                let np = p.int("new_prefix")? as u8;
                let n = nets[0];
                if np < n.prefix_len() || np > n.max_prefix_len() {
                    return Err(ToolError::invalid(
                        "new_prefix",
                        format!(
                            "must be between {} and {}",
                            n.prefix_len(),
                            n.max_prefix_len()
                        ),
                    ));
                }
                let bits = (np - n.prefix_len()) as u32;
                let count: u128 = if bits >= 128 {
                    u128::MAX
                } else {
                    1u128 << bits
                };
                let subnets = n
                    .subnets(np)
                    .map_err(|e| ToolError::invalid("new_prefix", e.to_string()))?;
                let rows: Vec<Vec<Value>> = subnets
                    .take(MAX_SPLIT_LIST)
                    .enumerate()
                    .map(|(i, s)| {
                        let d = net_info(&s);
                        vec![
                            json!(i + 1),
                            d["cidr"].clone(),
                            d["first_host"].clone(),
                            d["last_host"].clone(),
                            d.get("broadcast").cloned().unwrap_or(Value::Null),
                        ]
                    })
                    .collect();
                let mut o = ToolOutput::data(json!({
                    "subnet_count": count.to_string(),
                    "addresses_per_subnet": count_str(&IpNet::new(n.network(), np).unwrap()),
                    "table": table(&["#", "subnet", "first host", "last host", "broadcast"], rows),
                }));
                if count > MAX_SPLIT_LIST as u128 {
                    o = o.warn(format!(
                        "{count} subnets; only the first {MAX_SPLIT_LIST} are listed"
                    ));
                }
                o
            }
            "summarize" => {
                let agg = IpNet::aggregate(&nets);
                let total: u128 = agg
                    .iter()
                    .map(addr_count)
                    .fold(0u128, |a, b| a.saturating_add(b));
                let rows: Vec<Vec<Value>> = agg
                    .iter()
                    .map(|n| vec![json!(n.to_string()), json!(count_str(n))])
                    .collect();
                ToolOutput::text(agg.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("\n"))
                    .with_data(json!({ "input_count": nets.len(), "output_count": agg.len(), "total_addresses": total.to_string(), "table": table(&["cidr", "addresses"], rows) }))
            }
            other => {
                return Err(ToolError::invalid(
                    "action",
                    format!("unknown action {other}"),
                ))
            }
        };
        for n in notes {
            out = out.warn(n);
        }
        Ok(out)
    })
}

struct Req {
    name: String,
    hosts: u128,
}

fn parse_requirements(text: &str) -> ToolResult<Vec<Req>> {
    let t = text.trim();
    let mut out = vec![];
    if t.starts_with('[') || t.starts_with('{') {
        let v: Value = serde_json::from_str(t)
            .map_err(|e| ToolError::invalid("requirements", format!("invalid JSON: {e}")))?;
        match v {
            Value::Object(m) => {
                for (k, v) in m {
                    let h = v.as_u64().ok_or_else(|| {
                        ToolError::invalid("requirements", format!("'{k}' needs a host count"))
                    })?;
                    out.push(Req {
                        name: k,
                        hosts: h as u128,
                    });
                }
            }
            Value::Array(a) => {
                for (i, item) in a.iter().enumerate() {
                    let (name, h) = match item {
                        Value::Number(n) => (format!("subnet-{}", i + 1), n.as_u64()),
                        Value::Object(o) => (
                            o.get("name")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| format!("subnet-{}", i + 1)),
                            o.get("hosts").and_then(|v| v.as_u64()),
                        ),
                        _ => (String::new(), None),
                    };
                    let h = h.ok_or_else(|| {
                        ToolError::invalid(
                            "requirements",
                            format!("entry {} needs a host count", i + 1),
                        )
                    })?;
                    out.push(Req {
                        name,
                        hosts: h as u128,
                    });
                }
            }
            _ => {
                return Err(ToolError::invalid(
                    "requirements",
                    "expected a JSON array or object",
                ))
            }
        }
    } else {
        for (i, line) in t
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .enumerate()
        {
            let (name, num) = match line.rsplit_once([':', '=', ',', ' ', '\t']) {
                Some((n, h)) => (
                    n.trim()
                        .trim_end_matches([':', '=', ','])
                        .trim()
                        .to_string(),
                    h.trim(),
                ),
                None => (format!("subnet-{}", i + 1), line),
            };
            let hosts: u128 = num.parse().map_err(|_| {
                ToolError::invalid(
                    "requirements",
                    format!("line '{line}': '{num}' is not a host count"),
                )
            })?;
            out.push(Req {
                name: if name.is_empty() {
                    format!("subnet-{}", i + 1)
                } else {
                    name
                },
                hosts,
            });
        }
    }
    if out.is_empty() {
        return Err(ToolError::invalid(
            "requirements",
            "list at least one subnet",
        ));
    }
    if out.iter().any(|r| r.hosts == 0) {
        return Err(ToolError::invalid(
            "requirements",
            "host counts must be at least 1",
        ));
    }
    Ok(out)
}

fn subnet_plan() -> Arc<dyn Tool> {
    let spec = ToolSpec::new(
        "network.subnet_plan",
        "Subnet Planner (VLSM)",
        Category::Network,
    )
    .describe(
        "Allocate variable-length subnets from a parent block for a list of required host counts",
    )
    .keywords(&[
        "vlsm",
        "subnet",
        "planner",
        "allocation",
        "cidr",
        "ip plan",
        "addressing",
    ])
    .param(
        ParamSpec::text("parent")
            .label("Parent block")
            .required()
            .primary()
            .placeholder("10.0.0.0/22"),
    )
    .param(
        ParamSpec::textarea("requirements")
            .label("Required subnets")
            .describe("One per line as 'name: hosts' (or JSON [{\"name\":..,\"hosts\":..}])")
            .required()
            .placeholder("Office: 200\nLab: 50\nVoIP: 25\nLink: 2"),
    )
    .ui("live");
    sync_tool(spec, |_ctx, p| {
        let (parent, _) = parse_net("parent", p.text("parent"))?;
        let mut reqs = parse_requirements(p.text("requirements"))?;
        let max_bits = parent.max_prefix_len() as u32;
        let is_v4 = matches!(parent, IpNet::V4(_));
        // Stable sort by size, largest first: sequential allocation stays aligned.
        let mut order: Vec<usize> = (0..reqs.len()).collect();
        order.sort_by(|a, b| reqs[*b].hosts.cmp(&reqs[*a].hosts));
        if max_bits - parent.prefix_len() as u32 >= 127 {
            return Err(ToolError::invalid(
                "parent",
                "parent block is too large to plan (use a prefix of /2 or longer for IPv6)",
            ));
        }
        let base = match parent.network() {
            IpAddr::V4(a) => u32::from(a) as u128,
            IpAddr::V6(a) => u128::from(a),
        };
        let parent_size = addr_count(&parent);
        let mut cursor: u128 = 0;
        let mut rows = vec![];
        let mut allocations = vec![];
        let mut failed = vec![];
        for &i in &order {
            let r = &reqs[i];
            // IPv4 subnets need network + broadcast addresses (except /31,/32 links).
            let needed = if is_v4 {
                match r.hosts {
                    1 => 1,
                    2 => 2,
                    h => h + 2,
                }
            } else {
                r.hosts
            };
            let bits = if needed <= 1 {
                0
            } else {
                128 - (needed - 1).leading_zeros()
            };
            if bits > max_bits - parent.prefix_len() as u32
                || cursor + (1u128 << bits) > parent_size
            {
                failed.push(r.name.clone());
                continue;
            }
            let size = 1u128 << bits;
            let start = base + cursor;
            let prefix = (max_bits - bits) as u8;
            let addr = if is_v4 {
                IpAddr::V4(Ipv4Addr::from(start as u32))
            } else {
                IpAddr::V6(Ipv6Addr::from(start))
            };
            let net = IpNet::new(addr, prefix).unwrap();
            let d = net_info(&net);
            let usable: u128 = if is_v4 {
                d["usable_hosts"].as_u64().unwrap_or(0) as u128
            } else {
                size
            };
            rows.push(vec![
                json!(r.name),
                json!(r.hosts.to_string()),
                json!(net.to_string()),
                d["first_host"].clone(),
                d["last_host"].clone(),
                d.get("broadcast").cloned().unwrap_or(Value::Null),
                d["netmask"].clone(),
                json!(usable.to_string()),
                json!((usable.saturating_sub(r.hosts)).to_string()),
            ]);
            allocations.push(
                json!({ "name": r.name, "hosts": r.hosts.to_string(), "subnet": net.to_string() }),
            );
            cursor += size;
        }
        // Free space left after the allocations, as aggregated CIDRs.
        let mut free = vec![];
        if cursor < parent_size {
            let mut c = cursor;
            while c < parent_size {
                let mut bits = c
                    .trailing_zeros()
                    .min(max_bits - parent.prefix_len() as u32);
                while bits > 0 && c + (1u128 << bits) > parent_size {
                    bits -= 1;
                }
                let addr = if is_v4 {
                    IpAddr::V4(Ipv4Addr::from((base + c) as u32))
                } else {
                    IpAddr::V6(Ipv6Addr::from(base + c))
                };
                let prefix = (max_bits - bits) as u8;
                if let Ok(n) = IpNet::new(addr, prefix.max(parent.prefix_len())) {
                    free.push(n.to_string());
                }
                c += 1u128 << bits;
                if free.len() > 64 {
                    break;
                }
            }
        }
        reqs.clear();
        let mut out = ToolOutput::data(json!({
            "parent": parent.to_string(),
            "allocated": allocations,
            "unallocated": free,
            "utilization_percent": (cursor as f64 / parent_size as f64 * 1000.0).round() / 10.0,
            "table": table(&["name", "hosts needed", "subnet", "first host", "last host", "broadcast", "netmask", "usable", "spare"], rows),
        }));
        if !failed.is_empty() {
            out = out.warn(format!(
                "not enough space in {parent} for: {}",
                failed.join(", ")
            ));
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_v4() {
        let (n, hb) = parse_net("c", "192.168.1.77/26").unwrap();
        assert!(hb);
        let d = net_info(&n);
        assert_eq!(d["network"], "192.168.1.64");
        assert_eq!(d["broadcast"], "192.168.1.127");
        assert_eq!(d["netmask"], "255.255.255.192");
        assert_eq!(d["wildcard"], "0.0.0.63");
        assert_eq!(d["first_host"], "192.168.1.65");
        assert_eq!(d["last_host"], "192.168.1.126");
        assert_eq!(d["usable_hosts"], 62);
        let (n, _) = parse_net("c", "10.0.0.0 255.255.254.0").unwrap();
        assert_eq!(n.to_string(), "10.0.0.0/23");
        let d = net_info(&parse_net("c", "10.0.0.0/31").unwrap().0);
        assert_eq!(d["usable_hosts"], 2);
        assert!(parse_net("c", "10.0.0.0 255.0.255.0").is_err());
    }

    #[test]
    fn info_v6() {
        let d = net_info(&parse_net("c", "2001:db8::1/48").unwrap().0);
        assert_eq!(d["network"], "2001:db8::");
        assert_eq!(d["last_address"], "2001:db8:0:ffff:ffff:ffff:ffff:ffff");
        assert_eq!(d["subnets_64"], "65536");
        let d = net_info(&parse_net("c", "::/0").unwrap().0);
        assert_eq!(
            d["total_addresses"],
            "340282366920938463463374607431768211456"
        );
    }

    #[test]
    fn classify_and_ptr() {
        assert!(classify("100.64.1.1".parse().unwrap())
            .iter()
            .any(|t| t.contains("CGNAT")));
        assert!(classify("8.8.8.8".parse().unwrap())
            .iter()
            .any(|t| t.contains("global")));
        assert_eq!(
            reverse_pointer("1.2.3.4".parse().unwrap()),
            "4.3.2.1.in-addr.arpa"
        );
        assert!(reverse_pointer("2001:db8::1".parse().unwrap()).starts_with("1.0.0.0.0.0.0.0"));
        assert!(
            reverse_pointer("2001:db8::1".parse().unwrap()).ends_with("8.b.d.0.1.0.0.2.ip6.arpa")
        );
        assert_eq!(
            parse_ip("p", "3232235777").unwrap(),
            "192.168.1.1".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            parse_ip("p", "[fe80::1%eth0]").unwrap(),
            "fe80::1".parse::<IpAddr>().unwrap()
        );
    }
}
