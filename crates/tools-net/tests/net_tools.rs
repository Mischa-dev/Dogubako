//! Integration tests for dogubako-tools-net. Everything here runs offline
//! against loopback servers; tests needing the public internet are
//! `#[ignore]`d (run with `cargo test -p dogubako-tools-net -- --ignored`).

use dogubako_core::engine::{Engine, EngineConfig, RunRequest};
use dogubako_core::error::ToolResult;
use dogubako_core::params::LocalFileResolver;
use dogubako_core::policy::{NetPolicy, PermissionSet};
use dogubako_core::registry::Registry;
use dogubako_core::testing::TestHarness;
use dogubako_core::tool::{Environment, ToolOutput};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

fn harness() -> TestHarness {
    TestHarness::new(dogubako_tools_net::tools())
}

// ---------------------------------------------------------------------------
// Engines with a custom network policy

struct PolicyEngine {
    engine: Engine,
    dir: tempfile::TempDir,
}

impl PolicyEngine {
    fn new(net: NetPolicy) -> Self {
        let mut reg = Registry::new();
        reg.register_all(dogubako_tools_net::tools());
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = EngineConfig::local(Environment::Server);
        cfg.net = net;
        cfg.temp_root = dir.path().join("tmp");
        PolicyEngine {
            engine: Engine::new(reg, cfg, vec![]),
            dir,
        }
    }

    async fn run(&self, id: &str, params: Value) -> ToolResult<ToolOutput> {
        let resolver = LocalFileResolver::unrestricted();
        self.engine
            .run(RunRequest {
                tool_id: id,
                params: params.as_object().cloned().unwrap_or_default(),
                resolver: &resolver,
                out_dir: self
                    .dir
                    .path()
                    .join(format!("out-{}", dogubako_core::util::random_id(4))),
                permissions: PermissionSet::all(),
                authorize: None,
                progress: None,
                cancel: Default::default(),
                timeout: None,
                fs: None,
            })
            .await
            .map(|(o, _)| o)
    }
}

// ---------------------------------------------------------------------------
// Tiny loopback HTTP server

type Route = Arc<dyn Fn(&str) -> (u16, Vec<(String, String)>, Vec<u8>) + Send + Sync>;

async fn spawn_http(route: Route) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut s, _)) = l.accept().await else {
                break;
            };
            let route = route.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let mut n = 0;
                while n < buf.len() {
                    match s.read(&mut buf[n..]).await {
                        Ok(0) | Err(_) => return,
                        Ok(k) => n += k,
                    }
                    if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let is_head = req.starts_with("HEAD");
                let (status, headers, body) = route(&path);
                let mut resp = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                    body.len()
                );
                for (k, v) in headers {
                    resp.push_str(&format!("{k}: {v}\r\n"));
                }
                resp.push_str("\r\n");
                let mut bytes = resp.into_bytes();
                if !is_head {
                    bytes.extend(body);
                }
                let _ = s.write_all(&bytes).await;
                let _ = s.shutdown().await;
            });
        }
    });
    port
}

fn h(k: &str, v: &str) -> (String, String) {
    (k.to_string(), v.to_string())
}

fn png(w: u32, hgt: u32) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    b.extend(w.to_be_bytes());
    b.extend(hgt.to_be_bytes());
    b.extend([8, 6, 0, 0, 0, 0, 0, 0, 0]);
    b
}

// ---------------------------------------------------------------------------
// Pure tools

