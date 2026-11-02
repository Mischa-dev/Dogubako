//! X.509 certificate / CSR decoding shared by `cert_decode`, `tls_inspect`
//! and `domain_inspect`.

use crate::util::{b64, colon_hex, parse_pem, sha1, sha256, table};
use dogubako_core::prelude::*;
use x509_parser::extensions::{GeneralName, ParsedExtension};
use x509_parser::prelude::*;
use x509_parser::public_key::PublicKey;
use x509_parser::x509::SubjectPublicKeyInfo;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![cert_decode()]
}

/// Friendly names for common algorithm / curve OIDs.
pub fn oid_name(oid: &str) -> Option<&'static str> {
    Some(match oid {
        "1.2.840.113549.1.1.1" => "RSA",
        "1.2.840.113549.1.1.4" => "md5WithRSAEncryption",
        "1.2.840.113549.1.1.5" => "sha1WithRSAEncryption",
        "1.2.840.113549.1.1.10" => "RSASSA-PSS",
        "1.2.840.113549.1.1.11" => "sha256WithRSAEncryption",
        "1.2.840.113549.1.1.12" => "sha384WithRSAEncryption",
        "1.2.840.113549.1.1.13" => "sha512WithRSAEncryption",
        "1.2.840.10045.2.1" => "EC",
        "1.2.840.10045.4.1" => "ecdsa-with-SHA1",
        "1.2.840.10045.4.3.2" => "ecdsa-with-SHA256",
        "1.2.840.10045.4.3.3" => "ecdsa-with-SHA384",
        "1.2.840.10045.4.3.4" => "ecdsa-with-SHA512",
        "1.2.840.10040.4.1" => "DSA",
        "1.2.840.10040.4.3" => "dsa-with-SHA1",
        "2.16.840.1.101.3.4.3.2" => "dsa-with-SHA256",
        "1.3.101.110" => "X25519",
        "1.3.101.111" => "X448",
        "1.3.101.112" => "Ed25519",
        "1.3.101.113" => "Ed448",
        "1.2.840.10045.3.1.7" => "P-256 (secp256r1)",
        "1.3.132.0.34" => "P-384 (secp384r1)",
        "1.3.132.0.35" => "P-521 (secp521r1)",
        "1.3.132.0.10" => "secp256k1",
        "1.3.36.3.3.2.8.1.1.7" => "brainpoolP256r1",
        "1.3.36.3.3.2.8.1.1.11" => "brainpoolP384r1",
        "1.3.36.3.3.2.8.1.1.13" => "brainpoolP512r1",
        "1.3.6.1.5.5.7.48.1" => "OCSP",
        "1.3.6.1.5.5.7.48.2" => "CA Issuers",
        _ => return None,
    })
}

fn oid_label(oid: &x509_parser::der_parser::oid::Oid) -> String {
    let s = oid.to_id_string();
    oid_name(&s).map(|n| n.to_string()).unwrap_or(s)
}

fn curve_bits(curve_oid: &str) -> Option<usize> {
    Some(match curve_oid {
        "1.2.840.10045.3.1.7" | "1.3.132.0.10" | "1.3.36.3.3.2.8.1.1.7" => 256,
        "1.3.132.0.34" | "1.3.36.3.3.2.8.1.1.11" => 384,
        "1.3.132.0.35" => 521,
        "1.3.36.3.3.2.8.1.1.13" => 512,
        _ => return None,
    })
}

