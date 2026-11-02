//! `network.tls_inspect`: TLS handshake details and certificate chain, with
//! an honest, separate trust evaluation against the Mozilla root store.

use crate::certs::{cert_details, summary_line};
use crate::util::{connect_any, ms, split_host_port, table};
use dogubako_core::prelude::*;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use std::time::{Duration, Instant};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![tls_inspect()]
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls::crypto::aws_lc_rs::default_provider())
}

/// Accepts any certificate chain so it can be inspected, but still checks
/// handshake signatures (proving the server holds the certificate's key).
/// Trust is evaluated separately with `WebPkiServerVerifier`.
#[derive(Debug)]
struct CaptureVerifier {
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for CaptureVerifier {
    fn verify_server_cert(
        &self,
        _: &CertificateDer<'_>,
        _: &[CertificateDer<'_>],
        _: &ServerName<'_>,
        _: &[u8],
        _: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Connect, handshake and describe. `host` must already be validated.
pub async fn tls_probe(
    ctx: &ToolContext,
    host: &str,
    port: u16,
    sni: Option<&str>,
    timeout: Duration,
) -> ToolResult<Value> {
    let started = Instant::now();
    let addrs = dogubako_core::net::resolve_checked(ctx.net_policy(), host, port).await?;
    let dns_ms = ms(started.elapsed());
    let (tcp, addr, connect_time) = connect_any(&addrs, timeout).await?;
    let sni_name = sni.unwrap_or(host).trim_matches(['[', ']']).to_string();
    let server_name = ServerName::try_from(sni_name.clone()).map_err(|_| {
        ToolError::invalid("sni", format!("'{sni_name}' is not a valid server name"))
    })?;
    let prov = provider();
    let mut cfg = rustls::ClientConfig::builder_with_provider(prov.clone())
        .with_safe_default_protocol_versions()
        .map_err(|e| ToolError::failed(format!("TLS config: {e}")))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(CaptureVerifier {
            provider: prov.clone(),
        }))
        .with_no_client_auth();
    cfg.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let connector = tokio_rustls::TlsConnector::from(Arc::new(cfg));
    let hs_start = Instant::now();
    let stream = tokio::time::timeout(timeout, connector.connect(server_name.clone(), tcp))
        .await
        .map_err(|_| ToolError::failed(format!("TLS handshake with {addr} timed out")))?
        .map_err(|e| ToolError::failed(format!("TLS handshake with {addr} failed: {e} (note: TLS 1.0/1.1 and SSLv3 are not supported by this client)")))?;
    let handshake_ms = ms(hs_start.elapsed());
    let (_, conn) = stream.get_ref();
    let chain: Vec<CertificateDer<'static>> = conn
        .peer_certificates()
        .map(|c| c.iter().map(|x| x.clone().into_owned()).collect())
        .unwrap_or_default();
    let version = conn
        .protocol_version()
        .map(|v| {
            format!("{v:?}")
                .replace('_', ".")
                .replace("TLSv1.", "TLS 1.")
        })
        .unwrap_or_default();
    let cipher = conn
        .negotiated_cipher_suite()
        .map(|c| format!("{:?}", c.suite()))
        .unwrap_or_default();
    let alpn = conn
        .alpn_protocol()
        .map(|a| String::from_utf8_lossy(a).to_string());
    let kx = conn
        .negotiated_key_exchange_group()
        .map(|g| format!("{:?}", g.name()));

    // Separate trust evaluation.
    let trust = if chain.is_empty() {
        json!({ "trusted": false, "error": "server sent no certificate" })
    } else {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        match rustls::client::WebPkiServerVerifier::builder_with_provider(
            Arc::new(roots),
            prov.clone(),
        )
        .build()
        {
            Ok(v) => match v.verify_server_cert(
                &chain[0],
                &chain[1..],
                &server_name,
                &[],
                UnixTime::now(),
            ) {
                Ok(_) => {
                    json!({ "trusted": true, "store": "Mozilla root store (webpki-roots)", "hostname_match": true })
                }
                Err(e) => {
                    json!({ "trusted": false, "store": "Mozilla root store (webpki-roots)", "error": e.to_string() })
                }
            },
            Err(e) => {
                json!({ "trusted": Value::Null, "error": format!("cannot build verifier: {e}") })
            }
        }
    };
    let certs: Vec<Value> = chain
        .iter()
        .map(|c| cert_details(c.as_ref()).unwrap_or_else(|e| json!({ "error": e.to_string() })))
        .collect();
    Ok(json!({
        "host": host,
        "port": port,
        "address": addr.to_string(),
        "sni": sni_name,
        "protocol": version,
        "cipher_suite": cipher,
        "alpn": alpn,
        "key_exchange_group": kx,
        "trust": trust,
        "timing_ms": { "dns": dns_ms, "tcp_connect": ms(connect_time), "tls_handshake": handshake_ms },
        "certificate": certs.first().cloned().unwrap_or(Value::Null),
        "chain": certs,
    }))
}

fn tls_inspect() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("network.tls_inspect", "TLS / SSL Inspector", Category::Network)
        .describe("Connect to a TLS server and show protocol, cipher, ALPN, the certificate chain (SANs, validity, key, fingerprints) and whether it is publicly trusted")
        .keywords(&["tls", "ssl", "https", "certificate", "chain", "expiry", "cipher", "alpn", "handshake", "openssl s_client"])
        .param(ParamSpec::text("host").label("Host").describe("host, host:port or https:// URL").required().primary().placeholder("example.com"))
        .param(ParamSpec::integer("port").label("Port").range(1.0, 65535.0).default_int(443))
        .param(ParamSpec::text("sni").label("SNI server name override").advanced())
        .param(ParamSpec::integer("timeout_secs").label("Timeout (s)").range(1.0, 30.0).default_int(10).advanced())
        .permission(Permission::Network)
        .timeout(60);
    async_tool(spec, |ctx, p| async move {
        let (host, explicit) = split_host_port(p.str("host")?, 0)?;
        let port = if explicit != 0 {
            explicit
        } else {
            p.int_or("port", 443) as u16
        };
        let host = crate::util::clean_host("host", &host)?;
        let timeout = Duration::from_secs(p.int_or("timeout_secs", 10) as u64);
        let d = tls_probe(&ctx, &host, port, p.opt_str("sni"), timeout).await?;
        let mut text = format!(
            "{}:{} ({}) — {} {}{}\nTrust: {}\n",
            host,
            port,
            d["address"].as_str().unwrap_or(""),
            d["protocol"].as_str().unwrap_or(""),
            d["cipher_suite"].as_str().unwrap_or(""),
            d["alpn"]
                .as_str()
                .map(|a| format!(" — ALPN {a}"))
                .unwrap_or_default(),
            if d["trust"]["trusted"] == true {
                "trusted".to_string()
            } else {
                format!(
                    "NOT trusted ({})",
                    d["trust"]["error"].as_str().unwrap_or("unknown")
                )
            }
        );
        let mut rows = vec![];
        if let Some(chain) = d["chain"].as_array() {
            for (i, c) in chain.iter().enumerate() {
                text.push_str(&format!("[{i}] {}\n", summary_line(c)));
                rows.push(vec![
                    json!(i),
                    c["subject"].clone(),
                    c["issuer"].clone(),
                    c["not_after"].clone(),
                    c["days_remaining"].clone(),
                    json!(format!(
                        "{} {}",
                        c["public_key"]["algorithm"].as_str().unwrap_or(""),
                        c["public_key"]["bits"]
                    )),
                    c["signature_algorithm"].clone(),
                ]);
            }
        }
        let mut data = d;
        data["table"] = table(
            &[
                "#",
                "subject",
                "issuer",
                "not after",
                "days left",
                "key",
                "signature",
            ],
            rows,
        );
        let mut out = ToolOutput::text(text).with_data(data.clone());
        if let Some(w) = data["certificate"]["warnings"].as_array() {
            for x in w.iter().filter_map(|x| x.as_str()) {
                out = out.warn(x.to_string());
            }
        }
        Ok(out)
    })
}
