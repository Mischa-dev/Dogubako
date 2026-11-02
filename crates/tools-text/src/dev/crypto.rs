//! Hashes, HMAC, secure random generators, UUIDs and JWT decoding.
//!
//! The passphrase generator embeds the EFF Large Wordlist
//! (https://www.eff.org/dice), © Electronic Frontier Foundation, licensed
//! under CC BY 3.0 US (https://creativecommons.org/licenses/by/3.0/us/).

use crate::util::*;
use base64::Engine;
use dogubako_core::prelude::*;
use rand::prelude::*;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![hash(), hmac_tool(), random(), uuid_tool(), jwt()]
}

const EFF_WORDS: &str = include_str!("../../data/eff_large_wordlist.txt");
const EFF_ATTRIBUTION: &str =
    "EFF Large Wordlist © Electronic Frontier Foundation, CC BY 3.0 US (https://www.eff.org/dice)";

fn encode_digest(bytes: &[u8], enc: &str) -> String {
    match enc {
        "base64" => base64::engine::general_purpose::STANDARD.encode(bytes),
        "HEX" => hex::encode_upper(bytes),
        _ => hex::encode(bytes),
    }
}

// ---------------------------------------------------------------------------
// dev.hash

const HASHES: &[(&str, &str)] = &[
    ("md5", "MD5"),
    ("sha1", "SHA-1"),
    ("sha224", "SHA-224"),
    ("sha256", "SHA-256"),
    ("sha384", "SHA-384"),
    ("sha512", "SHA-512"),
    ("sha512_256", "SHA-512/256"),
    ("sha3_224", "SHA3-224"),
    ("sha3_256", "SHA3-256"),
    ("sha3_384", "SHA3-384"),
    ("sha3_512", "SHA3-512"),
    ("keccak256", "Keccak-256 (Ethereum)"),
    ("blake2b", "BLAKE2b-512"),
    ("blake2s", "BLAKE2s-256"),
    ("blake3", "BLAKE3"),
    ("crc32", "CRC-32"),
];

pub(crate) fn digest(alg: &str, data: &[u8]) -> ToolResult<Vec<u8>> {
    use sha2::Digest;
    Ok(match alg {
        "md5" => md5::Md5::digest(data).to_vec(),
        "sha1" => sha1::Sha1::digest(data).to_vec(),
        "sha224" => sha2::Sha224::digest(data).to_vec(),
        "sha256" => sha2::Sha256::digest(data).to_vec(),
        "sha384" => sha2::Sha384::digest(data).to_vec(),
        "sha512" => sha2::Sha512::digest(data).to_vec(),
        "sha512_256" => sha2::Sha512_256::digest(data).to_vec(),
        "sha3_224" => sha3::Sha3_224::digest(data).to_vec(),
        "sha3_256" => sha3::Sha3_256::digest(data).to_vec(),
        "sha3_384" => sha3::Sha3_384::digest(data).to_vec(),
        "sha3_512" => sha3::Sha3_512::digest(data).to_vec(),
        "keccak256" => sha3::Keccak256::digest(data).to_vec(),
        "blake2b" => blake2::Blake2b512::digest(data).to_vec(),
        "blake2s" => blake2::Blake2s256::digest(data).to_vec(),
        "blake3" => blake3::hash(data).as_bytes().to_vec(),
        "crc32" => crc32fast::hash(data).to_be_bytes().to_vec(),
        other => {
            return Err(ToolError::invalid(
                "algorithms",
                format!("unknown algorithm {other}"),
            ))
        }
    })
}