/// Describe a SubjectPublicKeyInfo: algorithm, size, curve, fingerprints.
pub fn spki_details(spki: &SubjectPublicKeyInfo) -> Value {
    let alg_oid = spki.algorithm.algorithm.to_id_string();
    let mut alg = oid_name(&alg_oid).unwrap_or(&alg_oid).to_string();
    let mut bits: Option<usize> = None;
    let mut curve: Option<String> = None;
    let mut exponent: Option<u64> = None;
    match spki.parsed() {
        Ok(PublicKey::RSA(rsa)) => {
            bits = Some(rsa.key_size());
            exponent = rsa.try_exponent().ok();
        }
        Ok(PublicKey::EC(_)) => {
            if let Some(p) = &spki.algorithm.parameters {
                if let Ok(o) = p.as_oid() {
                    let cs = o.to_id_string();
                    bits = curve_bits(&cs);
                    curve = Some(oid_name(&cs).map(|s| s.to_string()).unwrap_or(cs));
                }
            }
        }
        Ok(PublicKey::DSA(y)) => {
            bits = Some(
                y.len()
                    .saturating_sub(if y.first() == Some(&0) { 1 } else { 0 })
                    * 8,
            )
        }
        _ => {}
    }
    match alg_oid.as_str() {
        "1.3.101.112" | "1.3.101.110" => bits = Some(256),
        "1.3.101.113" => bits = Some(456),
        "1.3.101.111" => bits = Some(448),
        _ => {}
    }
    if alg == "EC" {
        alg = "ECDSA / EC".into();
    }
    let raw = spki.raw;
    let strength = match (alg_oid.as_str(), bits) {
        ("1.2.840.113549.1.1.1", Some(b)) if b < 2048 => "weak (RSA < 2048 bits)",
        ("1.2.840.113549.1.1.1", Some(_)) => "ok",
        ("1.2.840.10040.4.1", _) => "legacy (DSA)",
        _ => "ok",
    };
    json!({
        "algorithm": alg,
        "algorithm_oid": alg_oid,
        "bits": bits,
        "curve": curve,
        "rsa_exponent": exponent,
        "strength": strength,
        "spki_sha256": colon_hex(&sha256(raw)),
        "spki_sha256_base64": b64(&sha256(raw)),
        "hpkp_pin": format!("pin-sha256=\"{}\"", b64(&sha256(raw))),
    })
}

fn general_name(g: &GeneralName) -> (String, String) {
    match g {
        GeneralName::DNSName(s) => ("DNS".into(), s.to_string()),
        GeneralName::RFC822Name(s) => ("email".into(), s.to_string()),
        GeneralName::URI(s) => ("URI".into(), s.to_string()),
        GeneralName::IPAddress(b) => (
            "IP".into(),
            match b.len() {
                4 => std::net::Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string(),
                16 => {
                    let mut a = [0u8; 16];
                    a.copy_from_slice(b);
                    std::net::Ipv6Addr::from(a).to_string()
                }
                _ => hex::encode(b),
            },
        ),
        GeneralName::DirectoryName(n) => ("dirName".into(), n.to_string()),
        GeneralName::RegisteredID(o) => ("RID".into(), o.to_id_string()),
        GeneralName::OtherName(o, _) => ("otherName".into(), o.to_id_string()),
        other => ("other".into(), format!("{other:?}")),
    }
}

fn name_fields(n: &X509Name) -> Value {
    let get = |it: &mut dyn Iterator<Item = &AttributeTypeAndValue>| -> Value {
        let v: Vec<String> = it
            .filter_map(|a| a.as_str().ok().map(|s| s.to_string()))
            .collect();
        match v.len() {
            0 => Value::Null,
            1 => json!(v[0]),
            _ => json!(v),
        }
    };
    json!({
        "common_name": get(&mut n.iter_common_name()),
        "organization": get(&mut n.iter_organization()),
        "organizational_unit": get(&mut n.iter_organizational_unit()),
        "country": get(&mut n.iter_country()),
        "state": get(&mut n.iter_state_or_province()),
        "locality": get(&mut n.iter_locality()),
    })
}

fn rfc3339(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_else(|| ts.to_string())
}

