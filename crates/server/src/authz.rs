//! Authorization: roles, per-principal policies and token scopes.
//!
//! Effective access = principal policy ∩ token scope (when authenticating
//! with a scoped API token). The result is enforced inside the engine for
//! every invocation, regardless of interface (UI, HTTP, MCP).

use dogubako_core::policy::PermissionSet;
use dogubako_core::spec::{Permission, ToolSpec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Admin,
    User,
    Agent,
}

impl Role {
    pub fn parse(s: &str) -> Option<Role> {
        match s {
            "owner" => Some(Role::Owner),
            "admin" => Some(Role::Admin),
            "user" => Some(Role::User),
            "agent" => Some(Role::Agent),
            _ => None,
        }
    }
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Owner => "owner",
            Role::Admin => "admin",
            Role::User => "user",
            Role::Agent => "agent",
        }
    }
    pub fn is_admin(&self) -> bool {
        matches!(self, Role::Owner | Role::Admin)
    }
}

/// What a principal (or a token) may do.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Policy {
    /// Allowed tool patterns: `*`, `image.*`, `video.compress`.
    #[serde(default)]
    pub allow: Vec<String>,
    /// Denied tool patterns (override allow).
    #[serde(default)]
    pub deny: Vec<String>,
    /// Extra capabilities (network, host_fs_read, ...).
    #[serde(default)]
    pub permissions: Vec<Permission>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_jobs: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quota_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_upload_mb: Option<u64>,
    /// MCP discovery mode override ("full" / "compact").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp_discovery: Option<String>,
}

pub fn pattern_matches(pattern: &str, spec: &ToolSpec) -> bool {
    let p = pattern.trim();
    if p == "*" {
        return true;
    }
    if let Some(cat) = p.strip_prefix("category:") {
        return spec.category.id() == cat;
    }
    if let Some(prefix) = p.strip_suffix(".*") {
        return spec.id.starts_with(&format!("{prefix}.")) || spec.category.id() == prefix;
    }
    spec.id == p
}

impl Policy {
    /// Defaults by role (least privilege for agents: nothing).
    pub fn for_role(role: Role) -> Policy {
        match role {
            Role::Owner | Role::Admin => Policy {
                allow: vec!["*".into()],
                permissions: Permission::ALL.to_vec(),
                ..Default::default()
            },
            Role::User => Policy {
                allow: vec!["*".into()],
                permissions: vec![Permission::Network],
                ..Default::default()
            },
            Role::Agent => Policy::default(),
        }
    }

    pub fn allows_tool(&self, spec: &ToolSpec) -> bool {
        self.allow.iter().any(|p| pattern_matches(p, spec))
            && !self.deny.iter().any(|p| pattern_matches(p, spec))
    }

    pub fn validate(&self) -> Result<(), String> {
        for p in self.allow.iter().chain(self.deny.iter()) {
            let ok = p == "*"
                || p.strip_prefix("category:")
                    .map(|c| dogubako_core::spec::Category::parse(c).is_some())
                    .unwrap_or(false)
                || p.chars().all(|c| {
                    c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || matches!(c, '.' | '_' | '-' | '*')
                });
            if !ok {
                return Err(format!("invalid tool pattern '{p}'"));
            }
        }
        if let Some(m) = &self.mcp_discovery {
            if m != "full" && m != "compact" {
                return Err("mcp_discovery must be 'full' or 'compact'".into());
            }
        }
        Ok(())
    }
}

/// An authenticated caller.
#[derive(Debug, Clone, Serialize)]
pub struct Principal {
    pub user_id: String,
    pub username: String,
    pub role: Role,
    pub policy: Policy,
    /// Scope of the API token used, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_scope: Option<Policy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_id: Option<String>,
    /// "session", "token" or "none" (auth disabled).
    pub via: &'static str,
    #[serde(skip)]
    pub csrf: Option<String>,
}

impl Principal {
    /// Administrative access requires an admin role and no token scope:
    /// a scoped token never carries admin rights, even with `allow: ["*"]`,
    /// so it cannot be used to mint an unrestricted token.
    pub fn is_admin(&self) -> bool {
        self.role.is_admin() && self.token_scope.is_none()
    }

