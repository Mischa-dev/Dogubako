//! SQLite persistence (users, sessions, tokens, files, jobs, audit log).

use crate::authz::{Policy, Role};
use dogubako_mcp::{FileView, JobStatus, JobView};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UserRow {
    pub id: String,
    pub username: String,
    #[serde(skip)]
    pub password_hash: Option<String>,
    pub role: Role,
    pub policy: Policy,
    pub disabled: bool,
    pub created_at: i64,
    pub last_seen_at: Option<i64>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenRow {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub name: String,
    pub prefix: String,
    pub scope: Option<Policy>,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub revoked: bool,
}

#[derive(Debug, Clone)]
pub struct StoredFile {
    pub view: FileView,
    pub owner_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditRow {
    pub id: i64,
    pub ts: i64,
    pub actor: String,
    pub action: String,
    pub target: String,
    pub outcome: String,
    pub ip: String,
    pub detail: String,
}

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT,
    role TEXT NOT NULL,
    policy TEXT NOT NULL,
    disabled INTEGER NOT NULL DEFAULT 0,
    description TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER
);
CREATE TABLE IF NOT EXISTS sessions (
    id_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    csrf TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    ip TEXT NOT NULL DEFAULT '',
    user_agent TEXT NOT NULL DEFAULT ''
);
CREATE TABLE IF NOT EXISTS tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    scope TEXT,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    expires_at INTEGER,
    revoked INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    name TEXT NOT NULL,
    mime TEXT NOT NULL,
    size INTEGER NOT NULL,
    path TEXT NOT NULL,
    source TEXT NOT NULL,
    job_id TEXT,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);
