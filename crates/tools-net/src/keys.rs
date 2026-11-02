//! `network.key_inspect`: identify public/private keys without ever echoing
//! private key material.

use crate::certs::spki_details;
use crate::util::{
    der_children, der_read, der_seq, der_write, parse_pem, pem_encode, table, PemBlock,
};
use dogubako_core::prelude::*;
use x509_parser::prelude::FromDer;
use x509_parser::x509::SubjectPublicKeyInfo;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![key_inspect()]
}

// DER-encoded OID contents.
const OID_RSA: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
const OID_EC: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
const OID_ED25519: &[u8] = &[0x2b, 0x65, 0x70];

fn oid_string(content: &[u8]) -> String {
    x509_parser::der_parser::oid::Oid::new(std::borrow::Cow::Borrowed(content)).to_id_string()
}

/// SPKI DER for an RSA public key given INTEGER contents of n and e.
fn rsa_spki(n: &[u8], e: &[u8]) -> Vec<u8> {
    let pkcs1 = der_seq(&[der_write(0x02, n), der_write(0x02, e)]);
    spki(
        &der_seq(&[der_write(0x06, OID_RSA), der_write(0x05, &[])]),
        &pkcs1,
    )
}

fn spki(alg_id: &[u8], key: &[u8]) -> Vec<u8> {
    let mut bits = vec![0u8];
    bits.extend_from_slice(key);
    der_seq(&[alg_id.to_vec(), der_write(0x03, &bits)])
}

/// Result of analysing one key.
struct KeyInfo {
    kind: &'static str,
    format: String,
    encrypted: bool,
    spki_der: Option<Vec<u8>>,
    algorithm_hint: Option<String>,
    extra: serde_json::Map<String, Value>,
}

impl KeyInfo {
    fn new(kind: &'static str, format: &str) -> Self {
        KeyInfo {
            kind,
            format: format.into(),
            encrypted: false,
            spki_der: None,
            algorithm_hint: None,
            extra: Default::default(),
        }
    }
}

/// PKCS#1 RSAPrivateKey → public SPKI.
fn rsa_private_to_spki(der: &[u8]) -> Option<Vec<u8>> {
    let (seq, _) = der_read(der)?;
    let items = der_children(seq.content)?;
    if seq.tag != 0x30 || items.len() < 3 || items[1].tag != 0x02 || items[2].tag != 0x02 {
        return None;
    }
    Some(rsa_spki(items[1].content, items[2].content))
}

/// SEC1 ECPrivateKey → (curve OID content, public point) when embedded.
type Sec1Parts = (Option<Vec<u8>>, Option<Vec<u8>>);

fn sec1_parts(der: &[u8]) -> Option<Sec1Parts> {
    let (seq, _) = der_read(der)?;
    let items = der_children(seq.content)?;
    if seq.tag != 0x30 || items.len() < 2 || items[1].tag != 0x04 {
        return None;
    }
    let mut curve = None;
    let mut point = None;
    for it in &items[2..] {
        match it.tag {
            0xa0 => {
                curve = der_read(it.content)
                    .filter(|(t, _)| t.tag == 0x06)
                    .map(|(t, _)| t.content.to_vec())
            }
            0xa1 => {
                point = der_read(it.content)
                    .filter(|(t, _)| t.tag == 0x03 && !t.content.is_empty())
                    .map(|(t, _)| t.content[1..].to_vec())
            }
            _ => {}
        }
    }
    Some((curve, point))
}

fn ec_spki(curve: &[u8], point: &[u8]) -> Vec<u8> {
    spki(
        &der_seq(&[der_write(0x06, OID_EC), der_write(0x06, curve)]),
        point,
    )
}