    pub fn can_use(&self, spec: &ToolSpec) -> bool {
        self.policy.allows_tool(spec)
            && self
                .token_scope
                .as_ref()
                .map(|t| t.allows_tool(spec))
                .unwrap_or(true)
    }

    /// Granted capabilities, intersected with the token scope and with what
    /// the instance enables (host fs only when roots are configured).
    pub fn permissions(&self, host_fs_enabled: bool) -> PermissionSet {
        PermissionSet::from_iter(self.policy.permissions.iter().copied().filter(|p| {
            let instance_ok =
                host_fs_enabled || !matches!(p, Permission::HostFsRead | Permission::HostFsWrite);
            let token_ok = self
                .token_scope
                .as_ref()
                .map(|t| t.permissions.contains(p))
                .unwrap_or(true);
            instance_ok && token_ok
        }))
    }

    pub fn concurrency(&self, default: usize) -> usize {
        self.token_scope
            .as_ref()
            .and_then(|t| t.max_concurrent_jobs)
            .or(self.policy.max_concurrent_jobs)
            .unwrap_or(default)
            .max(1)
    }

    pub fn quota_bytes(&self, default_mb: u64) -> u64 {
        self.policy
            .quota_mb
            .unwrap_or(default_mb)
            .saturating_mul(1024 * 1024)
    }

    pub fn max_upload_bytes(&self, default_mb: u64) -> u64 {
        let mb = self
            .policy
            .max_upload_mb
            .unwrap_or(default_mb)
            .min(default_mb);
        mb.saturating_mul(1024 * 1024)
    }

    pub fn mcp_discovery(&self) -> Option<&str> {
        self.token_scope
            .as_ref()
            .and_then(|t| t.mcp_discovery.as_deref())
            .or(self.policy.mcp_discovery.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dogubako_core::spec::Category;

    fn spec(id: &str, cat: Category) -> ToolSpec {
        ToolSpec::new(id, id, cat)
    }

    #[test]
    fn patterns() {
        let s = spec("image.convert", Category::Image);
        assert!(pattern_matches("*", &s));
        assert!(pattern_matches("image.*", &s));
        assert!(pattern_matches("category:image", &s));
        assert!(pattern_matches("image.convert", &s));
        assert!(!pattern_matches("image.conv", &s));
        assert!(!pattern_matches("video.*", &s));
    }

    #[test]
    fn agent_least_privilege_and_scopes() {
        let img = spec("image.resize", Category::Image);
        let sys = spec("system.battery", Category::System);
        let agent = Principal {
            user_id: "a".into(),
            username: "imgbot".into(),
            role: Role::Agent,
            policy: Policy {
                allow: vec!["image.*".into()],
                deny: vec!["image.ocr".into()],
                ..Default::default()
            },
            token_scope: None,
            token_id: None,
            via: "token",
            csrf: None,
        };
        assert!(agent.can_use(&img));
        assert!(!agent.can_use(&sys));
        assert!(!agent.can_use(&spec("image.ocr", Category::Image)));
        assert!(agent.permissions(true).0.is_empty());
        assert!(!Principal {
            policy: Policy::for_role(Role::Agent),
            ..agent.clone()
        }
        .can_use(&img));

        // token narrower than the user
        let mut admin = Principal {
            role: Role::Admin,
            policy: Policy::for_role(Role::Admin),
            ..agent
        };
        admin.token_scope = Some(Policy {
            allow: vec!["image.resize".into()],
            permissions: vec![Permission::Network],
            ..Default::default()
        });
        assert!(admin.can_use(&img));
        assert!(!admin.can_use(&sys));
        assert!(!admin.is_admin(), "scoped tokens do not carry admin rights");
        let mut wildcard = admin.clone();
        wildcard.token_scope = Some(Policy {
            allow: vec!["*".into()],
            deny: vec!["system.*".into()],
            ..Default::default()
        });
        assert!(
            !wildcard.is_admin(),
            "a wildcard-scoped token still carries no admin rights"
        );
        wildcard.token_scope = None;
        assert!(wildcard.is_admin());
        let perms = admin.permissions(false);
        assert!(perms.has(Permission::Network));
        assert!(!perms.has(Permission::SystemInfo));
        assert!(!perms.has(Permission::HostFsRead));
    }
}