fn hash() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.hash", "Hash Generator", Category::Dev)
        .describe("Compute MD5, SHA-1, SHA-2, SHA-3, Keccak, BLAKE2, BLAKE3 and CRC-32 digests of text or a file, optionally comparing with an expected hash")
        .keywords(&["hash", "md5", "sha1", "sha256", "sha512", "sha3", "blake3", "crc32", "checksum", "digest"])
        .param(ParamSpec::textarea("text").label("Text").primary())
        .param(ParamSpec::file("file").label("Or a file"))
        .param(ParamSpec::multi_choice("algorithms", HASHES).label("Algorithms").default_value(json!(["md5", "sha1", "sha256", "sha512"])))
        .param(ParamSpec::choice("encoding", &[("hex", "hex"), ("HEX", "HEX (uppercase)"), ("base64", "Base64")]).default_str("hex"))
        .param(ParamSpec::text("compare").label("Compare with expected hash").advanced())
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let bytes = match p.opt_file("file") {
            Some(f) => read_file(f)?,
            None => p.text("text").as_bytes().to_vec(),
        };
        let algs = p.list("algorithms");
        if algs.is_empty() {
            return Err(ToolError::invalid(
                "algorithms",
                "choose at least one algorithm",
            ));
        }
        let enc = p.str_or("encoding", "hex");
        let cmp = p.opt_str("compare").map(|c| c.trim().to_string());
        let mut rows = vec![];
        let mut matched: Option<String> = None;
        for a in &algs {
            let d = digest(a, &bytes)?;
            let s = encode_digest(&d, enc);
            if let Some(c) = &cmp {
                // Hex is case-insensitive; Base64 is not (case changes the bytes).
                if c.eq_ignore_ascii_case(&hex::encode(&d))
                    || *c == base64::engine::general_purpose::STANDARD.encode(&d)
                    || *c == s
                {
                    matched = Some(a.clone());
                }
            }
            let label = HASHES.iter().find(|h| h.0 == a).map(|h| h.1).unwrap_or(a);
            rows.push(vec![json!(label), json!(s)]);
        }
        let mut data =
            json!({"bytes": bytes.len(), "table": table(&["algorithm", "digest"], rows.clone())});
        if cmp.is_some() {
            data["match"] = json!(matched.is_some());
            data["matched_algorithm"] = json!(matched);
        }
        let mut out = ToolOutput::data(data);
        if rows.len() == 1 {
            out = out.with_text(rows[0][1].as_str().unwrap_or("").to_string(), "text/plain");
        }
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// dev.hmac

fn hmac_bytes(alg: &str, key: &[u8], data: &[u8]) -> ToolResult<Vec<u8>> {
    use hmac::{Hmac, KeyInit, Mac};
    macro_rules! mac {
        ($h:ty) => {{
            let mut m = <Hmac<$h> as KeyInit>::new_from_slice(key)
                .map_err(|e| ToolError::invalid("key", e.to_string()))?;
            m.update(data);
            m.finalize().into_bytes().to_vec()
        }};
    }
    Ok(match alg {
        "sha1" => mac!(sha1::Sha1),
        "sha256" => mac!(sha2::Sha256),
        "sha384" => mac!(sha2::Sha384),
        "sha512" => mac!(sha2::Sha512),
        "md5" => mac!(md5::Md5),
        other => {
            return Err(ToolError::invalid(
                "algorithm",
                format!("unknown algorithm {other}"),
            ))
        }
    })
}

fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn hmac_tool() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.hmac", "HMAC Generator & Verifier", Category::Dev)
        .describe("Compute or verify an HMAC (SHA-1, SHA-256, SHA-384, SHA-512, MD5) of a message with a secret key — e.g. webhook signatures")
        .keywords(&["hmac", "signature", "webhook", "sha256", "mac", "verify", "sign"])
        .param(ParamSpec::textarea("text").label("Message").required().primary())
        .param(ParamSpec::secret("key").label("Secret key").required())
        .param(ParamSpec::choice("key_format", &[("text", "Text (UTF-8)"), ("hex", "Hex"), ("base64", "Base64")]).default_str("text").advanced())
        .param(
            ParamSpec::choice("algorithm", &[("sha256", "HMAC-SHA256"), ("sha1", "HMAC-SHA1"), ("sha384", "HMAC-SHA384"), ("sha512", "HMAC-SHA512"), ("md5", "HMAC-MD5")])
                .default_str("sha256"),
        )
        .param(ParamSpec::choice("encoding", &[("hex", "hex"), ("base64", "Base64")]).default_str("hex"))
        .param(ParamSpec::text("expected").label("Verify against signature (optional)").placeholder("sha256=… or hex/base64"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let key_s = p.str("key")?;
        let key = match p.str_or("key_format", "text") {
            "hex" => hex::decode(key_s.trim())
                .map_err(|e| ToolError::invalid("key", format!("invalid hex key: {e}")))?,
            "base64" => super::encoders::b64_decode_lenient(key_s)
                .map_err(|e| ToolError::invalid("key", e))?,
            _ => key_s.as_bytes().to_vec(),
        };
        let alg = p.str("algorithm")?;
        let mac = hmac_bytes(alg, &key, p.text("text").as_bytes())?;
        let enc = p.str_or("encoding", "hex");
        let s = encode_digest(&mac, enc);
        let mut data =
            json!({"algorithm": format!("HMAC-{}", alg.to_uppercase()), "bytes": mac.len()});
        if let Some(exp) = p.opt_str("expected") {
            let e = exp.trim();
            let decode = |v: &str| {
                hex::decode(v)
                    .ok()
                    .filter(|b| b.len() == mac.len())
                    .or_else(|| super::encoders::b64_decode_lenient(v).ok())
                    .map(|c| ct_eq(&c, &mac))
                    .unwrap_or(false)
            };
            // The value as given first (padded Base64 ends in '='), then
            // without a label prefix such as "sha256=" (GitHub/Stripe style).
            let ok = decode(e)
                || e.split_once('=')
                    .filter(|(pre, v)| {
                        !pre.is_empty()
                            && !v.is_empty()
                            && pre
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                    })
                    .map(|(_, v)| decode(v))
                    .unwrap_or(false);
            data["valid"] = json!(ok);
        }
        Ok(ToolOutput::text(s).with_data(data))
    })
}