fn pkcs8_private(der: &[u8], info: &mut KeyInfo) -> ToolResult<()> {
    let bad = || ToolError::input("malformed PKCS#8 private key");
    let (seq, _) = der_read(der).ok_or_else(bad)?;
    let items = der_children(seq.content).ok_or_else(bad)?;
    if seq.tag != 0x30 || items.len() < 3 || items[1].tag != 0x30 || items[2].tag != 0x04 {
        return Err(bad());
    }
    let alg = der_children(items[1].content).ok_or_else(bad)?;
    let alg_oid = alg
        .first()
        .filter(|t| t.tag == 0x06)
        .ok_or_else(bad)?
        .content;
    info.algorithm_hint = Some(
        crate::certs::oid_name(&oid_string(alg_oid))
            .map(|s| s.to_string())
            .unwrap_or_else(|| oid_string(alg_oid)),
    );
    let inner = items[2].content;
    if alg_oid == OID_RSA {
        info.spki_der = rsa_private_to_spki(inner);
    } else if alg_oid == OID_EC {
        let curve = alg
            .get(1)
            .filter(|t| t.tag == 0x06)
            .map(|t| t.content.to_vec());
        if let Some((c2, point)) = sec1_parts(inner) {
            if let (Some(c), Some(p)) = (curve.or(c2), point) {
                info.spki_der = Some(ec_spki(&c, &p));
            }
        }
        if info.spki_der.is_none() {
            info.extra.insert(
                "note".into(),
                json!("public point not embedded; fingerprint unavailable"),
            );
        }
    } else if alg_oid == OID_ED25519 {
        let (seed, _) = der_read(inner)
            .filter(|(t, _)| t.tag == 0x04 && t.content.len() == 32)
            .ok_or_else(bad)?;
        let mut s = [0u8; 32];
        s.copy_from_slice(seed.content);
        let sk = ed25519_dalek::SigningKey::from_bytes(&s);
        s.fill(0);
        let pk = sk.verifying_key().to_bytes();
        info.spki_der = Some(spki(&der_seq(&[der_write(0x06, OID_ED25519)]), &pk));
    } else {
        // Optional publicKey [1] (PKCS#8 v2 / OneAsymmetricKey).
        if let Some(pubk) = items
            .iter()
            .find(|t| t.tag == 0x81)
            .map(|t| t.content)
            .filter(|c| !c.is_empty())
        {
            info.spki_der = Some(spki(&der_write(0x30, items[1].content), &pubk[1..]));
        } else {
            info.extra.insert(
                "note".into(),
                json!("public key cannot be derived for this algorithm"),
            );
        }
    }
    Ok(())
}

fn inspect_pem(b: &PemBlock) -> ToolResult<KeyInfo> {
    let legacy_encrypted = b
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("Proc-Type") && v.contains("ENCRYPTED"));
    match b.label.as_str() {
        "PUBLIC KEY" => {
            let mut k = KeyInfo::new("public", "SubjectPublicKeyInfo (X.509 / PKCS#8 public)");
            k.spki_der = Some(b.der.clone());
            Ok(k)
        }
        "RSA PUBLIC KEY" => {
            let mut k = KeyInfo::new("public", "PKCS#1 RSA public key");
            let (seq, _) =
                der_read(&b.der).ok_or_else(|| ToolError::input("malformed RSA public key"))?;
            let it = der_children(seq.content)
                .ok_or_else(|| ToolError::input("malformed RSA public key"))?;
            if it.len() != 2 {
                return Err(ToolError::input("malformed RSA public key"));
            }
            k.spki_der = Some(rsa_spki(it[0].content, it[1].content));
            Ok(k)
        }
        "RSA PRIVATE KEY" => {
            let mut k = KeyInfo::new("private", "PKCS#1 RSA private key (traditional OpenSSL)");
            k.algorithm_hint = Some("RSA".into());
            if legacy_encrypted {
                k.encrypted = true;
            } else {
                k.spki_der = Some(
                    rsa_private_to_spki(&b.der)
                        .ok_or_else(|| ToolError::input("malformed RSA private key"))?,
                );
            }
            Ok(k)
        }
        "EC PRIVATE KEY" => {
            let mut k = KeyInfo::new("private", "SEC1 EC private key (traditional OpenSSL)");
            k.algorithm_hint = Some("EC".into());
            if legacy_encrypted {
                k.encrypted = true;
            } else {
                match sec1_parts(&b.der) {
                    Some((Some(c), Some(p))) => k.spki_der = Some(ec_spki(&c, &p)),
                    Some(_) => {
                        k.extra.insert(
                            "note".into(),
                            json!("curve or public point not embedded; fingerprint unavailable"),
                        );
                    }
                    None => return Err(ToolError::input("malformed EC private key")),
                }
            }
            Ok(k)
        }
        "PRIVATE KEY" => {
            let mut k = KeyInfo::new("private", "PKCS#8 private key");
            pkcs8_private(&b.der, &mut k)?;
            Ok(k)
        }
        "ENCRYPTED PRIVATE KEY" => {
            let mut k = KeyInfo::new("private", "PKCS#8 encrypted private key");
            k.encrypted = true;
            // EncryptedPrivateKeyInfo ::= SEQ { AlgorithmIdentifier, OCTET STRING }
            if let Some(oid) = der_read(&b.der)
                .and_then(|(s, _)| der_children(s.content))
                .and_then(|c| c.first().and_then(|a| der_children(a.content)))
                .and_then(|a| a.first().map(|o| oid_string(o.content)))
            {
                let name = match oid.as_str() {
                    "1.2.840.113549.1.5.13" => "PBES2".to_string(),
                    o => o.to_string(),
                };
                k.extra.insert("encryption_scheme".into(), json!(name));
            }
            Ok(k)
        }
        "OPENSSH PRIVATE KEY" => {
            let pem = pem_encode("OPENSSH PRIVATE KEY", &b.der);
            let key = ssh_key::PrivateKey::from_openssh(pem.as_bytes())
                .map_err(|e| ToolError::input(format!("invalid OpenSSH private key: {e}")))?;
            let mut k = KeyInfo::new("private", "OpenSSH private key");
            k.encrypted = key.is_encrypted();
            k.extra
                .insert("cipher".into(), json!(format!("{:?}", key.cipher())));
            ssh_public_details(key.public_key(), &mut k.extra);
            k.algorithm_hint = Some(key.algorithm().to_string());
            Ok(k)
        }
        "CERTIFICATE" => {
            let (_, c) = x509_parser::prelude::X509Certificate::from_der(&b.der)
                .map_err(|e| ToolError::input(format!("invalid certificate: {e}")))?;
            let mut k = KeyInfo::new("public", "public key extracted from X.509 certificate");
            k.spki_der = Some(c.public_key().raw.to_vec());
            Ok(k)
        }
        "DSA PRIVATE KEY" => {
            let mut k = KeyInfo::new("private", "DSA private key (traditional OpenSSL)");
            k.algorithm_hint = Some("DSA".into());
            k.encrypted = legacy_encrypted;
            Ok(k)
        }
        other => Err(ToolError::input(format!("unsupported PEM block '{other}'"))),
    }
}