#[tokio::test]
async fn url_tools() {
    let h = harness();
    let out = h
        .run(
            "network.url_parse",
            json!({"url": "https://user:pw@BÜCHER.example:8080/a%20b/c?x=1&y=%C3%A9&x=2#frag"}),
        )
        .await
        .unwrap();
    let d = &out.data;
    assert_eq!(d["scheme"], "https");
    assert_eq!(d["host"], "xn--bcher-kva.example");
    assert_eq!(d["host_unicode"], "bücher.example");
    assert_eq!(d["port"], 8080);
    assert_eq!(d["has_password"], true);
    assert_eq!(d["path_segments"][0], "a b");
    assert_eq!(d["fragment"], "frag");
    assert_eq!(d["table"]["rows"].as_array().unwrap().len(), 3);
    assert_eq!(d["table"]["rows"][1][2], "é");
    let out = h
        .run("network.url_parse", json!({"url": "example.com/path"}))
        .await
        .unwrap();
    assert_eq!(out.data["scheme_assumed"], true);
    assert_eq!(out.data["host"], "example.com");
    assert_eq!(
        h.run("network.url_parse", json!({"url": "http://[::1"}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );

    let out = h
        .run("network.url_normalize", json!({"urls": "HTTPS://WWW.Example.com:443/a/../b/?utm_source=nl&z=1&a=2&gclid=x#\nnot a url ::", "remove_www": true}))
        .await
        .unwrap();
    let text = out.text.unwrap();
    assert_eq!(
        text.lines().next().unwrap(),
        "https://example.com/b/?a=2&z=1"
    );
    assert!(text.lines().nth(1).unwrap().starts_with("# invalid"));

    let out = h.run("network.query_params", json!({"url": "https://x.test/p?a=1&utm_x=2&b=3", "set": "{\"a\": \"9\", \"c\": [\"x\", \"y\"], \"b\": null}", "remove": "utm_*"})).await.unwrap();
    assert_eq!(out.text.unwrap(), "https://x.test/p?a=9&c=x&c=y");
    let out = h
        .run(
            "network.query_params",
            json!({"url": "https://x.test/?q=old", "set": "q=new value\npage=2"}),
        )
        .await
        .unwrap();
    assert_eq!(out.text.unwrap(), "https://x.test/?q=new+value&page=2");
}

#[tokio::test]
async fn ip_tools() {
    let h = harness();
    let out = h
        .run("network.ip_info", json!({"ip": "10.1.2.3"}))
        .await
        .unwrap();
    assert_eq!(out.data["integer"], 167838211u32);
    assert_eq!(out.data["is_public"], false);
    assert_eq!(out.data["reverse_pointer"], "3.2.1.10.in-addr.arpa");
    assert_eq!(out.data["binary"], "00001010.00000001.00000010.00000011");
    let out = h
        .run("network.ip_info", json!({"ip": "2606:4700:4700::1111"}))
        .await
        .unwrap();
    assert_eq!(out.data["is_public"], true);
    assert_eq!(
        out.data["expanded"],
        "2606:4700:4700:0000:0000:0000:0000:1111"
    );
    let out = h
        .run("network.ip_info", json!({"ip": "::ffff:192.168.0.1"}))
        .await
        .unwrap();
    assert_eq!(out.data["embedded_ipv4"], "192.168.0.1");
    assert_eq!(
        h.run("network.ip_info", json!({"ip": "300.1.1.1"}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );

    let out = h
        .run("network.cidr", json!({"cidr": "172.16.5.4/20"}))
        .await
        .unwrap();
    assert_eq!(out.data["network"], "172.16.0.0");
    assert_eq!(out.data["broadcast"], "172.16.15.255");
    assert_eq!(out.data["usable_hosts"], 4094);
    assert!(!out.warnings.is_empty(), "host bits warning");
    let out = h.run("network.cidr", json!({"cidr": "10.0.0.0/8\n192.168.0.0/16", "action": "contains", "address": "192.168.44.1"})).await.unwrap();
    assert_eq!(out.data["contained"], true);
    let out = h
        .run(
            "network.cidr",
            json!({"cidr": "192.168.0.0/24", "action": "split", "new_prefix": 26}),
        )
        .await
        .unwrap();
    let rows = out.data["table"]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[3][1], "192.168.0.192/26");
    assert_eq!(rows[3][4], "192.168.0.255");
    let out = h.run("network.cidr", json!({"cidr": "10.0.0.0/24\n10.0.1.0/24\n10.0.2.0/23\n10.0.0.128/25", "action": "summarize"})).await.unwrap();
    assert_eq!(out.text.unwrap(), "10.0.0.0/22");
    let out = h
        .run(
            "network.cidr",
            json!({"cidr": "2001:db8::/32", "action": "split", "new_prefix": 48}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["subnet_count"], "65536");
    assert_eq!(out.warnings.len(), 1);
    assert_eq!(
        h.run(
            "network.cidr",
            json!({"cidr": "10.0.0.0/24", "action": "split", "new_prefix": 16})
        )
        .await
        .unwrap_err()
        .code(),
        "invalid_input"
    );
    assert_eq!(
        h.run("network.cidr", json!({"cidr": "10.0.0.0/33"}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );

    let out = h.run("network.subnet_plan", json!({"parent": "192.168.10.0/24", "requirements": "Office: 100\nLab: 50\nVoIP: 20\nLink: 2"})).await.unwrap();
    let rows = out.data["table"]["rows"].as_array().unwrap();
    assert_eq!(rows[0][0], "Office");
    assert_eq!(rows[0][2], "192.168.10.0/25");
    assert_eq!(rows[1][2], "192.168.10.128/26");
    assert_eq!(rows[2][2], "192.168.10.192/27");
    assert_eq!(rows[3][2], "192.168.10.224/31");
    assert_eq!(out.data["unallocated"][0], "192.168.10.226/31");
    assert!(out.warnings.is_empty());
    let out = h.run("network.subnet_plan", json!({"parent": "10.0.0.0/28", "requirements": "[{\"name\":\"big\",\"hosts\":100},{\"name\":\"small\",\"hosts\":5}]"})).await.unwrap();
    assert_eq!(out.data["allocated"].as_array().unwrap().len(), 1);
    assert!(out.warnings[0].contains("big"));
}

#[tokio::test]
async fn cert_decode_rcgen() {
    let h = harness();
    let mut params = rcgen::CertificateParams::new(vec![
        "test.example".to_string(),
        "www.test.example".to_string(),
    ])
    .unwrap();
    params
        .subject_alt_names
        .push(rcgen::SanType::IpAddress("192.0.2.7".parse().unwrap()));
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "test.example");
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, "Dogubako Test");
    let kp = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&kp).unwrap();
    let out = h
        .run("network.cert_decode", json!({"pem": cert.pem()}))
        .await
        .unwrap();
    let d = &out.data;
    assert_eq!(d["kind"], "certificate");
    assert_eq!(d["subject_fields"]["common_name"], "test.example");
    assert_eq!(d["subject_fields"]["organization"], "Dogubako Test");
    assert_eq!(d["self_issued"], true);
    let sans: Vec<&str> = d["subject_alt_names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(sans.contains(&"DNS:www.test.example"));
    assert!(sans.contains(&"IP:192.0.2.7"));
    assert_eq!(d["public_key"]["algorithm"], "ECDSA / EC");
    assert_eq!(d["public_key"]["bits"], 256);
    assert_eq!(d["signature_algorithm"], "ecdsa-with-SHA256");
    assert_eq!(d["fingerprint_sha256"].as_str().unwrap().len(), 32 * 3 - 1);
    assert!(d["days_remaining"].as_i64().unwrap() > 300);

    // DER file upload
    let der = h.write("cert.der", cert.der());
    let out = h
        .run(
            "network.cert_decode",
            json!({"file": der.to_string_lossy()}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["fingerprint_sha256"], d["fingerprint_sha256"]);

    // CSR + chain of two certs
    let csr = params.serialize_request(&kp).unwrap();
    let ca_kp = rcgen::KeyPair::generate().unwrap();
    let mut ca_params = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "Test CA");
    let ca = ca_params.self_signed(&ca_kp).unwrap();
    let issuer = rcgen::Issuer::new(ca_params, ca_kp);
    let leaf = params.signed_by(&kp, &issuer).unwrap();
    let text = format!("{}{}{}", leaf.pem(), ca.pem(), csr.pem().unwrap());
    let out = h
        .run("network.cert_decode", json!({"pem": text}))
        .await
        .unwrap();
    assert_eq!(out.data["count"], 3);
    assert_eq!(out.data["items"][2]["kind"], "certificate_request");
    assert_eq!(out.data["chain"][0]["issued_by_next"], true);
    assert_eq!(out.data["items"][1]["basic_constraints"]["ca"], true);

    assert_eq!(
        h.run("network.cert_decode", json!({"pem": "hello"}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );
    assert_eq!(
        h.run(
            "network.cert_decode",
            json!({"pem": "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----"})
        )
        .await
        .unwrap_err()
        .code(),
        "invalid_input"
    );
}

fn body_of(pem: &str) -> String {
    pem.lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<Vec<_>>()
        .join("")
}

/// No 24-char window of the private key's base64 may appear in the output,
/// except windows that also occur in the public key encoding (private key
/// containers embed the public key, which is legitimately shown).
fn assert_no_secret(out: &ToolOutput, private_pem: &str, public_pem: &str) {
    let body = body_of(private_pem);
    let public = body_of(public_pem);
    let s = serde_json::to_string(out).unwrap();
    for i in 0..body.len().saturating_sub(24) {
        let w = &body[i..i + 24];
        assert!(
            !s.contains(w) || public.contains(w),
            "private key material leaked: {w}"
        );
    }
}

#[tokio::test]
async fn key_inspect_keys() {
    let h = harness();
    // ECDSA P-256 PKCS#8
    let kp = rcgen::KeyPair::generate().unwrap();
    let priv_pem = kp.serialize_pem();
    let out = h
        .run("network.key_inspect", json!({"key": priv_pem}))
        .await
        .unwrap();
    assert_eq!(out.data["kind"], "private");
    assert_eq!(out.data["bits"], 256);
    assert_eq!(out.data["curve"], "P-256 (secp256r1)");
    assert_no_secret(&out, &priv_pem, &kp.public_key_pem());
    // The fingerprint of the private key equals that of its public key.
    let pub_pem = kp.public_key_pem();
    let out_pub = h
        .run("network.key_inspect", json!({"key": pub_pem}))
        .await
        .unwrap();
    assert_eq!(out_pub.data["kind"], "public");
    assert_eq!(out_pub.data["spki_sha256"], out.data["spki_sha256"]);

    // Ed25519 PKCS#8: derived public key must match rcgen's.
    let ed = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).unwrap();
    let out = h
        .run("network.key_inspect", json!({"key": ed.serialize_pem()}))
        .await
        .unwrap();
    assert_eq!(out.data["algorithm"], "Ed25519");
    assert_eq!(out.data["bits"], 256);
    assert_eq!(
        out.data["public_key_pem"].as_str().unwrap().trim(),
        // rcgen emits CRLF line endings on Windows; the tool always uses LF.
        ed.public_key_pem().replace("\r\n", "\n").trim()
    );
    assert_no_secret(&out, &ed.serialize_pem(), &ed.public_key_pem());

    // SSH public key (built from raw bytes) incl. authorized_keys options.
    let key = ssh_key::PublicKey::new(
        ssh_key::public::KeyData::Ed25519(ssh_key::public::Ed25519PublicKey([7u8; 32])),
        "me@host",
    );
    let line = format!("no-pty,from=\"10.0.0.0/8\" {}", key.to_openssh().unwrap());
    let out = h
        .run("network.key_inspect", json!({"key": line}))
        .await
        .unwrap();
    assert_eq!(out.data["ssh_algorithm"], "ssh-ed25519");
    assert_eq!(out.data["comment"], "me@host");
    assert_eq!(
        out.data["authorized_keys_options"],
        "no-pty,from=\"10.0.0.0/8\""
    );
    let blob = key.to_bytes().unwrap();
    use base64::Engine as _;
    use sha2::Digest as _;
    let expect = format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(sha2::Sha256::digest(&blob))
    );
    assert_eq!(out.data["ssh_fingerprint_sha256"], expect);

    // RSA via openssl if available (PKCS#1 traditional + PKCS#8 + public).
    let dir = tempfile::tempdir().unwrap();
    let k8 = dir.path().join("k8.pem");
    let ok = std::process::Command::new("openssl")
        .args([
            "genpkey",
            "-algorithm",
            "RSA",
            "-pkeyopt",
            "rsa_keygen_bits:2048",
            "-out",
        ])
        .arg(&k8)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if ok {
        let k1 = dir.path().join("k1.pem");
        let kp = dir.path().join("kp.pem");
        std::process::Command::new("openssl")
            .args(["rsa", "-traditional", "-in"])
            .arg(&k8)
            .arg("-out")
            .arg(&k1)
            .output()
            .unwrap();
        std::process::Command::new("openssl")
            .args(["rsa", "-pubout", "-in"])
            .arg(&k8)
            .arg("-out")
            .arg(&kp)
            .output()
            .unwrap();
        let p8 = std::fs::read_to_string(&k8).unwrap();
        let p1 = std::fs::read_to_string(&k1).unwrap();
        let pp = std::fs::read_to_string(&kp).unwrap();
        assert!(p1.contains("BEGIN RSA PRIVATE KEY"));
        let a = h
            .run("network.key_inspect", json!({"key": p8}))
            .await
            .unwrap();
        let b = h
            .run("network.key_inspect", json!({"key": p1}))
            .await
            .unwrap();
        let c = h
            .run("network.key_inspect", json!({"file": kp.to_string_lossy()}))
            .await
            .unwrap();
        assert_eq!(a.data["bits"], 2048);
        assert_eq!(a.data["rsa_exponent"], 65537);
        assert_eq!(a.data["spki_sha256"], b.data["spki_sha256"]);
        assert_eq!(a.data["spki_sha256"], c.data["spki_sha256"]);
        // openssl writes CRLF on Windows; the tool always emits LF.
        assert_eq!(
            c.data["public_key_pem"].as_str().unwrap().trim(),
            pp.replace("\r\n", "\n").trim()
        );
        assert_no_secret(&a, &p8, &pp);
        assert_no_secret(&b, &p1, &pp);
    } else {
        eprintln!("openssl not available; skipping RSA key checks");
    }

    // Encrypted PKCS#8 is recognised without a passphrase.
    let enc = "-----BEGIN ENCRYPTED PRIVATE KEY-----\nMIGbMFcGCSqGSIb3DQEFDTBKMCkGCSqGSIb3DQEFDDAcBAgAAAAAAAAAAAICCAAwDAYIKoZIhvcNAgkFADAdBglghkgBZQMEASoEEAAAAAAAAAAAAAAAAAAAAAAEQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n-----END ENCRYPTED PRIVATE KEY-----\n";
    let out = h
        .run("network.key_inspect", json!({"key": enc}))
        .await
        .unwrap();
    assert_eq!(out.data["encrypted"], true);
    assert_eq!(out.data["encryption_scheme"], "PBES2");

    assert_eq!(
        h.run("network.key_inspect", json!({"key": "nothing here"}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );
}

// ---------------------------------------------------------------------------
// SSRF protection (hosted policy)

#[tokio::test]
async fn ssrf_blocked_in_public_only_mode() {
    let port = spawn_http(Arc::new(|_| (200, vec![], b"ok".to_vec()))).await;
    let e = PolicyEngine::new(NetPolicy::public_only());
    let denied = |r: ToolResult<ToolOutput>, what: &str| {
        let err = r.expect_err(&format!("{what} must be denied"));
        assert_eq!(err.code(), "permission_denied", "{what}: {err}");
    };
    denied(
        e.run(
            "network.http_headers",
            json!({"url": format!("http://127.0.0.1:{port}/")}),
        )
        .await,
        "http 127.0.0.1",
    );
    denied(
        e.run(
            "network.http_headers",
            json!({"url": format!("http://localhost:{port}/")}),
        )
        .await,
        "http localhost",
    );
    denied(
        e.run(
            "network.http_headers",
            json!({"url": "http://169.254.169.254/latest/meta-data/"}),
        )
        .await,
        "metadata IP",
    );
    denied(
        e.run("network.http_headers", json!({"url": "http://[::1]/"}))
            .await,
        "ipv6 loopback",
    );
    denied(
        e.run(
            "network.http_headers",
            json!({"url": "http://metadata.google.internal/"}),
        )
        .await,
        ".internal name",
    );
    denied(
        e.run(
            "network.favicon",
            json!({"url": format!("http://127.0.0.1:{port}/")}),
        )
        .await,
        "favicon",
    );
    denied(
        e.run(
            "network.tls_inspect",
            json!({"host": format!("127.0.0.1:{port}")}),
        )
        .await,
        "tls",
    );
    denied(
        e.run(
            "network.port_check",
            json!({"host": "127.0.0.1", "preset": "custom", "ports": port.to_string()}),
        )
        .await,
        "port check",
    );
    denied(
        e.run("network.port_check", json!({"host": "10.0.0.1"}))
            .await,
        "port check private range",
    );
    denied(
        e.run(
            "network.whois",
            json!({"query": "example.com", "server": "127.0.0.1"}),
        )
        .await,
        "whois server",
    );
    denied(
        e.run("network.dns_lookup", json!({"name": "localhost"}))
            .await,
        "dns localhost",
    );
    denied(
        e.run("network.dns_lookup", json!({"name": "192.168.1.1"}))
            .await,
        "ptr private",
    );
    denied(
        e.run("network.reverse_dns", json!({"ips": "10.0.0.1"}))
            .await,
        "reverse private",
    );
    denied(
        e.run("network.rdap", json!({"query": "printer.local"}))
            .await,
        "rdap .local",
    );
    denied(
        e.run(
            "network.domain_inspect",
            json!({"domain": "router.internal"}),
        )
        .await,
        "domain_inspect",
    );
    // connectivity reports per-target failures instead of failing the run
    let out = e.run("network.connectivity", json!({"targets": format!("127.0.0.1:{port}\nhttp://127.0.0.1:{port}/"), "attempts": 1})).await.unwrap();
    assert_eq!(out.data["reachable"], 0);
    for r in out.data["results"].as_array().unwrap() {
        assert!(r["error"].as_str().unwrap().contains("not public"), "{r}");
    }

    // A public host name that resolves to 127.0.0.1 (DNS-rebinding style).
    if tokio::net::lookup_host(("localtest.me", 80))
        .await
        .map(|mut a| a.any(|x| x.ip().is_loopback()))
        .unwrap_or(false)
    {
        denied(
            e.run(
                "network.http_headers",
                json!({"url": format!("http://localtest.me:{port}/")}),
            )
            .await,
            "localtest.me",
        );
        denied(
            e.run(
                "network.tls_inspect",
                json!({"host": format!("localtest.me:{port}")}),
            )
            .await,
            "localtest.me tls",
        );
    } else {
        eprintln!("localtest.me not resolvable here; skipping hostname→loopback check");
    }
}

#[tokio::test]
async fn redirects_to_blocked_hosts_are_refused() {
    let port = spawn_http(Arc::new(|path: &str| match path {
        "/meta" => (
            302,
            vec![h("Location", "http://169.254.169.254/latest/meta-data/")],
            vec![],
        ),
        "/local" => (301, vec![h("Location", "/next")], vec![]),
        "/next" => (302, vec![h("Location", "http://localhost/admin")], vec![]),
        _ => (200, vec![], b"ok".to_vec()),
    }))
    .await;
    // Loopback allowed for the test server itself, but the redirect targets are denied.
    let e = PolicyEngine::new(NetPolicy {
        allow_private: true,
        deny_hosts: vec!["169.254.169.254".into(), "localhost".into()],
    });
    let out = e
        .run(
            "network.http_headers",
            json!({"url": format!("http://127.0.0.1:{port}/meta")}),
        )
        .await
        .unwrap();
    assert_eq!(
        out.data["redirects"].as_array().unwrap().len(),
        1,
        "must not follow the redirect"
    );
    assert!(out.data["stopped"].as_str().unwrap().contains("refused"));
    let out = e
        .run(
            "network.http_headers",
            json!({"url": format!("http://127.0.0.1:{port}/local")}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["redirects"].as_array().unwrap().len(), 2);
    assert!(out.data["stopped"].as_str().unwrap().contains("localhost"));
    // Core's guarded client (used by favicon/rdap fetches) also refuses.
    let err = e
        .run(
            "network.favicon",
            json!({"url": format!("http://127.0.0.1:{port}/meta")}),
        )
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("blocked") || err.to_string().contains("redirect"),
        "{err}"
    );
}

// ---------------------------------------------------------------------------
// Functional tests against loopback servers (default local policy)

#[tokio::test]
async fn http_headers_local() {
    let port = spawn_http(Arc::new(|path: &str| match path {
        "/start" => (301, vec![h("Location", "/final")], vec![]),
        "/final" => (
            200,
            vec![
                h("Content-Type", "text/html"),
                h(
                    "Content-Security-Policy",
                    "default-src 'self'; frame-ancestors 'none'",
                ),
                h("X-Content-Type-Options", "nosniff"),
                h("Referrer-Policy", "no-referrer"),
                h("Set-Cookie", "session=abc; HttpOnly; SameSite=Lax"),
                h("X-Powered-By", "PHP/8.1"),
            ],
            b"<html></html>".to_vec(),
        ),
        "/loop" => (302, vec![h("Location", "/loop")], vec![]),
        _ => (404, vec![], vec![]),
    }))
    .await;
    let hns = harness();
    let out = hns
        .run(
            "network.http_headers",
            json!({"url": format!("http://127.0.0.1:{port}/start")}),
        )
        .await
        .unwrap();
    let d = &out.data;
    assert_eq!(d["status"], 200);
    assert_eq!(d["redirect_count"], 1);
    assert_eq!(d["redirects"][0]["status"], 301);
    assert!(d["final_url"].as_str().unwrap().ends_with("/final"));
    let sec = d["security"].as_array().unwrap();
    let status_of = |name: &str| {
        sec.iter()
            .find(|s| s["header"] == name)
            .map(|s| s["status"].as_str().unwrap().to_string())
    };
    assert_eq!(
        status_of("Content-Security-Policy").as_deref(),
        Some("good")
    );
    assert_eq!(status_of("X-Frame-Options").as_deref(), Some("good")); // via frame-ancestors
    assert_eq!(status_of("Permissions-Policy").as_deref(), Some("missing"));
    assert_eq!(status_of("x-powered-by").as_deref(), Some("info"));
    assert_eq!(d["cookies"][0]["http_only"], true);
    assert!(d["table"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r[0] == "x-content-type-options"));
    assert_eq!(out.text_mime.as_deref(), Some("text/markdown"));

    let out = hns
        .run(
            "network.http_headers",
            json!({"url": format!("http://127.0.0.1:{port}/loop")}),
        )
        .await
        .unwrap();
    assert!(out.data["stopped"].as_str().unwrap().contains("loop"));
    let out = hns.run("network.http_headers", json!({"url": format!("http://127.0.0.1:{port}/start"), "method": "HEAD", "max_redirects": 0})).await.unwrap();
    assert!(out.data["stopped"].as_str().unwrap().contains("redirects"));
    // Closed port → clean failure
    let closed = TcpListener::bind("127.0.0.1:0")
        .await
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    assert_eq!(
        hns.run(
            "network.http_headers",
            json!({"url": format!("http://127.0.0.1:{closed}/")})
        )
        .await
        .unwrap_err()
        .code(),
        "failed"
    );
    assert_eq!(
        hns.run("network.http_headers", json!({"url": "ftp://example.com/"}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );
}

#[tokio::test]
async fn favicon_local() {
    let port = spawn_http(Arc::new(|path: &str| match path {
        "/" => (200, vec![h("Content-Type", "text/html")], br#"<html><head><link rel="icon" sizes="32x32" href="/i32.png"><link rel="apple-touch-icon" href="touch.png"><link rel="manifest" href="/site.webmanifest"><link rel="icon" href="/missing.png"></head></html>"#.to_vec()),
        "/i32.png" => (200, vec![h("Content-Type", "image/png")], png(32, 32)),
        "/touch.png" => (200, vec![h("Content-Type", "image/png")], png(180, 180)),
        "/site.webmanifest" => (200, vec![h("Content-Type", "application/manifest+json")], br#"{"icons":[{"src":"/m512.png","sizes":"512x512"}]}"#.to_vec()),
        "/m512.png" => (200, vec![h("Content-Type", "image/png")], png(512, 512)),
        "/favicon.ico" => (200, vec![h("Content-Type", "image/x-icon")], vec![0, 0, 1, 0, 1, 0, 16, 16, 0, 0, 1, 0, 32, 0, 0, 0, 0, 0, 22, 0, 0, 0]),
        _ => (404, vec![], b"nope".to_vec()),
    }))
    .await;
    let hns = harness();
    let out = hns
        .run(
            "network.favicon",
            json!({"url": format!("http://127.0.0.1:{port}/")}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["icons_found"], 4, "{}", out.data);
    assert_eq!(out.artifacts.len(), 4);
    let names: Vec<&str> = out.artifacts.iter().map(|a| a.name.as_str()).collect();
    assert!(
        names
            .iter()
            .any(|n| n.contains("512x512") && n.ends_with(".png")),
        "{names:?}"
    );
    assert!(names.iter().any(|n| n.ends_with("16x16.ico")), "{names:?}");
    let rows = out.data["table"]["rows"].as_array().unwrap();
    assert!(rows.iter().any(|r| r[7] == "HTTP 404"));
}

#[tokio::test]
async fn tls_inspect_local() {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_der = rustls::pki_types::CertificateDer::from(ck.cert.der().to_vec());
    let key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(
        rustls::pki_types::PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der()),
    );
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let mut cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .unwrap();
    cfg.alpn_protocols = vec![b"h2".to_vec()];
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((s, _)) = l.accept().await {
            let a = acceptor.clone();
            tokio::spawn(async move {
                if let Ok(mut t) = a.accept(s).await {
                    let mut b = [0u8; 16];
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(2), t.read(&mut b))
                        .await;
                }
            });
        }
    });
    let hns = harness();
    let out = hns
        .run(
            "network.tls_inspect",
            json!({"host": format!("localhost:{port}")}),
        )
        .await
        .unwrap();
    let d = &out.data;
    assert_eq!(d["protocol"], "TLS 1.3");
    assert_eq!(d["alpn"], "h2");
    assert_eq!(
        d["trust"]["trusted"], false,
        "self-signed must not be reported as trusted"
    );
    assert!(d["trust"]["error"].as_str().unwrap().len() > 3);
    assert_eq!(d["chain"].as_array().unwrap().len(), 1);
    assert!(d["certificate"]["subject_alt_names"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s == "DNS:localhost"));
    assert_eq!(
        d["certificate"]["fingerprint_sha256"]
            .as_str()
            .unwrap()
            .replace(':', "")
            .to_lowercase(),
        {
            use sha2::Digest;
            hex::encode(sha2::Sha256::digest(ck.cert.der()))
        }
    );
    // Port param also works, and a non-TLS port fails cleanly.
    let out = hns
        .run(
            "network.tls_inspect",
            json!({"host": "127.0.0.1", "port": port, "sni": "localhost"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["sni"], "localhost");
    let plain = spawn_http(Arc::new(|_| (200, vec![], b"x".to_vec()))).await;
    assert_eq!(
        hns.run(
            "network.tls_inspect",
            json!({"host": format!("127.0.0.1:{plain}"), "timeout_secs": 3})
        )
        .await
        .unwrap_err()
        .code(),
        "failed"
    );
}

#[tokio::test]
async fn port_check_and_connectivity_local() {
    let open = spawn_http(Arc::new(|_| (200, vec![], b"ok".to_vec()))).await;
    let hns = harness();
    assert_eq!(
        hns.run(
            "network.port_check",
            json!({"host": "127.0.0.1", "preset": "custom", "ports": "1-60"})
        )
        .await
        .unwrap_err()
        .code(),
        "invalid_input"
    );
    // A "closed" port comes from binding port 0 and dropping the listener.
    // A test running in parallel can be handed the same port and open it,
    // so retry with a fresh port when that happens. (Holding a bound,
    // non-listening socket instead doesn't work: macOS drops SYNs to it
    // rather than refusing them.)
    for attempt in 1..=5 {
        let closed = TcpListener::bind("127.0.0.1:0")
            .await
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        // Windows retries refused SYNs for ~2 s before reporting "connection
        // refused", so allow enough time to classify the port as closed.
        let out = hns.run("network.port_check", json!({"host": "127.0.0.1", "preset": "custom", "ports": format!("{open},{closed}"), "timeout_ms": 4000})).await.unwrap();
        let conn = hns.run("network.connectivity", json!({"targets": format!("127.0.0.1:{open}\nhttp://127.0.0.1:{open}/\n127.0.0.1:{closed}"), "attempts": 2})).await.unwrap();
        let reused =
            out.data["open_ports"] != json!([open]) || conn.data["results"][2]["ok"] != false;
        if reused && attempt < 5 {
            continue;
        }
        assert_eq!(out.data["open_ports"], json!([open]));
        let rows = out.data["table"]["rows"].as_array().unwrap();
        assert!(rows.iter().any(|r| r[0] == closed && r[2] == "closed"));
        let r = conn.data["results"].as_array().unwrap();
        assert_eq!(r[0]["ok"], true);
        assert_eq!(r[0]["successes"], 2);
        assert_eq!(r[1]["status"], 200);
        assert_eq!(r[2]["ok"], false);
        assert_eq!(conn.data["reachable"], 2);
        break;
    }
    assert_eq!(
        hns.run("network.connectivity", json!({"targets": "example.com"}))
            .await
            .unwrap_err()
            .code(),
        "invalid_input"
    );
}

#[tokio::test]
async fn specs_are_consistent() {
    for t in dogubako_tools_net::tools() {
        let s = t.spec();
        assert!(s.id.starts_with("network."), "{}", s.id);
        assert!(!s.description.is_empty());
        let networked = !matches!(
            s.id.as_str(),
            "network.url_parse"
                | "network.url_normalize"
                | "network.query_params"
                | "network.ip_info"
                | "network.cidr"
                | "network.subnet_plan"
                | "network.cert_decode"
                | "network.key_inspect"
        );
        assert_eq!(
            s.permissions
                .contains(&dogubako_core::spec::Permission::Network),
            networked,
            "{}",
            s.id
        );
        if !networked {
            assert_eq!(s.ui.as_deref(), Some("live"), "{}", s.id);
        }
    }
}

// ---------------------------------------------------------------------------
// Internet tests (the sandbox proxy blocks direct egress; run manually).

#[tokio::test]
#[ignore]
async fn internet_dns_rdap_whois_tls() {
    let h = harness();
    let out = h
        .run(
            "network.dns_lookup",
            json!({"name": "example.com", "types": ["A", "NS", "SOA"], "resolver": "cloudflare"}),
        )
        .await
        .unwrap();
    assert!(!out.data["table"]["rows"].as_array().unwrap().is_empty());
    let out = h
        .run(
            "network.dns_lookup",
            json!({"name": "example.com", "types": ["A"], "resolver": "google-doh"}),
        )
        .await
        .unwrap();
    assert!(!out.data["records"]["A"].as_array().unwrap().is_empty());
    let out = h
        .run("network.reverse_dns", json!({"ips": "1.1.1.1"}))
        .await
        .unwrap();
    assert!(out.text.unwrap().contains("one.one.one.one"));
    let out = h
        .run("network.rdap", json!({"query": "example.com"}))
        .await
        .unwrap();
    assert!(out.data["nameservers"].as_array().unwrap().len() >= 2);
    let out = h
        .run("network.rdap", json!({"query": "AS13335"}))
        .await
        .unwrap();
    assert_eq!(out.data["object_class"], "autnum");
    let out = h
        .run("network.whois", json!({"query": "example.com"}))
        .await
        .unwrap();
    assert!(out.data["chain"].as_array().unwrap().len() >= 2);
    let out = h
        .run("network.tls_inspect", json!({"host": "example.com"}))
        .await
        .unwrap();
    assert_eq!(out.data["trust"]["trusted"], true);
    let out = h
        .run("network.domain_inspect", json!({"domain": "example.com"}))
        .await
        .unwrap();
    assert_eq!(out.data["dns"]["ok"], true);
}