fn extension_details(
    exts: &mut dyn Iterator<Item = &ParsedExtension>,
    out: &mut serde_json::Map<String, Value>,
) {
    for ext in exts {
        match ext {
            ParsedExtension::SubjectAlternativeName(san) => {
                let names: Vec<Value> = san
                    .general_names
                    .iter()
                    .map(|g| {
                        let (k, v) = general_name(g);
                        json!(format!("{k}:{v}"))
                    })
                    .collect();
                out.insert("subject_alt_names".into(), json!(names));
            }
            ParsedExtension::KeyUsage(ku) => {
                let mut v = vec![];
                for (on, name) in [
                    (ku.digital_signature(), "digitalSignature"),
                    (ku.non_repudiation(), "nonRepudiation"),
                    (ku.key_encipherment(), "keyEncipherment"),
                    (ku.data_encipherment(), "dataEncipherment"),
                    (ku.key_agreement(), "keyAgreement"),
                    (ku.key_cert_sign(), "keyCertSign"),
                    (ku.crl_sign(), "cRLSign"),
                    (ku.encipher_only(), "encipherOnly"),
                    (ku.decipher_only(), "decipherOnly"),
                ] {
                    if on {
                        v.push(name);
                    }
                }
                out.insert("key_usage".into(), json!(v));
            }
            ParsedExtension::ExtendedKeyUsage(e) => {
                let mut v: Vec<String> = vec![];
                for (on, name) in [
                    (e.any, "any"),
                    (e.server_auth, "serverAuth"),
                    (e.client_auth, "clientAuth"),
                    (e.code_signing, "codeSigning"),
                    (e.email_protection, "emailProtection"),
                    (e.time_stamping, "timeStamping"),
                    (e.ocsp_signing, "OCSPSigning"),
                ] {
                    if on {
                        v.push(name.into());
                    }
                }
                v.extend(e.other.iter().map(|o| o.to_id_string()));
                out.insert("extended_key_usage".into(), json!(v));
            }
            ParsedExtension::BasicConstraints(bc) => {
                out.insert(
                    "basic_constraints".into(),
                    json!({ "ca": bc.ca, "path_len": bc.path_len_constraint }),
                );
            }
            ParsedExtension::SubjectKeyIdentifier(k) => {
                out.insert("subject_key_id".into(), json!(colon_hex(k.0)));
            }
            ParsedExtension::AuthorityKeyIdentifier(a) => {
                if let Some(k) = &a.key_identifier {
                    out.insert("authority_key_id".into(), json!(colon_hex(k.0)));
                }
            }
            ParsedExtension::AuthorityInfoAccess(aia) => {
                let v: Vec<Value> = aia
                    .accessdescs
                    .iter()
                    .map(|d| json!({ "method": oid_label(&d.access_method), "location": general_name(&d.access_location).1 }))
                    .collect();
                out.insert("authority_info_access".into(), json!(v));
            }
            ParsedExtension::CRLDistributionPoints(c) => {
                let mut v = vec![];
                for p in c.iter() {
                    if let Some(x509_parser::extensions::DistributionPointName::FullName(names)) =
                        &p.distribution_point
                    {
                        v.extend(names.iter().map(|g| general_name(g).1));
                    }
                }
                out.insert("crl_distribution_points".into(), json!(v));
            }
            ParsedExtension::CertificatePolicies(pols) => {
                let v: Vec<String> = pols.iter().map(|p| p.policy_id.to_id_string()).collect();
                out.insert("certificate_policies".into(), json!(v));
            }
            ParsedExtension::SCT(scts) => {
                out.insert("embedded_scts".into(), json!(scts.len()));
            }
            _ => {}
        }
    }
}