fn ssh_public_details(pk: &ssh_key::PublicKey, m: &mut serde_json::Map<String, Value>) {
    use ssh_key::public::KeyData;
    m.insert("ssh_algorithm".into(), json!(pk.algorithm().to_string()));
    m.insert(
        "ssh_fingerprint_sha256".into(),
        json!(pk.fingerprint(ssh_key::HashAlg::Sha256).to_string()),
    );
    m.insert(
        "ssh_fingerprint_sha512".into(),
        json!(pk.fingerprint(ssh_key::HashAlg::Sha512).to_string()),
    );
    if !pk.comment().is_empty() {
        m.insert("comment".into(), json!(pk.comment()));
    }
    let bits: Option<usize> = match pk.key_data() {
        KeyData::Rsa(r) => {
            let n = r.n.as_bytes();
            let n = if n.first() == Some(&0) { &n[1..] } else { n };
            Some(n.len() * 8 - n.first().map(|b| b.leading_zeros() as usize).unwrap_or(0))
        }
        KeyData::Ed25519(_) | KeyData::SkEd25519(_) => Some(256),
        KeyData::Ecdsa(e) => Some(match e.curve() {
            ssh_key::EcdsaCurve::NistP256 => 256,
            ssh_key::EcdsaCurve::NistP384 => 384,
            ssh_key::EcdsaCurve::NistP521 => 521,
        }),
        KeyData::SkEcdsaSha2NistP256(_) => Some(256),
        KeyData::Dsa(d) => Some(d.p.as_bytes().len().saturating_sub(1) * 8),
        _ => None,
    };
    m.insert("bits".into(), json!(bits));
    if let Ok(s) = pk.to_openssh() {
        m.insert("ssh_public_key".into(), json!(s));
    }
}

fn describe(k: KeyInfo, source: &str) -> Value {
    let mut m = serde_json::Map::new();
    m.insert("source".into(), json!(source));
    m.insert("kind".into(), json!(k.kind));
    m.insert("format".into(), json!(k.format));
    m.insert("encrypted".into(), json!(k.encrypted));
    if let Some(h) = &k.algorithm_hint {
        m.insert("algorithm".into(), json!(h));
    }
    if let Some(der) = &k.spki_der {
        match SubjectPublicKeyInfo::from_der(der) {
            Ok((_, spki)) => {
                let d = spki_details(&spki);
                for (key, v) in d.as_object().unwrap() {
                    m.insert(key.clone(), v.clone());
                }
                m.insert(
                    "public_key_pem".into(),
                    json!(pem_encode("PUBLIC KEY", der)),
                );
            }
            Err(e) => {
                m.insert(
                    "error".into(),
                    json!(format!("public key could not be parsed: {e}")),
                );
            }
        }
    }
    if k.encrypted {
        m.insert(
            "note".into(),
            json!(
                "key is encrypted; only metadata is shown (no passphrase is requested or needed)"
            ),
        );
    }
    for (key, v) in k.extra {
        m.entry(key).or_insert(v);
    }
    if k.kind == "private" {
        m.insert("private_material_echoed".into(), json!(false));
    }
    Value::Object(m)
}

