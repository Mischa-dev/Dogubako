//! Security policies enforced by the engine: filesystem roots, network
//! egress rules, granted permissions and resource limits.

use crate::error::{ToolError, ToolResult};
use crate::spec::Permission;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::net::IpAddr;
use std::path::{Component, Path, PathBuf};

/// Restricts which host paths `path` parameters may reference.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FsPolicy {
    /// `None` = unrestricted (local desktop / CLI). `Some(empty)` = no host
    /// filesystem access at all. Otherwise paths must lie within a root
    /// after symlink resolution.
    pub roots: Option<Vec<PathBuf>>,
}

impl FsPolicy {
    pub fn unrestricted() -> Self {
        FsPolicy { roots: None }
    }
    pub fn deny_all() -> Self {
        FsPolicy { roots: Some(vec![]) }
    }
    pub fn roots(roots: Vec<PathBuf>) -> Self {
        let canon = roots.into_iter().filter_map(|r| std::fs::canonicalize(&r).ok()).collect();
        FsPolicy { roots: Some(canon) }
    }

    /// Validate an existing path; returns its canonical form.
    pub fn check(&self, param: &str, path: &Path) -> ToolResult<PathBuf> {
        if path.as_os_str().is_empty() {
            return Err(ToolError::invalid(param, "path is empty"));
        }
        if path.to_string_lossy().contains('\0') {
            return Err(ToolError::invalid(param, "path contains NUL byte"));
        }
        if let Some(roots) = &self.roots {
            if roots.is_empty() {
                return Err(ToolError::denied("host filesystem access is disabled on this instance"));
            }
        }
        let canon = std::fs::canonicalize(path)
            .map_err(|e| ToolError::invalid(param, format!("cannot access '{}': {e}", path.display())))?;
        self.check_canonical(param, &canon)?;
        Ok(canon)
    }

    /// Validate a path that may not exist yet (e.g. a rename target): the
    /// parent must exist and be allowed, and the file name must be plain.
    pub fn check_new(&self, param: &str, path: &Path) -> ToolResult<PathBuf> {
        let name = path.file_name().ok_or_else(|| ToolError::invalid(param, "path has no file name"))?;
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let parent = self.check(param, parent)?;
        let target = parent.join(name);
        if let Ok(meta) = std::fs::symlink_metadata(&target) {
            if meta.file_type().is_symlink() {
                return Err(ToolError::invalid(param, "refusing to write through a symlink"));
            }
        }
        Ok(target)
    }

    fn check_canonical(&self, param: &str, canon: &Path) -> ToolResult<()> {
        match &self.roots {
            None => Ok(()),
            Some(roots) => {
                if roots.iter().any(|r| canon.starts_with(r)) {
                    Ok(())
                } else {
                    Err(ToolError::PermissionDenied {
                        message: format!("'{}' for '{param}' is outside the allowed folders", canon.display()),
                    })
                }
            }
        }
    }
}

/// Network egress policy (SSRF protection for hosted deployments).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetPolicy {
    /// Allow loopback, private, link-local and other non-public addresses.
    pub allow_private: bool,
    /// Hostnames always denied (lower-case).
    #[serde(default)]
    pub deny_hosts: Vec<String>,
}

impl Default for NetPolicy {
    fn default() -> Self {
        NetPolicy { allow_private: true, deny_hosts: vec![] }
    }
}

impl NetPolicy {
    pub fn public_only() -> Self {
        NetPolicy { allow_private: false, deny_hosts: vec![] }
    }

    pub fn ip_allowed(&self, ip: IpAddr) -> bool {
        self.allow_private || is_public_ip(ip)
    }

    pub fn check_host(&self, host: &str) -> ToolResult<()> {
        let h = host.trim_end_matches('.').to_ascii_lowercase();
        if self.deny_hosts.iter().any(|d| h == *d || h.ends_with(&format!(".{d}"))) {
            return Err(ToolError::denied(format!("host '{host}' is blocked by policy")));
        }
        if !self.allow_private {
            if h == "localhost" || h.ends_with(".localhost") || h.ends_with(".internal") || h.ends_with(".local") {
                return Err(ToolError::denied(format!("host '{host}' is an internal name; blocked by network policy")));
            }
            if let Ok(ip) = h.trim_matches(['[', ']']).parse::<IpAddr>() {
                if !is_public_ip(ip) {
                    return Err(ToolError::denied(format!("address {ip} is not public; blocked by network policy")));
                }
            }
        }
        Ok(())
    }
}

/// True when the address is globally routable (not loopback, private,
/// link-local, CGNAT, multicast, documentation, unique-local, ...).
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                || v4.is_multicast()
                || o[0] == 0
                || (o[0] == 100 && (o[1] & 0xc0) == 64) // 100.64/10 CGNAT (incl. Tailscale)
                || (o[0] == 192 && o[1] == 0 && o[2] == 0) // 192.0.0/24
                || (o[0] == 198 && (o[1] & 0xfe) == 18) // 198.18/15 benchmarking
                || o[0] >= 240)
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let s = v6.segments();
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || (s[0] & 0xfe00) == 0xfc00 // unique local
                || (s[0] & 0xffc0) == 0xfe80 // link local
                || (s[0] == 0x2001 && s[1] == 0x0db8) // documentation
                || (s[0] == 0x64 && s[1] == 0xff9b)) // NAT64 (could map to private v4)
        }
    }
}

/// Permissions granted to the caller of a tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PermissionSet(pub BTreeSet<Permission>);