// ---------------------------------------------------------------------------
// dev.random

const AMBIGUOUS: &str = "Il1O0o";

fn random() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.random", "Password & Random Generator", Category::Dev)
        .describe("Generate cryptographically secure passwords (with entropy estimate), passphrases (EFF wordlist), hex/Base64 tokens and random integers")
        .keywords(&["password generator", "passphrase", "diceware", "random", "token", "secret", "api key", "random number", "entropy"])
        .param(
            ParamSpec::choice("kind", &[("password", "Password"), ("passphrase", "Passphrase"), ("hex", "Hex token"), ("base64", "Base64url token"), ("integer", "Integers")])
                .default_str("password"),
        )
        .param(ParamSpec::integer("count").label("How many").default_int(1).range(1.0, 1000.0))
        .param(ParamSpec::integer("length").label("Length (characters / bytes)").default_int(20).range(1.0, 4096.0).show_if("kind", &["password", "hex", "base64"]))
        .param(
            ParamSpec::multi_choice("charset", &[("lower", "a–z"), ("upper", "A–Z"), ("digits", "0–9"), ("symbols", "Symbols !@#…")])
                .label("Characters")
                .default_value(json!(["lower", "upper", "digits", "symbols"]))
                .show_if("kind", &["password"]),
        )
        .param(ParamSpec::boolean("exclude_ambiguous").label("Exclude look-alikes (Il1O0o)").show_if("kind", &["password"]))
        .param(ParamSpec::text("symbols").label("Symbol set").default_str("!@#$%^&*()-_=+[]{};:,.?/").show_if("kind", &["password"]).advanced())
        .param(ParamSpec::integer("words").label("Words").default_int(6).range(1.0, 64.0).show_if("kind", &["passphrase"]))
        .param(ParamSpec::text("separator").label("Separator").default_str("-").show_if("kind", &["passphrase"]))
        .param(ParamSpec::boolean("capitalize").label("Capitalize words").show_if("kind", &["passphrase"]))
        .param(ParamSpec::boolean("add_number").label("Append a digit").show_if("kind", &["passphrase"]))
        .param(ParamSpec::integer("min").label("Minimum").default_int(1).show_if("kind", &["integer"]))
        .param(ParamSpec::integer("max").label("Maximum").default_int(100).show_if("kind", &["integer"]))
        .param(ParamSpec::boolean("unique").label("No repeats").show_if("kind", &["integer"]))
        .text_output("text/plain");
    sync_tool(spec, |_ctx, p| {
        // ThreadRng is a CSPRNG (ChaCha) seeded and periodically reseeded from the OS.
        let mut rng = rand::rng();
        let count = p.int_or("count", 1).clamp(1, 1000) as usize;
        let kind = p.str("kind")?;
        let (items, entropy, extra): (Vec<String>, f64, Value) = match kind {
            "password" => {
                let len = p.int_or("length", 20).max(1) as usize;
                let excl = p.bool("exclude_ambiguous");
                let mut sets: Vec<Vec<char>> = vec![];
                for s in p.list("charset") {
                    let chars: String = match s.as_str() {
                        "lower" => ('a'..='z').collect(),
                        "upper" => ('A'..='Z').collect(),
                        "digits" => ('0'..='9').collect(),
                        "symbols" => p.str_or("symbols", "!@#$%^&*()-_=+[]{};:,.?/").to_string(),
                        _ => String::new(),
                    };
                    let mut v: Vec<char> = chars
                        .chars()
                        .filter(|c| !(excl && AMBIGUOUS.contains(*c)))
                        .collect();
                    v.dedup();
                    if !v.is_empty() {
                        sets.push(v);
                    }
                }
                if sets.is_empty() {
                    return Err(ToolError::invalid(
                        "charset",
                        "choose at least one character set",
                    ));
                }
                let mut all: Vec<char> = sets.concat();
                all.sort();
                all.dedup();
                let enforce = len >= sets.len();
                let gen = |rng: &mut ThreadRng| -> String {
                    loop {
                        let pw: Vec<char> = (0..len).map(|_| *all.choose(rng).unwrap()).collect();
                        // Rejection sampling keeps the distribution uniform over valid passwords.
                        if !enforce || sets.iter().all(|s| pw.iter().any(|c| s.contains(c))) {
                            return pw.into_iter().collect();
                        }
                    }
                };
                let items = (0..count).map(|_| gen(&mut rng)).collect();
                (
                    items,
                    len as f64 * (all.len() as f64).log2(),
                    json!({"alphabet_size": all.len()}),
                )
            }
            "passphrase" => {
                let words: Vec<&str> = EFF_WORDS.lines().filter(|l| !l.is_empty()).collect();
                let n = p.int_or("words", 6).max(1) as usize;
                let sep = p.text("separator");
                let cap = p.bool("capitalize");
                let num = p.bool("add_number");
                let items = (0..count)
                    .map(|_| {
                        let mut w: Vec<String> = (0..n)
                            .map(|_| {
                                let w = *words.choose(&mut rng).unwrap();
                                if cap {
                                    let mut c = w.chars();
                                    c.next()
                                        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                                        .unwrap_or_default()
                                } else {
                                    w.to_string()
                                }
                            })
                            .collect();
                        if num {
                            let i = rng.random_range(0..w.len());
                            w[i].push_str(&rng.random_range(0..10).to_string());
                        }
                        w.join(sep)
                    })
                    .collect();
                let mut bits = n as f64 * (words.len() as f64).log2();
                if num {
                    bits += 10f64.log2() + (n as f64).log2();
                }
                (
                    items,
                    bits,
                    json!({"wordlist": EFF_ATTRIBUTION, "wordlist_size": words.len()}),
                )
            }
            "hex" | "base64" => {
                let len = p.int_or("length", 20).max(1) as usize;
                let items = (0..count)
                    .map(|_| {
                        let mut b = vec![0u8; len];
                        rng.fill_bytes(&mut b);
                        if kind == "hex" {
                            hex::encode(b)
                        } else {
                            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
                        }
                    })
                    .collect();
                (items, len as f64 * 8.0, json!({"bytes": len}))
            }
            _ => {
                let (lo, hi) = (p.int_or("min", 1), p.int_or("max", 100));
                if lo > hi {
                    return Err(ToolError::invalid("max", "maximum must be ≥ minimum"));
                }
                let span = (hi as i128 - lo as i128 + 1) as f64;
                let items: Vec<String> = if p.bool("unique") {
                    if (count as f64) > span {
                        return Err(ToolError::invalid(
                            "count",
                            format!("only {span} distinct values in range"),
                        ));
                    }
                    let mut seen = std::collections::HashSet::new();
                    let mut v = vec![];
                    while v.len() < count {
                        let x = rng.random_range(lo..=hi);
                        if seen.insert(x) {
                            v.push(x.to_string());
                        }
                    }
                    v
                } else {
                    (0..count)
                        .map(|_| rng.random_range(lo..=hi).to_string())
                        .collect()
                };
                (items, span.log2(), json!({}))
            }
        };
        let strength = match entropy {
            e if e < 40.0 => "weak",
            e if e < 64.0 => "fair",
            e if e < 100.0 => "strong",
            _ => "very strong",
        };
        let mut data = json!({"entropy_bits": (entropy * 10.0).round() / 10.0, "strength": strength, "count": items.len()});
        if let (Value::Object(d), Value::Object(e)) = (&mut data, extra) {
            d.extend(e);
        }
        Ok(ToolOutput::text(items.join("\n")).with_data(data))
    })
}