CREATE INDEX IF NOT EXISTS files_owner ON files(owner_id);
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL,
    tool_id TEXT NOT NULL,
    status TEXT NOT NULL,
    params TEXT NOT NULL,
    result TEXT,
    error TEXT,
    via TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER
);
CREATE INDEX IF NOT EXISTS jobs_owner ON jobs(owner_id, created_at);
CREATE TABLE IF NOT EXISTS audit (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    ts INTEGER NOT NULL,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT NOT NULL DEFAULT '',
    outcome TEXT NOT NULL,
    ip TEXT NOT NULL DEFAULT '',
    detail TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS audit_ts ON audit(ts);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
"#];

fn user_from_row(r: &Row) -> rusqlite::Result<UserRow> {
    let role: String = r.get("role")?;
    let policy: String = r.get("policy")?;
    Ok(UserRow {
        id: r.get("id")?,
        username: r.get("username")?,
        password_hash: r.get("password_hash")?,
        role: Role::parse(&role).unwrap_or(Role::User),
        policy: serde_json::from_str(&policy).unwrap_or_default(),
        disabled: r.get::<_, i64>("disabled")? != 0,
        created_at: r.get("created_at")?,
        last_seen_at: r.get("last_seen_at")?,
        description: r.get("description")?,
    })
}

fn file_from_row(r: &Row) -> rusqlite::Result<StoredFile> {
    Ok(StoredFile {
        view: FileView {
            id: r.get("id")?,
            name: r.get("name")?,
            mime: r.get("mime")?,
            size: r.get::<_, i64>("size")? as u64,
            created_at: r.get("created_at")?,
            expires_at: r.get("expires_at")?,
            source: r.get("source")?,
            job_id: r.get("job_id")?,
            url: None,
        },
        owner_id: r.get("owner_id")?,
        path: r.get("path")?,
    })
}

fn job_from_row(r: &Row) -> rusqlite::Result<JobView> {
    let params: String = r.get("params")?;
    let result: Option<String> = r.get("result")?;
    let error: Option<String> = r.get("error")?;
    let status: String = r.get("status")?;
    Ok(JobView {
        id: r.get("id")?,
        tool_id: r.get("tool_id")?,
        status: JobStatus::parse(&status),
        progress: None,
        message: None,
        result: result.and_then(|s| serde_json::from_str(&s).ok()),
        error: error.and_then(|s| serde_json::from_str(&s).ok()),
        created_at: r.get("created_at")?,
        finished_at: r.get("finished_at")?,
        params: serde_json::from_str(&params).unwrap_or_default(),
        owner: Some(r.get("owner_id")?),
    })
}

impl Db {
    pub fn open(path: &Path) -> rusqlite::Result<Db> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn memory() -> rusqlite::Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> rusqlite::Result<Db> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000; PRAGMA synchronous=NORMAL;")?;
        for m in MIGRATIONS {
            conn.execute_batch(m)?;
        }
        Ok(Db {
            conn: Mutex::new(conn),
        })
    }

    fn c(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ---- users
    pub fn user_count(&self) -> i64 {
        self.c()
            .query_row(
                "SELECT COUNT(*) FROM users WHERE role != 'agent'",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0)
    }

    pub fn create_user(
        &self,
        id: &str,
        username: &str,
        password_hash: Option<&str>,
        role: Role,
        policy: &Policy,
        description: &str,
    ) -> rusqlite::Result<()> {
        self.c().execute(
            "INSERT INTO users (id, username, password_hash, role, policy, description, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, username, password_hash, role.as_str(), serde_json::to_string(policy).unwrap(), description, now()],
        )?;
        Ok(())
    }

    pub fn user_by_name(&self, username: &str) -> Option<UserRow> {
        self.c()
            .query_row(
                "SELECT * FROM users WHERE username = ?1",
                [username],
                user_from_row,
            )
            .optional()
            .ok()
            .flatten()
    }

    pub fn user(&self, id: &str) -> Option<UserRow> {
        self.c()
            .query_row("SELECT * FROM users WHERE id = ?1", [id], user_from_row)
            .optional()
            .ok()
            .flatten()
    }

    pub fn users(&self) -> Vec<UserRow> {
        let c = self.c();
        let mut st = c
            .prepare("SELECT * FROM users ORDER BY role, username")
            .unwrap();
        st.query_map([], user_from_row)
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
    }

    pub fn update_user(&self, u: &UserRow) -> rusqlite::Result<()> {
        self.c().execute(
            "UPDATE users SET role = ?2, policy = ?3, disabled = ?4, description = ?5 WHERE id = ?1",
            params![u.id, u.role.as_str(), serde_json::to_string(&u.policy).unwrap(), u.disabled as i64, u.description],
        )?;
        Ok(())
    }

    pub fn set_password(&self, id: &str, hash: &str) -> rusqlite::Result<()> {
        let c = self.c();
        c.execute(
            "UPDATE users SET password_hash = ?2 WHERE id = ?1",
            params![id, hash],
        )?;
        c.execute("DELETE FROM sessions WHERE user_id = ?1", [id])?;
        Ok(())
    }

    pub fn delete_user(&self, id: &str) -> rusqlite::Result<()> {
        self.c().execute("DELETE FROM users WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn touch_user(&self, id: &str) {
        let _ = self.c().execute(
            "UPDATE users SET last_seen_at = ?2 WHERE id = ?1",
            params![id, now()],
        );
    }

    // ---- sessions
    pub fn create_session(
        &self,
        id_hash: &str,
        user_id: &str,
        csrf: &str,
        ttl_secs: i64,
        ip: &str,
        ua: &str,
    ) -> rusqlite::Result<()> {
        let n = now();
        self.c().execute(
            "INSERT INTO sessions (id_hash, user_id, csrf, created_at, expires_at, ip, user_agent) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id_hash, user_id, csrf, n, n + ttl_secs, ip, ua],
        )?;
        Ok(())
    }

    /// Returns (user, csrf) for a valid session.
    pub fn session_user(&self, id_hash: &str) -> Option<(UserRow, String)> {
        let c = self.c();
        let row: Option<(String, String)> = c
            .query_row(
                "SELECT user_id, csrf FROM sessions WHERE id_hash = ?1 AND expires_at > ?2",
                params![id_hash, now()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .ok()
            .flatten();
        let (uid, csrf) = row?;
        let user = c
            .query_row("SELECT * FROM users WHERE id = ?1", [uid], user_from_row)
            .optional()
            .ok()
            .flatten()?;
        Some((user, csrf))
    }

    pub fn delete_session(&self, id_hash: &str) {
        let _ = self
            .c()
            .execute("DELETE FROM sessions WHERE id_hash = ?1", [id_hash]);
    }

    // ---- tokens
    #[allow(clippy::too_many_arguments)]
    pub fn create_token(
        &self,
        id: &str,
        user_id: &str,
        name: &str,
        hash: &str,
        prefix: &str,
        scope: Option<&Policy>,
        expires_at: Option<i64>,
    ) -> rusqlite::Result<()> {
        self.c().execute(
            "INSERT INTO tokens (id, user_id, name, token_hash, prefix, scope, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, user_id, name, hash, prefix, scope.map(|s| serde_json::to_string(s).unwrap()), now(), expires_at],
        )?;
        Ok(())
    }

    /// Returns (user, token id, scope) for a valid, unrevoked, unexpired token.
    pub fn token_user(&self, hash: &str) -> Option<(UserRow, String, Option<Policy>)> {
        let c = self.c();
        let row: Option<(String, String, Option<String>)> = c
            .query_row(
                "SELECT id, user_id, scope FROM tokens WHERE token_hash = ?1 AND revoked = 0 AND (expires_at IS NULL OR expires_at > ?2)",
                params![hash, now()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .ok()
            .flatten();
        let (tid, uid, scope) = row?;
        let user = c
            .query_row("SELECT * FROM users WHERE id = ?1", [uid], user_from_row)
            .optional()
            .ok()
            .flatten()?;
        let _ = c.execute(
            "UPDATE tokens SET last_used_at = ?2 WHERE id = ?1",
            params![tid, now()],
        );
        Some((user, tid, scope.and_then(|s| serde_json::from_str(&s).ok())))
    }

    pub fn tokens(&self, user_id: Option<&str>) -> Vec<TokenRow> {
        let c = self.c();
        let sql = "SELECT t.*, u.username FROM tokens t JOIN users u ON u.id = t.user_id WHERE (?1 IS NULL OR t.user_id = ?1) ORDER BY t.created_at DESC";
        let mut st = c.prepare(sql).unwrap();
        st.query_map([user_id], |r| {
            let scope: Option<String> = r.get("scope")?;
            Ok(TokenRow {
                id: r.get("id")?,
                user_id: r.get("user_id")?,
                username: r.get("username")?,
                name: r.get("name")?,
                prefix: r.get("prefix")?,
                scope: scope.and_then(|s| serde_json::from_str(&s).ok()),
                created_at: r.get("created_at")?,
                last_used_at: r.get("last_used_at")?,
                expires_at: r.get("expires_at")?,
                revoked: r.get::<_, i64>("revoked")? != 0,
            })
        })
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
    }

    pub fn token_owner(&self, id: &str) -> Option<String> {
        self.c()
            .query_row("SELECT user_id FROM tokens WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten()
    }

    pub fn revoke_token(&self, id: &str) -> rusqlite::Result<usize> {
        self.c()
            .execute("UPDATE tokens SET revoked = 1 WHERE id = ?1", [id])
    }

    // ---- files
    pub fn insert_file(&self, f: &StoredFile) -> rusqlite::Result<()> {
        self.c().execute(
            "INSERT INTO files (id, owner_id, name, mime, size, path, source, job_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![f.view.id, f.owner_id, f.view.name, f.view.mime, f.view.size as i64, f.path, f.view.source, f.view.job_id, f.view.created_at, f.view.expires_at],
        )?;
        Ok(())
    }

    pub fn file(&self, id: &str) -> Option<StoredFile> {
        self.c()
            .query_row("SELECT * FROM files WHERE id = ?1", [id], file_from_row)
            .optional()
            .ok()
            .flatten()
    }

    pub fn files(&self, owner: &str, limit: i64) -> Vec<StoredFile> {
        let c = self.c();
        let mut st = c
            .prepare("SELECT * FROM files WHERE owner_id = ?1 ORDER BY created_at DESC LIMIT ?2")
            .unwrap();
        st.query_map(params![owner, limit], file_from_row)
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
    }

    pub fn files_for_job(&self, job_id: &str) -> Vec<StoredFile> {
        let c = self.c();
        let mut st = c
            .prepare("SELECT * FROM files WHERE job_id = ?1 ORDER BY rowid")
            .unwrap();
        st.query_map([job_id], file_from_row)
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
    }

    pub fn delete_file(&self, id: &str) -> rusqlite::Result<()> {
        self.c().execute("DELETE FROM files WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn storage_used(&self, owner: &str) -> u64 {
        self.c()
            .query_row(
                "SELECT COALESCE(SUM(size), 0) FROM files WHERE owner_id = ?1",
                [owner],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(0) as u64
    }

    pub fn expired_files(&self) -> Vec<StoredFile> {
        let c = self.c();
        let mut st = c
            .prepare("SELECT * FROM files WHERE expires_at IS NOT NULL AND expires_at < ?1")
            .unwrap();
        st.query_map([now()], file_from_row)
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
    }

    pub fn files_of_owner(&self, owner: &str) -> Vec<StoredFile> {
        self.files(owner, i64::MAX)
    }

    // ---- jobs
    pub fn insert_job(&self, j: &JobView, owner: &str, via: &str) -> rusqlite::Result<()> {
        self.c().execute(
            "INSERT INTO jobs (id, owner_id, tool_id, status, params, via, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![j.id, owner, j.tool_id, j.status.as_str(), j.params.to_string(), via, j.created_at],
        )?;
        Ok(())
    }

    pub fn update_job(&self, j: &JobView) -> rusqlite::Result<()> {
        self.c().execute(
            "UPDATE jobs SET status = ?2, result = ?3, error = ?4, finished_at = ?5, started_at = COALESCE(started_at, CASE WHEN ?2 = 'running' THEN ?6 END) WHERE id = ?1",
            params![
                j.id,
                j.status.as_str(),
                j.result.as_ref().map(|r| serde_json::to_string(r).unwrap()),
                j.error.as_ref().map(|e| serde_json::to_string(e).unwrap()),
                j.finished_at,
                now()
            ],
        )?;
        Ok(())
    }

    pub fn job(&self, id: &str) -> Option<JobView> {
        self.c()
            .query_row("SELECT * FROM jobs WHERE id = ?1", [id], job_from_row)
            .optional()
            .ok()
            .flatten()
    }

    pub fn jobs(&self, owner: Option<&str>, limit: i64) -> Vec<JobView> {
        let c = self.c();
        let mut st = c.prepare("SELECT * FROM jobs WHERE (?1 IS NULL OR owner_id = ?1) ORDER BY created_at DESC LIMIT ?2").unwrap();
        st.query_map(params![owner, limit], job_from_row)
            .map(|it| it.flatten().collect())
            .unwrap_or_default()
    }

    pub fn delete_job(&self, id: &str) -> rusqlite::Result<()> {
        self.c().execute("DELETE FROM jobs WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Mark jobs left running by a previous process as failed.
    pub fn fail_orphaned_jobs(&self) -> usize {
        let err = serde_json::to_string(&dogubako_core::error::ToolError::failed(
            "server restarted while the job was running",
        ))
        .unwrap();
        self.c()
            .execute("UPDATE jobs SET status = 'failed', error = ?1, finished_at = ?2 WHERE status IN ('queued', 'running')", params![err, now()])
            .unwrap_or(0)
    }

    pub fn purge_jobs_before(&self, ts: i64) -> usize {
        self.c()
            .execute(
                "DELETE FROM jobs WHERE created_at < ?1 AND status NOT IN ('queued','running')",
                [ts],
            )
            .unwrap_or(0)
    }

    pub fn purge_sessions(&self) -> usize {
        self.c()
            .execute("DELETE FROM sessions WHERE expires_at < ?1", [now()])
            .unwrap_or(0)
    }

    // ---- audit
    pub fn audit(
        &self,
        actor: &str,
        action: &str,
        target: &str,
        outcome: &str,
        ip: &str,
        detail: &str,
    ) {
        let _ = self.c().execute(
            "INSERT INTO audit (ts, actor, action, target, outcome, ip, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![now(), actor, action, target, outcome, ip, detail],
        );
    }

    pub fn audit_log(&self, limit: i64, before: Option<i64>) -> Vec<AuditRow> {
        let c = self.c();
        let mut st = c
            .prepare("SELECT * FROM audit WHERE (?2 IS NULL OR id < ?2) ORDER BY id DESC LIMIT ?1")
            .unwrap();
        st.query_map(params![limit, before], |r| {
            Ok(AuditRow {
                id: r.get("id")?,
                ts: r.get("ts")?,
                actor: r.get("actor")?,
                action: r.get("action")?,
                target: r.get("target")?,
                outcome: r.get("outcome")?,
                ip: r.get("ip")?,
                detail: r.get("detail")?,
            })
        })
        .map(|it| it.flatten().collect())
        .unwrap_or_default()
    }

    pub fn purge_audit_before(&self, ts: i64) -> usize {
        self.c()
            .execute("DELETE FROM audit WHERE ts < ?1", [ts])
            .unwrap_or(0)
    }

    // ---- settings
    pub fn setting(&self, key: &str) -> Option<String> {
        self.c()
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) {
        let _ = self.c().execute("INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value", params![key, value]);
    }
}