impl PermissionSet {
    pub fn all() -> Self {
        PermissionSet(Permission::ALL.into_iter().collect())
    }
    pub fn none() -> Self {
        PermissionSet(BTreeSet::new())
    }
    pub fn from_iter(it: impl IntoIterator<Item = Permission>) -> Self {
        PermissionSet(it.into_iter().collect())
    }
    pub fn has(&self, p: Permission) -> bool {
        self.0.contains(&p)
    }
    pub fn missing(&self, required: &[Permission]) -> Vec<Permission> {
        required.iter().copied().filter(|p| !self.has(*p)).collect()
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SandboxMode {
    /// Run external programs directly (with rlimits on Unix).
    #[default]
    None,
    /// Wrap external programs with bubblewrap (Linux): no network, private
    /// /tmp, read-only system dirs, only inputs/work dirs visible.
    Bwrap,
}

/// Resource limits applied to tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecLimits {
    /// Address-space limit per external process (MiB). Unix only.
    pub max_memory_mb: Option<u64>,
    /// CPU time limit per external process (seconds). Unix only.
    pub max_cpu_secs: Option<u64>,
    /// Max size of any file written by an external process (MiB). Unix only.
    pub max_file_size_mb: Option<u64>,
    /// Max bytes captured from a process' stdout.
    pub max_stdout_bytes: usize,
    /// Upper bound on a tool's wall-clock timeout (seconds).
    pub max_timeout_secs: u64,
    /// Archive extraction: total uncompressed bytes.
    pub max_extract_bytes: u64,
    /// Archive extraction: number of entries.
    pub max_extract_entries: u64,
    /// Max image pixels decoded (decompression-bomb guard).
    pub max_image_pixels: u64,
    pub sandbox: SandboxMode,
}

impl Default for ExecLimits {
    fn default() -> Self {
        ExecLimits {
            max_memory_mb: None,
            max_cpu_secs: None,
            max_file_size_mb: None,
            max_stdout_bytes: 256 * 1024 * 1024,
            max_timeout_secs: 6 * 3600,
            max_extract_bytes: 20 * 1024 * 1024 * 1024,
            max_extract_entries: 200_000,
            max_image_pixels: 400_000_000,
            sandbox: SandboxMode::None,
        }
    }
}

impl ExecLimits {
    /// Conservative defaults for hosted, multi-user deployments.
    pub fn hosted() -> Self {
        ExecLimits {
            max_memory_mb: Some(4096),
            max_cpu_secs: Some(3600),
            max_file_size_mb: Some(8192),
            max_stdout_bytes: 64 * 1024 * 1024,
            max_timeout_secs: 3600,
            max_extract_bytes: 4 * 1024 * 1024 * 1024,
            max_extract_entries: 50_000,
            max_image_pixels: 200_000_000,
            sandbox: SandboxMode::None,
        }
    }
}

/// Sanitize a user-provided file name: strip directories and dangerous
/// characters so it is safe to create inside an output directory.
pub fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().to_string()),
            _ => None,
        })
        .last()
        .unwrap_or_default();
    let mut s: String = base
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    s = s.trim().trim_matches('.').to_string();
    if s.is_empty() {
        s = "file".into();
    }
    // Windows reserved device names
    let upper = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "LPT1", "LPT2", "LPT3"].contains(&upper.as_str()) {
        s = format!("_{s}");
    }
    if s.len() > 200 {
        let ext = Path::new(&s).extension().map(|e| e.to_string_lossy().to_string());
        let mut cut: String = s.chars().take(180).collect();
        if let Some(e) = ext {
            cut = format!("{cut}.{e}");
        }
        s = cut;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_ip_checks() {
        for private in ["127.0.0.1", "10.1.2.3", "192.168.1.1", "172.16.0.1", "169.254.169.254", "100.100.1.1", "::1", "fe80::1", "fd00::1", "::ffff:10.0.0.1", "0.0.0.0"] {
            assert!(!is_public_ip(private.parse().unwrap()), "{private}");
        }
        for public in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(is_public_ip(public.parse().unwrap()), "{public}");
        }
    }

    #[test]
    fn sanitizes_names() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("a/b\\c.txt"), "b_c.txt");
        assert_eq!(sanitize_filename(".."), "file");
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
    }

    #[test]
    fn fs_roots_enforced() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "x").unwrap();
        std::fs::write(other.path().join("b.txt"), "x").unwrap();
        let pol = FsPolicy::roots(vec![dir.path().to_path_buf()]);
        assert!(pol.check("p", &dir.path().join("a.txt")).is_ok());
        assert!(pol.check("p", &other.path().join("b.txt")).is_err());
        assert!(pol.check("p", &dir.path().join("../").join(other.path().file_name().unwrap()).join("b.txt")).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(other.path().join("b.txt"), dir.path().join("link")).unwrap();
            assert!(pol.check("p", &dir.path().join("link")).is_err(), "symlink escape must be rejected");
        }
        assert!(FsPolicy::deny_all().check("p", &dir.path().join("a.txt")).is_err());
    }

    #[test]
    fn net_policy() {
        let p = NetPolicy::public_only();
        assert!(p.check_host("localhost").is_err());
        assert!(p.check_host("127.0.0.1").is_err());
        assert!(p.check_host("[::1]").is_err());
        assert!(p.check_host("example.com").is_ok());
        assert!(NetPolicy::default().check_host("localhost").is_ok());
    }
}
