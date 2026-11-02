//! Authentication: Argon2id password hashing, browser sessions with CSRF
//! protection, revocable API tokens and login rate limiting.

use crate::authz::{Policy, Principal, Role};
use crate::config::AuthMode;
use crate::error::ApiError;
use crate::state::AppState;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{header, Method};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const SESSION_COOKIE: &str = "dgb_session";
pub const TOKEN_PREFIX: &str = "dgb_";
pub const LOCAL_USER_ID: &str = "local";

pub fn hash_password(pw: &str) -> Result<String, String> {
    Argon2::default().hash_password(pw.as_bytes()).map(|h| h.to_string()).map_err(|e| e.to_string())
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default().verify_password(pw.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

/// Run a dummy verification so unknown usernames take as long as known ones.
pub fn dummy_verify() {
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let h = DUMMY.get_or_init(|| hash_password("dummy-password-for-timing").unwrap_or_default());
    let _ = verify_password("not-the-password", h);
}

pub fn validate_password(pw: &str) -> Result<(), ApiError> {
    if pw.chars().count() < 10 {
        return Err(ApiError::bad_request("password must be at least 10 characters"));
    }
    if pw.len() > 512 {
        return Err(ApiError::bad_request("password is too long"));
    }
    Ok(())
}

pub fn validate_username(u: &str) -> Result<(), ApiError> {
    let ok = (2..=64).contains(&u.len()) && u.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '@'));
    if ok {
        Ok(())
    } else {
        Err(ApiError::bad_request("username must be 2-64 characters: letters, digits, _ - . @"))
    }
}

pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

pub fn random_token(bytes: usize) -> String {
    dogubako_core::util::random_id(bytes)
}

/// New API token: (plaintext shown once, stored hash, display prefix).
pub fn new_api_token() -> (String, String, String) {
    let t = format!("{TOKEN_PREFIX}{}", random_token(32));
    let prefix = t[..12].to_string();
    let h = sha256_hex(&t);
    (t, h, prefix)
}

pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Simple sliding-window limiter for login attempts per key (IP / username).
pub struct RateLimiter {
    window: Duration,
    max: u32,
    hits: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    pub fn new(max: u32, window: Duration) -> Self {
        RateLimiter { window, max, hits: Mutex::new(HashMap::new()) }
    }
    /// Record an attempt; returns false if over the limit.
    pub fn check(&self, key: &str) -> bool {
        let mut m = self.hits.lock().unwrap();
        let now = Instant::now();
        if m.len() > 10_000 {
            m.retain(|_, (t, _)| now.duration_since(*t) < self.window);
        }
        let e = m.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(e.0) > self.window {
            *e = (now, 0);
        }
        e.1 += 1;
        e.1 <= self.max
    }
    pub fn reset(&self, key: &str) {
        self.hits.lock().unwrap().remove(key);
    }
}

fn cookie_value(parts: &Parts, name: &str) -> Option<String> {
    for v in parts.headers.get_all(header::COOKIE) {
        let Ok(s) = v.to_str() else { continue };
        for kv in s.split(';') {
            if let Some((k, val)) = kv.trim().split_once('=') {
                if k == name {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

pub fn client_ip(parts: &Parts) -> String {
    parts
        .extensions
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0.ip().to_string())
        .unwrap_or_default()
}

pub fn local_principal() -> Principal {
    Principal {
        user_id: LOCAL_USER_ID.into(),
        username: "local".into(),
        role: Role::Owner,
        policy: Policy::for_role(Role::Owner),
        token_scope: None,
        token_id: None,
        via: "none",
        csrf: None,
    }
}

/// Resolve the principal for a request, if any.
pub fn authenticate(state: &AppState, parts: &Parts) -> Result<Option<Principal>, ApiError> {
    if state.cfg.auth == AuthMode::None {
        return Ok(Some(local_principal()));
    }
    if let Some(auth) = parts.headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) {
        let token = auth.strip_prefix("Bearer ").or_else(|| auth.strip_prefix("bearer ")).unwrap_or("").trim();
        if !token.starts_with(TOKEN_PREFIX) {
            return Err(ApiError::unauthorized("invalid bearer token"));
        }
        let Some((user, tid, scope)) = state.db.token_user(&sha256_hex(token)) else {
            return Err(ApiError::unauthorized("invalid, expired or revoked token"));
        };
        if user.disabled {
            return Err(ApiError::unauthorized("account disabled"));
        }
        state.db.touch_user(&user.id);
        return Ok(Some(Principal {
            user_id: user.id,
            username: user.username,
            role: user.role,
            policy: user.policy,
            token_scope: scope,
            token_id: Some(tid),
            via: "token",
            csrf: None,
        }));
    }
    if let Some(sid) = cookie_value(parts, SESSION_COOKIE) {
        if let Some((user, csrf)) = state.db.session_user(&sha256_hex(&sid)) {
            if user.disabled {
                return Err(ApiError::unauthorized("account disabled"));
            }
            let safe = matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS);
            if !safe {
                let sent = parts.headers.get("x-csrf-token").and_then(|v| v.to_str().ok()).unwrap_or("");
                if !constant_time_eq(sent, &csrf) {
                    return Err(ApiError::forbidden("missing or invalid CSRF token"));
                }
            }
            return Ok(Some(Principal {
                user_id: user.id,
                username: user.username,
                role: user.role,
                policy: user.policy,
                token_scope: None,
                token_id: None,
                via: "session",
                csrf: Some(csrf),
            }));
        }
    }
    Ok(None)
}

impl FromRequestParts<AppState> for Principal {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        match authenticate(state, parts)? {
            Some(p) => Ok(p),
            None => Err(ApiError::unauthorized("authentication required")),
        }
    }
}

/// Extractor requiring an administrator.
pub struct Admin(pub Principal);

impl FromRequestParts<AppState> for Admin {
    type Rejection = ApiError;
    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let p = Principal::from_request_parts(parts, state).await?;
        if !p.is_admin() {
            return Err(ApiError::forbidden("administrator privileges required"));
        }
        Ok(Admin(p))
    }
}

/// Client IP extractor.
pub struct ClientIp(pub String);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientIp(client_ip(parts)))
    }
}

pub fn session_cookie(value: &str, max_age: i64, secure: bool) -> String {
    format!(
        "{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age}{}",
        if secure { "; Secure" } else { "" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn password_roundtrip() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery", &h));
        assert!(!verify_password("wrong", &h));
    }
    #[test]
    fn tokens_are_unique_and_prefixed() {
        let (a, ha, pa) = new_api_token();
        let (b, _, _) = new_api_token();
        assert_ne!(a, b);
        assert!(a.starts_with("dgb_") && a.len() == 68);
        assert_eq!(ha, sha256_hex(&a));
        assert!(a.starts_with(&pa));
    }
    #[test]
    fn limiter() {
        let l = RateLimiter::new(3, Duration::from_secs(60));
        assert!(l.check("x") && l.check("x") && l.check("x"));
        assert!(!l.check("x"));
        assert!(l.check("y"));
    }
}