/// Full details of a DER certificate.
pub fn cert_details(der: &[u8]) -> ToolResult<Value> {
    let (_, cert) = X509Certificate::from_der(der)
        .map_err(|e| ToolError::input(format!("not a valid X.509 certificate: {e}")))?;
    let now = chrono::Utc::now().timestamp();
    let nb = cert.validity().not_before.timestamp();
    let na = cert.validity().not_after.timestamp();
    let mut m = serde_json::Map::new();
    m.insert("kind".into(), json!("certificate"));
    m.insert("subject".into(), json!(cert.subject().to_string()));
    m.insert("subject_fields".into(), name_fields(cert.subject()));
    m.insert("issuer".into(), json!(cert.issuer().to_string()));
    m.insert("issuer_fields".into(), name_fields(cert.issuer()));
    m.insert("version".into(), json!(cert.version().0 + 1));
    m.insert("serial".into(), json!(colon_hex(cert.raw_serial())));
    m.insert("not_before".into(), json!(rfc3339(nb)));
    m.insert("not_after".into(), json!(rfc3339(na)));
    m.insert("days_remaining".into(), json!((na - now).div_euclid(86400)));
    m.insert("validity_days".into(), json!((na - nb) / 86400));
    m.insert("expired".into(), json!(now > na));
    m.insert("not_yet_valid".into(), json!(now < nb));
    m.insert(
        "self_issued".into(),
        json!(cert.subject().as_raw() == cert.issuer().as_raw()),
    );
    m.insert("is_ca".into(), json!(cert.is_ca()));
    m.insert(
        "signature_algorithm".into(),
        json!(oid_label(&cert.signature_algorithm.algorithm)),
    );
    m.insert("public_key".into(), spki_details(cert.public_key()));
    m.insert("fingerprint_sha256".into(), json!(colon_hex(&sha256(der))));
    m.insert("fingerprint_sha1".into(), json!(colon_hex(&sha1(der))));
    let parsed: Vec<&ParsedExtension> = cert
        .extensions()
        .iter()
        .map(|e| e.parsed_extension())
        .collect();
    extension_details(&mut parsed.into_iter(), &mut m);
    if !m.contains_key("subject_alt_names") {
        m.insert("subject_alt_names".into(), json!([]));
    }
    let sig = m["signature_algorithm"]
        .as_str()
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut warnings = vec![];
    if sig.contains("sha1") || sig.contains("md5") {
        warnings.push(format!("weak signature algorithm ({sig})"));
    }
    if now > na {
        warnings.push("certificate has expired".to_string());
    } else if na - now < 30 * 86400 {
        warnings.push("certificate expires within 30 days".to_string());
    }
    if m["public_key"]["strength"]
        .as_str()
        .map(|s| s != "ok")
        .unwrap_or(false)
    {
        warnings.push(format!(
            "public key: {}",
            m["public_key"]["strength"].as_str().unwrap()
        ));
    }
    m.insert("warnings".into(), json!(warnings));
    Ok(Value::Object(m))
}

/// Details of a DER PKCS#10 certificate signing request.
pub fn csr_details(der: &[u8]) -> ToolResult<Value> {
    let (_, csr) = X509CertificationRequest::from_der(der)
        .map_err(|e| ToolError::input(format!("not a valid certificate request: {e}")))?;
    let info = &csr.certification_request_info;
    let mut m = serde_json::Map::new();
    m.insert("kind".into(), json!("certificate_request"));
    m.insert("subject".into(), json!(info.subject.to_string()));
    m.insert("subject_fields".into(), name_fields(&info.subject));
    m.insert("version".into(), json!(info.version.0 + 1));
    m.insert(
        "signature_algorithm".into(),
        json!(oid_label(&csr.signature_algorithm.algorithm)),
    );
    m.insert("public_key".into(), spki_details(&info.subject_pki));
    m.insert("fingerprint_sha256".into(), json!(colon_hex(&sha256(der))));
    if let Some(exts) = csr.requested_extensions() {
        let v: Vec<&ParsedExtension> = exts.collect();
        extension_details(&mut v.into_iter(), &mut m);
    }
    if !m.contains_key("subject_alt_names") {
        m.insert("subject_alt_names".into(), json!([]));
    }
    Ok(Value::Object(m))
}

/// Short one-line summary of a certificate details value.
pub fn summary_line(d: &Value) -> String {
    let cn = d["subject_fields"]["common_name"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| d["subject"].as_str().unwrap_or("").to_string());
    let pk = &d["public_key"];
    let key = format!(
        "{}{}",
        pk["algorithm"].as_str().unwrap_or("?"),
        pk["bits"]
            .as_u64()
            .map(|b| format!(" {b}"))
            .unwrap_or_default()
    );
    if d["kind"] == "certificate" {
        format!(
            "{cn} — issued by {} — valid until {} ({} days) — {key}",
            d["issuer_fields"]["common_name"]
                .as_str()
                .or(d["issuer"].as_str())
                .unwrap_or("?"),
            d["not_after"].as_str().unwrap_or("?"),
            d["days_remaining"]
        )
    } else {
        format!("CSR for {cn} — {key}")
    }
}