// ---------------------------------------------------------------------------
// dev.uuid

fn inspect_uuid(s: &str) -> ToolResult<Value> {
    let u =
        uuid::Uuid::parse_str(s.trim()).map_err(|e| ToolError::invalid("uuid", e.to_string()))?;
    let variant = match u.get_variant() {
        uuid::Variant::NCS => "NCS (reserved)",
        uuid::Variant::RFC4122 => "RFC 9562 / RFC 4122",
        uuid::Variant::Microsoft => "Microsoft (reserved)",
        _ => "Future (reserved)",
    };
    let version = u.get_version_num();
    let vname = match version {
        0 if u.is_nil() => "nil",
        1 => "1 (time-based, MAC/node)",
        2 => "2 (DCE security)",
        3 => "3 (name-based, MD5)",
        4 => "4 (random)",
        5 => "5 (name-based, SHA-1)",
        6 => "6 (reordered time-based)",
        7 => "7 (Unix time-ordered)",
        8 => "8 (custom)",
        15 if u.is_max() => "max",
        _ => "unknown",
    };
    let mut v = json!({
        "uuid": u.hyphenated().to_string(),
        "version": vname,
        "variant": variant,
        "urn": u.urn().to_string(),
        "hex": u.simple().to_string(),
        "base64": base64::engine::general_purpose::STANDARD.encode(u.as_bytes()),
        "integer": u.as_u128().to_string(),
    });
    if let Some(ts) = u.get_timestamp() {
        let (secs, nanos) = ts.to_unix();
        if let Some(dt) = chrono::DateTime::from_timestamp(secs as i64, nanos) {
            v["timestamp"] = json!(dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
    }
    if let Some(node) = u.get_node_id() {
        v["node_id"] = json!(node
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join(":"));
    }
    Ok(v)
}

fn uuid_tool() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.uuid", "UUID Generator & Decoder", Category::Dev)
        .describe("Generate UUIDs (v4 random, v7 time-ordered, v1, v6, v3/v5 name-based, nil, max) in bulk, or decode a UUID to see its version, variant and embedded timestamp")
        .keywords(&["uuid", "guid", "uuid v4", "uuid v7", "uuidv5", "generate", "decode uuid", "ulid"])
        .param(ParamSpec::choice("mode", &[("generate", "Generate"), ("inspect", "Inspect / decode")]).default_str("generate"))
        .param(
            ParamSpec::choice("version", &[("v4", "v4 (random)"), ("v7", "v7 (time-ordered)"), ("v1", "v1 (time + random node)"), ("v6", "v6 (reordered time)"), ("v5", "v5 (SHA-1 name)"), ("v3", "v3 (MD5 name)"), ("nil", "Nil"), ("max", "Max")])
                .default_str("v4")
                .show_if("mode", &["generate"]),
        )
        .param(ParamSpec::integer("count").label("How many").default_int(1).range(1.0, 10000.0).show_if("mode", &["generate"]))
        .param(ParamSpec::text("namespace").label("Namespace (dns, url, oid, x500 or a UUID)").default_str("dns").show_if("version", &["v5", "v3"]))
        .param(ParamSpec::text("name").label("Name").show_if("version", &["v5", "v3"]))
        .param(ParamSpec::boolean("uppercase").label("Uppercase").show_if("mode", &["generate"]))
        .param(ParamSpec::boolean("no_hyphens").label("Without hyphens").show_if("mode", &["generate"]))
        .param(ParamSpec::boolean("braces").label("Wrap in {braces}").show_if("mode", &["generate"]).advanced())
        .param(ParamSpec::text("uuid").label("UUID to inspect").primary().show_if("mode", &["inspect"]))
        .text_output("text/plain");
    sync_tool(spec, |_ctx, p| {
        if p.str("mode")? == "inspect" {
            let s = p.str("uuid")?;
            return Ok(ToolOutput::data(inspect_uuid(s)?));
        }
        let count = p.int_or("count", 1).max(1) as usize;
        let ver = p.str("version")?;
        let ns = || -> ToolResult<uuid::Uuid> {
            Ok(
                match p.str_or("namespace", "dns").to_ascii_lowercase().as_str() {
                    "dns" => uuid::Uuid::NAMESPACE_DNS,
                    "url" => uuid::Uuid::NAMESPACE_URL,
                    "oid" => uuid::Uuid::NAMESPACE_OID,
                    "x500" => uuid::Uuid::NAMESPACE_X500,
                    other => uuid::Uuid::parse_str(other)
                        .map_err(|e| ToolError::invalid("namespace", e.to_string()))?,
                },
            )
        };
        let mut rng = rand::rng();
        let mut node = [0u8; 6];
        rng.fill_bytes(&mut node);
        node[0] |= 0x01; // multicast bit: random node id per RFC 9562 §6.10
        let mut out = vec![];
        for _ in 0..count {
            let u = match ver {
                "v4" => uuid::Uuid::new_v4(),
                "v7" => uuid::Uuid::now_v7(),
                "v1" => uuid::Uuid::now_v1(&node),
                "v6" => uuid::Uuid::now_v6(&node),
                "v5" | "v3" => {
                    let name = p.opt_str("name").ok_or_else(|| {
                        ToolError::invalid("name", "a name is required for name-based UUIDs")
                    })?;
                    if ver == "v5" {
                        uuid::Uuid::new_v5(&ns()?, name.as_bytes())
                    } else {
                        uuid::Uuid::new_v3(&ns()?, name.as_bytes())
                    }
                }
                "nil" => uuid::Uuid::nil(),
                _ => uuid::Uuid::max(),
            };
            let mut s = if p.bool("no_hyphens") {
                u.simple().to_string()
            } else {
                u.hyphenated().to_string()
            };
            if p.bool("uppercase") {
                s = s.to_uppercase();
            }
            if p.bool("braces") {
                s = format!("{{{s}}}");
            }
            out.push(s);
        }
        Ok(ToolOutput::text(out.join("\n")).with_data(json!({"count": out.len(), "version": ver})))
    })
}

// ---------------------------------------------------------------------------
// dev.jwt

fn b64url_json(part: &str, what: &str) -> ToolResult<Value> {
    let bytes = super::encoders::b64_decode_lenient(part)
        .map_err(|e| ToolError::invalid("token", format!("{what}: {e}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| ToolError::invalid("token", format!("{what} is not valid JSON: {e}")))
}

fn jwt() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("dev.jwt", "JWT Decoder & Verifier", Category::Dev)
        .describe("Decode a JSON Web Token's header and claims with readable exp/iat/nbf times. The signature is only verified if you supply a secret (HS*) or a PEM public key (RS*/PS*/ES*/EdDSA)")
        .keywords(&["jwt", "json web token", "decode jwt", "bearer token", "oauth", "verify signature", "claims", "jws"])
        .param(ParamSpec::textarea("token").label("Token").required().primary())
        .param(ParamSpec::secret("secret").label("HMAC secret (HS256/384/512)"))
        .param(ParamSpec::boolean("secret_base64").label("Secret is Base64-encoded").advanced())
        .param(ParamSpec::textarea("public_key").label("Public key (PEM) for RS/PS/ES/EdDSA").advanced())
        .output(OutputKind::Data)
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let token: String = p
            .text("token")
            .trim()
            .trim_start_matches("Bearer ")
            .trim()
            .to_string();
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() == 5 {
            return Err(ToolError::unsupported("this is an encrypted JWE token (5 parts); its payload cannot be decoded without the private key"));
        }
        if parts.len() != 3 {
            return Err(ToolError::invalid(
                "token",
                format!("a JWT has 3 dot-separated parts, found {}", parts.len()),
            ));
        }
        let header = b64url_json(parts[0], "header")?;
        let claims = b64url_json(parts[1], "payload")?;
        let now = chrono::Utc::now().timestamp();
        let mut times = serde_json::Map::new();
        for k in ["exp", "iat", "nbf", "auth_time"] {
            if let Some(t) = claims
                .get(k)
                .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
            {
                if let Some(dt) = chrono::DateTime::from_timestamp(t, 0) {
                    times.insert(
                        k.to_string(),
                        json!(format!(
                            "{} ({})",
                            dt.to_rfc3339(),
                            super::time::relative(t - now)
                        )),
                    );
                }
            }
        }
        let expired = claims
            .get("exp")
            .and_then(|v| v.as_f64())
            .map(|e| (e as i64) < now);
        let not_yet = claims
            .get("nbf")
            .and_then(|v| v.as_f64())
            .map(|n| (n as i64) > now);
        let alg = header
            .get("alg")
            .and_then(|a| a.as_str())
            .unwrap_or("")
            .to_string();
        let mut sig_status = "NOT VERIFIED — no key supplied. Anyone can forge an unverified token; never trust its claims without verification.".to_string();
        let mut verified = Value::Null;
        if alg.eq_ignore_ascii_case("none") {
            sig_status =
                "UNSIGNED (alg: none) — this token has no signature and must not be trusted".into();
            verified = json!(false);
        } else if p.has("secret") || p.opt_str("public_key").is_some() {
            use jsonwebtoken::{Algorithm, DecodingKey, Validation};
            let algo: Algorithm = alg
                .parse()
                .map_err(|_| ToolError::unsupported(format!("unsupported algorithm '{alg}'")))?;
            let key = if let Some(pem) = p.opt_str("public_key") {
                let pem = pem.as_bytes();
                match alg.get(..2) {
                    Some("RS") | Some("PS") => DecodingKey::from_rsa_pem(pem),
                    Some("ES") => DecodingKey::from_ec_pem(pem),
                    _ if alg == "EdDSA" => DecodingKey::from_ed_pem(pem),
                    _ => {
                        return Err(ToolError::invalid(
                            "public_key",
                            format!("a public key cannot verify {alg}; use the secret field"),
                        ))
                    }
                }
                .map_err(|e| ToolError::invalid("public_key", format!("invalid PEM key: {e}")))?
            } else {
                let s = p.str("secret")?;
                if !alg.starts_with("HS") {
                    return Err(ToolError::invalid(
                        "secret",
                        format!("{alg} tokens are verified with a public key, not a shared secret"),
                    ));
                }
                if p.bool("secret_base64") {
                    DecodingKey::from_secret(
                        &super::encoders::b64_decode_lenient(s)
                            .map_err(|e| ToolError::invalid("secret", e))?,
                    )
                } else {
                    DecodingKey::from_secret(s.as_bytes())
                }
            };
            let mut v = Validation::new(algo);
            v.validate_exp = false;
            v.validate_nbf = false;
            v.validate_aud = false;
            v.required_spec_claims.clear();
            match jsonwebtoken::decode::<Value>(&token, &key, &v) {
                Ok(_) => {
                    sig_status = "VALID signature".into();
                    verified = json!(true);
                }
                Err(e) => {
                    sig_status = format!("INVALID signature ({e})");
                    verified = json!(false);
                }
            }
        }
        Ok(ToolOutput::data(json!({
            "signature": sig_status,
            "signature_valid": verified,
            "algorithm": alg,
            "expired": expired,
            "not_yet_valid": not_yet,
            "times": times,
            "header": header,
            "payload": claims,
        })))
    })
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    #[tokio::test]
    async fn hashes() {
        let h = TestHarness::new(crate::tools());
        let out = h.run("dev.hash", json!({"text": "abc", "algorithms": ["md5", "sha256", "crc32", "blake3", "sha3_256"]})).await.unwrap();
        let rows = out.data["table"]["rows"].as_array().unwrap();
        assert_eq!(rows[0][1], "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            rows[1][1],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(rows[2][1], "352441c2");
        assert_eq!(
            rows[3][1],
            "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
        );
        assert_eq!(
            rows[4][1],
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
        );
        let out = h.run("dev.hash", json!({"text": "abc", "algorithms": ["md5"], "compare": "900150983CD24FB0D6963F7D28E17F72"})).await.unwrap();
        assert_eq!(out.data["match"], true);
        // The uppercase option must not be folded into the lowercase one.
        let out = h
            .run(
                "dev.hash",
                json!({"text": "abc", "algorithms": ["md5"], "encoding": "HEX"}),
            )
            .await
            .unwrap();
        assert_eq!(
            out.data["table"]["rows"][0][1],
            "900150983CD24FB0D6963F7D28E17F72"
        );
        // Base64 is case-sensitive: a case-changed digest must not match.
        let b64 = |c: &str| json!({"text": "abc", "algorithms": ["sha256"], "encoding": "base64", "compare": c});
        let out = h
            .run(
                "dev.hash",
                b64("ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="),
            )
            .await
            .unwrap();
        assert_eq!(out.data["match"], true);
        let out = h
            .run(
                "dev.hash",
                b64("UNGwV48bZ+PbqudExA4Ii7adyAowf3QCTbd/yFiafA0="),
            )
            .await
            .unwrap();
        assert_eq!(out.data["match"], false);
        assert_eq!(
            h.run("dev.hash", json!({"text": "x", "algorithms": []}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        // RFC 4231 test case 2
        let out = h
            .run(
                "dev.hmac",
                json!({"text": "what do ya want for nothing?", "key": "Jefe"}),
            )
            .await
            .unwrap();
        assert_eq!(
            out.text.unwrap(),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        let out = h
            .run("dev.hmac", json!({"text": "what do ya want for nothing?", "key": "Jefe", "expected": "sha256=5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"}))
            .await
            .unwrap();
        assert_eq!(out.data["valid"], true);
        let out = h
            .run(
                "dev.hmac",
                json!({"text": "x", "key": "Jefe", "expected": "00"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["valid"], false);
        // Padded Base64 whose characters before '=' are all alphanumeric
        // must not be mistaken for a "label=" prefix.
        let out = h
            .run(
                "dev.hmac",
                json!({"text": "1", "key": "key", "expected": "bakfuRUXvh9c3POvkdfUDHF91jijBhV2BvsuWE966SY="}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["valid"], true);
    }

    #[tokio::test]
    async fn randoms() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run(
                "dev.random",
                json!({"length": 16, "count": 5, "charset": ["lower", "digits"]}),
            )
            .await
            .unwrap();
        let t = out.text.unwrap();
        assert_eq!(t.lines().count(), 5);
        assert!(t.lines().all(|l| l.len() == 16
            && l.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && l.chars().any(|c| c.is_ascii_digit())));
        assert!((out.data["entropy_bits"].as_f64().unwrap() - 82.7).abs() < 0.1);
        let out = h
            .run(
                "dev.random",
                json!({"kind": "passphrase", "words": 5, "separator": " "}),
            )
            .await
            .unwrap();
        assert_eq!(out.text.unwrap().split(' ').count(), 5);
        assert!(out.data["wordlist"].as_str().unwrap().contains("EFF"));
        let out = h
            .run("dev.random", json!({"kind": "hex", "length": 8}))
            .await
            .unwrap();
        assert_eq!(out.text.unwrap().len(), 16);
        let out = h
            .run(
                "dev.random",
                json!({"kind": "integer", "min": 1, "max": 5, "count": 5, "unique": true}),
            )
            .await
            .unwrap();
        let mut v: Vec<i32> = out
            .text
            .unwrap()
            .lines()
            .map(|l| l.parse().unwrap())
            .collect();
        v.sort();
        assert_eq!(v, [1, 2, 3, 4, 5]);
        assert_eq!(
            h.run("dev.random", json!({"kind": "integer", "min": 5, "max": 1}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            h.run("dev.random", json!({"charset": []}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn uuids() {
        let h = TestHarness::new(crate::tools());
        let out = h
            .run("dev.uuid", json!({"version": "v7", "count": 3}))
            .await
            .unwrap()
            .text
            .unwrap();
        let ids: Vec<&str> = out.lines().collect();
        assert_eq!(ids.len(), 3);
        let info = h
            .run("dev.uuid", json!({"mode": "inspect", "uuid": ids[0]}))
            .await
            .unwrap();
        assert!(info.data["version"].as_str().unwrap().starts_with('7'));
        assert!(info.data["timestamp"].as_str().unwrap().starts_with("20"));
        let out = h
            .run(
                "dev.uuid",
                json!({"version": "v5", "namespace": "dns", "name": "example.com"}),
            )
            .await
            .unwrap()
            .text
            .unwrap();
        assert_eq!(out, "cfbff0d1-9375-5685-968c-48ce8b15ae17");
        let out = h
            .run(
                "dev.uuid",
                json!({"version": "v4", "uppercase": true, "no_hyphens": true}),
            )
            .await
            .unwrap()
            .text
            .unwrap();
        assert_eq!(out.len(), 32);
        assert_eq!(
            h.run("dev.uuid", json!({"mode": "inspect", "uuid": "not-a-uuid"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            h.run("dev.uuid", json!({"version": "v5"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }

    #[tokio::test]
    async fn jwts() {
        let h = TestHarness::new(crate::tools());
        // jwt.io sample token, HS256 with secret "your-256-bit-secret"
        let tok = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyfQ.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
        let out = h.run("dev.jwt", json!({"token": tok})).await.unwrap();
        assert_eq!(out.data["payload"]["name"], "John Doe");
        assert!(out.data["signature"]
            .as_str()
            .unwrap()
            .starts_with("NOT VERIFIED"));
        assert!(out.data["times"]["iat"]
            .as_str()
            .unwrap()
            .starts_with("2018-01-18"));
        let out = h
            .run(
                "dev.jwt",
                json!({"token": tok, "secret": "your-256-bit-secret"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["signature_valid"], true);
        let out = h
            .run("dev.jwt", json!({"token": tok, "secret": "wrong"}))
            .await
            .unwrap();
        assert_eq!(out.data["signature_valid"], false);
        assert_eq!(
            h.run("dev.jwt", json!({"token": "abc.def"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
        assert_eq!(
            h.run("dev.jwt", json!({"token": "!!.!!.!!"}))
                .await
                .unwrap_err()
                .code(),
            "invalid_input"
        );
    }
}