fn key_inspect() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.key_inspect", "Key Inspector", Category::Network)
        .describe("Identify public/private keys (PEM, PKCS#1/#8, SEC1, OpenSSH, SSH public keys): type, size, curve and SHA-256 fingerprints — private key material is never echoed")
        .keywords(&["key", "public key", "private key", "rsa", "ecdsa", "ed25519", "ssh", "fingerprint", "pem", "pkcs8", "spki"])
        .param(ParamSpec::secret("key").label("Key text").describe("PEM block(s) or SSH public key line(s). Processed locally; not stored or echoed."))
        .param(ParamSpec::file("file").label("…or key file").accept(&[".pem", ".key", ".pub", ".der", ".crt", ".txt"]))
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let mut text = p.text("key").to_string();
        if let Some(f) = p.opt_file("file") {
            let bytes = std::fs::read(&f.path)?;
            if bytes.len() > 1024 * 1024 {
                return Err(ToolError::invalid("file", "file too large for a key"));
            }
            match String::from_utf8(bytes) {
                Ok(s) => {
                    text.push('\n');
                    text.push_str(&s);
                }
                Err(e) => {
                    // Raw DER: try SPKI.
                    let der = e.into_bytes();
                    if SubjectPublicKeyInfo::from_der(&der).is_ok() {
                        text.push_str(&format!("\n{}", pem_encode("PUBLIC KEY", &der)));
                    } else {
                        return Err(ToolError::invalid("file", "binary file is not a DER SubjectPublicKeyInfo; convert private keys to PEM"));
                    }
                }
            }
        }
        if text.trim().is_empty() {
            return Err(ToolError::invalid(
                "key",
                "paste a key or upload a key file",
            ));
        }
        let mut items = vec![];
        let mut errors = vec![];
        for b in parse_pem(&text) {
            match inspect_pem(&b) {
                Ok(k) => items.push(describe(k, &format!("PEM {}", b.label))),
                Err(e) => errors.push(e.to_string()),
            }
        }
        // SSH public keys (authorized_keys / .pub lines), possibly with options.
        let mut in_pem = false;
        for line in text.lines().map(str::trim) {
            if line.starts_with("-----BEGIN") {
                in_pem = true;
            }
            if line.starts_with("-----END") {
                in_pem = false;
                continue;
            }
            if in_pem || line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some(pos) = ["ssh-", "ecdsa-sha2-", "sk-ssh-", "sk-ecdsa-"]
                .iter()
                .filter_map(|pfx| line.find(pfx))
                .min()
            else {
                continue;
            };
            match ssh_key::PublicKey::from_openssh(&line[pos..]) {
                Ok(pk) => {
                    let mut m = serde_json::Map::new();
                    m.insert("source".into(), json!("SSH public key"));
                    m.insert("kind".into(), json!("public"));
                    m.insert("format".into(), json!("OpenSSH public key"));
                    m.insert("encrypted".into(), json!(false));
                    ssh_public_details(&pk, &mut m);
                    m.insert("algorithm".into(), json!(pk.algorithm().to_string()));
                    if pos > 0 {
                        m.insert("authorized_keys_options".into(), json!(line[..pos].trim()));
                    }
                    items.push(Value::Object(m));
                }
                Err(e) => errors.push(format!("SSH key line: {e}")),
            }
        }
        if items.is_empty() {
            return Err(ToolError::input(if errors.is_empty() {
                "no key found (expected a PEM block or an SSH public key line)".to_string()
            } else {
                errors.join("; ")
            }));
        }
        let lines: Vec<String> = items
            .iter()
            .map(|d| {
                let fp = d
                    .get("spki_sha256_base64")
                    .and_then(|v| v.as_str())
                    .map(|s| format!("SPKI SHA256:{s}"))
                    .or_else(|| {
                        d.get("ssh_fingerprint_sha256")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| "no fingerprint".into());
                format!(
                    "{} {} key, {}{} — {}",
                    d["kind"].as_str().unwrap_or(""),
                    d.get("algorithm")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown"),
                    d.get("bits")
                        .and_then(|v| v.as_u64())
                        .map(|b| format!("{b} bits"))
                        .unwrap_or_else(|| "size unknown".into()),
                    if d["encrypted"] == true {
                        ", encrypted"
                    } else {
                        ""
                    },
                    fp
                )
            })
            .collect();
        let rows: Vec<Vec<Value>> = items
            .iter()
            .map(|d| {
                vec![
                    d["kind"].clone(),
                    d.get("algorithm").cloned().unwrap_or(Value::Null),
                    d.get("bits").cloned().unwrap_or(Value::Null),
                    d["format"].clone(),
                    d["encrypted"].clone(),
                    d.get("spki_sha256_base64")
                        .or(d.get("ssh_fingerprint_sha256"))
                        .cloned()
                        .unwrap_or(Value::Null),
                ]
            })
            .collect();
        let data = if items.len() == 1 {
            let mut d = items.pop().unwrap();
            d["errors"] = json!(errors);
            d
        } else {
            json!({ "count": items.len(), "items": items, "errors": errors, "table": table(&["kind", "algorithm", "bits", "format", "encrypted", "fingerprint"], rows) })
        };
        Ok(ToolOutput::text(lines.join("\n")).with_data(data))
    })
}