fn cert_decode() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.cert_decode", "Certificate Decoder", Category::Network)
        .describe("Decode PEM/DER X.509 certificates, chains and CSRs: subject, issuer, SANs, validity, key, fingerprints, extensions")
        .keywords(&["x509", "ssl", "tls", "certificate", "pem", "der", "csr", "decode", "san", "fingerprint", "chain"])
        .param(ParamSpec::textarea("pem").label("PEM text").describe("Paste one or more -----BEGIN CERTIFICATE----- / CERTIFICATE REQUEST blocks").primary())
        .param(ParamSpec::file("file").label("…or certificate file").accept(&[".pem", ".crt", ".cer", ".der", ".csr", ".req", ".p7b"]))
        .ui("live");
    sync_tool(spec, |_ctx, p| {
        let mut ders: Vec<(String, Vec<u8>)> = vec![];
        let mut text = p.text("pem").to_string();
        if let Some(f) = p.opt_file("file") {
            let bytes = std::fs::read(&f.path)?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err(ToolError::invalid(
                    "file",
                    "file too large for a certificate",
                ));
            }
            match std::str::from_utf8(&bytes) {
                Ok(s) if s.contains("-----BEGIN") => {
                    text.push('\n');
                    text.push_str(s);
                }
                _ => ders.push(("DER".into(), bytes)),
            }
        }
        for b in parse_pem(&text) {
            ders.push((b.label, b.der));
        }
        if ders.is_empty() {
            // Bare base64 DER without PEM armor?
            if let Some(der) =
                crate::util::b64_decode(text.trim()).filter(|d| d.first() == Some(&0x30))
            {
                ders.push(("DER".into(), der));
            }
        }
        if ders.is_empty() {
            return Err(ToolError::input(
                "no certificate found: paste a PEM block or upload a DER/PEM file",
            ));
        }
        let mut items = vec![];
        let mut errors = vec![];
        for (label, der) in &ders {
            let r = match label.as_str() {
                "CERTIFICATE REQUEST" | "NEW CERTIFICATE REQUEST" => csr_details(der),
                "CERTIFICATE" | "TRUSTED CERTIFICATE" | "X509 CERTIFICATE" => cert_details(der),
                "DER" => cert_details(der).or_else(|_| csr_details(der)),
                l if l.contains("PRIVATE KEY") => Err(ToolError::input("this is a private key — use network.key_inspect (key material is never echoed)")),
                l => Err(ToolError::input(format!("unsupported PEM block '{l}'"))),
            };
            match r {
                Ok(v) => items.push(v),
                Err(e) => errors.push(e.to_string()),
            }
        }
        if items.is_empty() {
            return Err(ToolError::input(errors.join("; ")));
        }
        // Chain linkage: does each certificate's issuer match the next subject?
        let certs: Vec<&Value> = items
            .iter()
            .filter(|v| v["kind"] == "certificate")
            .collect();
        let chain: Vec<Value> = certs
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let next = certs.get(i + 1);
                json!({
                    "position": i,
                    "subject": c["subject"],
                    "issuer": c["issuer"],
                    "issued_by_next": next.map(|n| n["subject"] == c["issuer"]),
                    "akid_matches_next_skid": next.and_then(|n| match (c.get("authority_key_id"), n.get("subject_key_id")) {
                        (Some(a), Some(s)) => Some(a == s),
                        _ => None,
                    }),
                })
            })
            .collect();
        let text = items
            .iter()
            .map(summary_line)
            .collect::<Vec<_>>()
            .join("\n");
        let rows: Vec<Vec<Value>> = items
            .iter()
            .map(|d| {
                vec![
                    d["kind"].clone(),
                    d["subject"].clone(),
                    d.get("issuer").cloned().unwrap_or(Value::Null),
                    d.get("not_after").cloned().unwrap_or(Value::Null),
                    d["public_key"]["algorithm"].clone(),
                    d["public_key"]["bits"].clone(),
                ]
            })
            .collect();
        let data = if items.len() == 1 {
            let mut d = items.into_iter().next().unwrap();
            d["errors"] = json!(errors);
            d
        } else {
            json!({ "count": items.len(), "items": items, "chain": chain, "errors": errors, "table": table(&["kind", "subject", "issuer", "not after", "key", "bits"], rows) })
        };
        Ok(ToolOutput::text(text).with_data(data))
    })
}
