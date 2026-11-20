//! PolicyProxy holds CONNECT tunnels and plain-HTTP requests to the NetPolicy.

use dogubako_core::net::PolicyProxy;
use dogubako_core::policy::NetPolicy;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A one-shot upstream that records the request head it gets and answers "hi".
async fn upstream() -> (u16, tokio::task::JoinHandle<String>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    let h = tokio::spawn(async move {
        let (mut s, _) = l.accept().await.unwrap();
        let mut buf = vec![0u8; 4096];
        let n = s.read(&mut buf).await.unwrap();
        s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi")
            .await
            .unwrap();
        String::from_utf8_lossy(&buf[..n]).into_owned()
    });
    (port, h)
}

async fn send(proxy: &PolicyProxy, req: String) -> String {
    let addr = proxy.url().trim_start_matches("http://").to_string();
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = vec![];
    let mut buf = [0u8; 1024];
    loop {
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), s.read(&mut buf))
            .await
            .unwrap()
            .unwrap();
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        let t = String::from_utf8_lossy(&out);
        if t.ends_with("hi") || t.contains("Connection established\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[tokio::test]
async fn private_targets_are_refused_under_public_only() {
    let proxy = PolicyProxy::start(NetPolicy::public_only()).await.unwrap();
    let (port, _up) = upstream().await;
    for req in [
        format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n"),
        format!("GET http://127.0.0.1:{port}/x HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"),
        "CONNECT localhost:443 HTTP/1.1\r\n\r\n".to_string(),
        "GET http://[::1]/ HTTP/1.1\r\n\r\n".to_string(),
        "GET http://metadata.google.internal/ HTTP/1.1\r\n\r\n".to_string(),
    ] {
        let r = send(&proxy, req.clone()).await;
        assert!(
            r.starts_with("HTTP/1.1 403 Blocked by network policy"),
            "{req}: {r}"
        );
    }
}

#[tokio::test]
async fn denied_hosts_are_refused() {
    let policy = NetPolicy {
        allow_private: true,
        deny_hosts: vec!["localhost".into()],
    };
    let proxy = PolicyProxy::start(policy).await.unwrap();
    let r = send(&proxy, "CONNECT localhost:443 HTTP/1.1\r\n\r\n".into()).await;
    assert!(r.starts_with("HTTP/1.1 403"), "{r}");
}

#[tokio::test]
async fn allowed_plain_http_is_forwarded_in_origin_form() {
    let proxy = PolicyProxy::start(NetPolicy::default()).await.unwrap();
    let (port, up) = upstream().await;
    let r = send(
        &proxy,
        format!("GET http://127.0.0.1:{port}/a/b?c=1 HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nProxy-Connection: keep-alive\r\n\r\n"),
    )
    .await;
    assert!(r.starts_with("HTTP/1.1 200 OK") && r.ends_with("hi"), "{r}");
    let head = up.await.unwrap();
    assert!(head.starts_with("GET /a/b?c=1 HTTP/1.1\r\n"), "{head}");
    assert!(head.contains("Connection: close\r\n"), "{head}");
    assert!(
        !head.to_ascii_lowercase().contains("proxy-connection"),
        "{head}"
    );
}

#[tokio::test]
async fn allowed_connect_tunnels() {
    let proxy = PolicyProxy::start(NetPolicy::default()).await.unwrap();
    let (port, up) = upstream().await;
    let addr = proxy.url().trim_start_matches("http://").to_string();
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(format!("CONNECT 127.0.0.1:{port} HTTP/1.1\r\n\r\nping").as_bytes())
        .await
        .unwrap();
    let mut got = vec![];
    let mut buf = [0u8; 256];
    while !String::from_utf8_lossy(&got).ends_with("hi") {
        let n = s.read(&mut buf).await.unwrap();
        assert!(n > 0);
        got.extend_from_slice(&buf[..n]);
    }
    let got = String::from_utf8_lossy(&got);
    assert!(
        got.starts_with("HTTP/1.1 200 Connection established\r\n\r\n"),
        "{got}"
    );
    assert_eq!(up.await.unwrap(), "ping");
}
