//! Access-controlled file storage. Files are addressed by random ids, never
//! by host paths; every lookup checks ownership.

use crate::db::{now, Db, StoredFile};
use dogubako_core::error::{ToolError, ToolResult};
use dogubako_core::params::{FileResolver, InputFile};
use dogubako_core::tool::Artifact;
use dogubako_mcp::{ArtifactView, FileView};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct Storage {
    pub dir: PathBuf,
    pub db: Arc<Db>,
    pub retention_secs: i64,
}

pub fn new_file_id() -> String {
    format!("f_{}", dogubako_core::util::random_id(16))
}

pub fn valid_file_id(id: &str) -> bool {
    id.len() == 34 && id.starts_with("f_") && id[2..].chars().all(|c| c.is_ascii_hexdigit())
}

pub fn file_url(id: &str) -> String {
    format!("/api/v1/files/{id}/content")
}

impl Storage {
    pub fn new(dir: PathBuf, db: Arc<Db>, retention_hours: u64) -> std::io::Result<Self> {
        std::fs::create_dir_all(&dir)?;
        Ok(Storage { dir, db, retention_secs: (retention_hours as i64).max(1) * 3600 })
    }

    fn path_for(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    fn expiry(&self) -> Option<i64> {
        Some(now() + self.retention_secs)
    }

    /// Move a fully-written temporary file into storage.
    pub fn store(&self, owner: &str, name: &str, tmp: &Path, source: &str, job_id: Option<&str>) -> std::io::Result<FileView> {
        let id = new_file_id();
        let dest = self.path_for(&id);
        if std::fs::rename(tmp, &dest).is_err() {
            std::fs::copy(tmp, &dest)?;
            let _ = std::fs::remove_file(tmp);
        }
        let size = std::fs::metadata(&dest)?.len();
        let name = dogubako_core::policy::sanitize_filename(name);
        let mime = dogubako_core::util::mime_for_path(Path::new(&name));
        let mime = if mime == "application/octet-stream" { dogubako_core::util::mime_for_path(&dest) } else { mime };
        let view = FileView {
            id: id.clone(),
            name,
            mime,
            size,
            created_at: now(),
            expires_at: self.expiry(),
            source: source.into(),
            job_id: job_id.map(|s| s.to_string()),
            url: Some(file_url(&id)),
        };
        self.db
            .insert_file(&StoredFile { view: view.clone(), owner_id: owner.into(), path: dest.to_string_lossy().to_string() })
            .map_err(std::io::Error::other)?;
        Ok(view)
    }

    pub fn register_artifact(&self, owner: &str, job_id: &str, a: &Artifact) -> std::io::Result<ArtifactView> {
        let v = self.store(owner, &a.name, &a.path, "artifact", Some(job_id))?;
        Ok(ArtifactView { id: Some(v.id.clone()), name: v.name, mime: a.mime.clone(), size: v.size, label: a.label.clone(), url: v.url, path: None })
    }

    /// Lookup a file owned by `owner`.
    pub fn get(&self, owner: &str, id: &str) -> Option<StoredFile> {
        if !valid_file_id(id) {
            return None;
        }
        self.db.file(id).filter(|f| f.owner_id == owner).map(|mut f| {
            f.view.url = Some(file_url(&f.view.id));
            f
        })
    }

    pub fn list(&self, owner: &str) -> Vec<FileView> {
        self.db
            .files(owner, 1000)
            .into_iter()
            .map(|mut f| {
                f.view.url = Some(file_url(&f.view.id));
                f.view
            })
            .collect()
    }

    pub fn delete(&self, f: &StoredFile) -> std::io::Result<()> {
        let p = PathBuf::from(&f.path);
        if p.starts_with(&self.dir) {
            let _ = std::fs::remove_file(&p);
        }
        self.db.delete_file(&f.view.id).map_err(std::io::Error::other)
    }

    /// Delete expired files; returns count.
    pub fn cleanup(&self) -> usize {
        let expired = self.db.expired_files();
        let n = expired.len();
        for f in expired {
            let _ = self.delete(&f);
        }
        n
    }

    pub fn resolver(self: &Arc<Self>, owner: &str) -> StoredResolver {
        StoredResolver { storage: self.clone(), owner: owner.to_string() }
    }
}

/// Resolves file parameters given as stored file ids, enforcing ownership.
pub struct StoredResolver {
    storage: Arc<Storage>,
    owner: String,
}

impl FileResolver for StoredResolver {
    fn resolve(&self, param: &str, value: &str) -> ToolResult<InputFile> {
        let id = value.trim().trim_start_matches("dogubako://files/");
        if !valid_file_id(id) {
            return Err(ToolError::invalid(param, format!("'{value}' is not a file id (upload the file first and pass its id, e.g. f_…)")));
        }
        let f = self.storage.get(&self.owner, id).ok_or_else(|| ToolError::invalid(param, format!("file '{id}' not found")))?;
        let path = PathBuf::from(&f.path);
        if !path.is_file() {
            return Err(ToolError::invalid(param, format!("file '{id}' is no longer available")));
        }
        Ok(InputFile { path, name: f.view.name })
    }
}
