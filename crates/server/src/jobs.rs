//! Job management: bounded global and per-principal concurrency, progress
//! broadcasting, cancellation, persistence and artifact registration.

use crate::authz::Principal;
use crate::db::{now, Db};
use crate::error::{ApiError, ApiResult};
use crate::storage::Storage;
use dogubako_core::engine::{Engine, RunRequest};
use dogubako_core::error::ToolError;
use dogubako_core::policy::FsPolicy;
use dogubako_core::tool::{ProgressEvent, ProgressFn};
use dogubako_mcp::{JobStatus, JobView, ResultView};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{watch, Semaphore};
use tokio_util::sync::CancellationToken;

struct Live {
    tx: watch::Sender<JobView>,
    cancel: CancellationToken,
    owner: String,
}

pub struct JobManager {
    db: Arc<Db>,
    engine: Arc<Engine>,
    storage: Arc<Storage>,
    jobs_dir: PathBuf,
    global: Arc<Semaphore>,
    per_user_default: usize,
    user_sems: Mutex<HashMap<String, Arc<Semaphore>>>,
    live: Mutex<HashMap<String, Live>>,
    host_fs_enabled: bool,
    max_job: Duration,
}

pub fn new_job_id() -> String {
    format!("j_{}", dogubako_core::util::random_id(12))
}

impl JobManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Arc<Db>,
        engine: Arc<Engine>,
        storage: Arc<Storage>,
        jobs_dir: PathBuf,
        max_concurrent: usize,
        per_user: usize,
        host_fs_enabled: bool,
        max_job: Duration,
    ) -> Self {
        let _ = std::fs::create_dir_all(&jobs_dir);
        JobManager {
            db,
            engine,
            storage,
            jobs_dir,
            global: Arc::new(Semaphore::new(max_concurrent.max(1))),
            per_user_default: per_user.max(1),
            user_sems: Mutex::new(HashMap::new()),
            live: Mutex::new(HashMap::new()),
            host_fs_enabled,
            max_job,
        }
    }

    pub fn engine(&self) -> &Arc<Engine> {
        &self.engine
    }

    fn user_sem(&self, p: &Principal) -> Arc<Semaphore> {
        let n = p.concurrency(self.per_user_default);
        let mut m = self.user_sems.lock().unwrap();
        m.entry(format!("{}:{n}", p.user_id))
            .or_insert_with(|| Arc::new(Semaphore::new(n)))
            .clone()
    }

    pub fn running_count(&self) -> usize {
        self.live.lock().unwrap().len()
    }

    /// Validate and enqueue a job; returns immediately with the queued view.
    pub fn submit(
        self: &Arc<Self>,
        principal: &Principal,
        tool_id: &str,
        params: Map<String, Value>,
        via: &str,
        quota_default_mb: u64,
    ) -> ApiResult<JobView> {
        let tool = self
            .engine
            .registry
            .find(tool_id)
            .ok_or_else(|| ApiError::not_found(format!("unknown tool '{tool_id}'")))?;
        let spec = tool.spec().clone();
        if !principal.can_use(&spec) {
            self.db.audit(
                &principal.username,
                "tool.denied",
                &spec.id,
                "denied",
                "",
                via,
            );
            return Err(ApiError::forbidden(format!(
                "you are not permitted to use '{}'",
                spec.id
            )));
        }
        if self.db.storage_used(&principal.user_id) >= principal.quota_bytes(quota_default_mb) {
            return Err(ApiError::too_large(
                "storage quota exceeded; delete some files first",
            ));
        }
        // Redact secrets before persisting parameters.
        let mut stored = params.clone();
        for p in &spec.params {
            if p.is_secret() && stored.contains_key(&p.name) {
                stored.insert(p.name.clone(), Value::String("••••••".into()));
            }
        }
        let view = JobView {
            id: new_job_id(),
            tool_id: spec.id.clone(),
            status: JobStatus::Queued,
            progress: None,
            message: None,
            result: None,
            error: None,
            created_at: now(),
            finished_at: None,
            params: Value::Object(stored),
            owner: Some(principal.user_id.clone()),
        };
        self.db.insert_job(&view, &principal.user_id, via)?;
        let (tx, _rx) = watch::channel(view.clone());
        let cancel = CancellationToken::new();
        self.live.lock().unwrap().insert(
            view.id.clone(),
            Live {
                tx: tx.clone(),
                cancel: cancel.clone(),
                owner: principal.user_id.clone(),
            },
        );

        let me = self.clone();
        let p = principal.clone();
        let job_id = view.id.clone();
        let via = via.to_string();
        tokio::spawn(async move {
            me.execute(job_id, p, params, tx, cancel, via).await;
        });
        Ok(view)
    }

    async fn execute(
        self: Arc<Self>,
        job_id: String,
        p: Principal,
        params: Map<String, Value>,
        tx: watch::Sender<JobView>,
        cancel: CancellationToken,
        via: String,
    ) {
        let user_sem = self.user_sem(&p);
        let permits = tokio::select! {
            r = async {
                let u = user_sem.acquire_owned().await;
                let g = self.global.clone().acquire_owned().await;
                (u, g)
            } => Some(r),
            _ = cancel.cancelled() => None,
        };
        let mut view = tx.borrow().clone();
        if permits.is_none() {
            view.status = JobStatus::Cancelled;
            view.error = Some(ToolError::Cancelled);
            view.finished_at = Some(now());
            self.finish(view, &tx);
            return;
        }
        view.status = JobStatus::Running;
        let _ = self.db.update_job(&view);
        tx.send_replace(view.clone());

        let out_dir = self.jobs_dir.join(&job_id);
        let tx_progress = tx.clone();
        let last = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
        let progress: ProgressFn = Arc::new(move |ev: ProgressEvent| {
            let mut l = last.lock().unwrap();
            let final_tick = ev.fraction.map(|f| f >= 1.0).unwrap_or(false);
            if l.elapsed() < Duration::from_millis(150) && !final_tick {
                return;
            }
            *l = Instant::now();
            tx_progress.send_modify(|v| {
                if let Some(f) = ev.fraction {
                    v.progress = Some(f);
                }
                if ev.message.is_some() {
                    v.message = ev.message.clone();
                }
            });
        });
        let resolver = self.storage.resolver(&p.user_id);
        let principal = p.clone();
        let authorize = move |s: &dogubako_core::spec::ToolSpec| principal.can_use(s);
        let fs = if self.host_fs_enabled {
            None
        } else {
            Some(FsPolicy::deny_all())
        };
        let result = self
            .engine
            .run(RunRequest {
                tool_id: &view.tool_id,
                params,
                resolver: &resolver,
                out_dir: out_dir.clone(),
                permissions: p.permissions(self.host_fs_enabled),
                authorize: Some(&authorize),
                progress: Some(progress),
                cancel: cancel.clone(),
                timeout: Some(self.max_job),
                fs,
            })
            .await;
        let mut view = tx.borrow().clone();
        view.finished_at = Some(now());
        match result {
            Ok((out, stats)) => {
                let mut artifacts = vec![];
                let mut err = None;
                for a in &out.artifacts {
                    match self.storage.register_artifact(&p.user_id, &job_id, a) {
                        Ok(v) => artifacts.push(v),
                        Err(e) => {
                            err = Some(ToolError::failed(format!("storing output failed: {e}")))
                        }
                    }
                }
                if let Some(e) = err {
                    view.status = JobStatus::Failed;
                    view.error = Some(e);
                } else {
                    view.status = JobStatus::Succeeded;
                    view.progress = Some(1.0);
                    view.result = Some(ResultView {
                        text: out.text,
                        text_mime: out.text_mime,
                        data: out.data,
                        artifacts,
                        warnings: out.warnings,
                        duration_ms: stats.duration_ms,
                    });
                }
            }
            Err(e) => {
                view.status = if matches!(e, ToolError::Cancelled) {
                    JobStatus::Cancelled
                } else {
                    JobStatus::Failed
                };
                view.error = Some(e);
            }
        }
        let _ = std::fs::remove_dir_all(&out_dir);
        let outcome = view.status.as_str().to_string();
        let detail = view
            .error
            .as_ref()
            .map(|e| format!("{via}: {e}"))
            .unwrap_or(via);
        self.db.audit(
            &p.username,
            "tool.run",
            &view.tool_id,
            &outcome,
            "",
            &detail,
        );
        drop(permits);
        self.finish(view, &tx);
    }

    fn finish(&self, view: JobView, tx: &watch::Sender<JobView>) {
        if let Err(e) = self.db.update_job(&view) {
            tracing::error!("failed to persist job {}: {e}", view.id);
        }
        tx.send_replace(view.clone());
        self.live.lock().unwrap().remove(&view.id);
    }

    /// Current state of a job owned by `owner`.
    pub fn get(&self, owner: &str, id: &str) -> Option<JobView> {
        if let Some(l) = self.live.lock().unwrap().get(id) {
            if l.owner == owner {
                return Some(l.tx.borrow().clone());
            }
            return None;
        }
        self.db
            .job(id)
            .filter(|j| j.owner.as_deref() == Some(owner))
    }

    /// Subscribe to updates of a live job.
    pub fn subscribe(&self, owner: &str, id: &str) -> Option<watch::Receiver<JobView>> {
        self.live
            .lock()
            .unwrap()
            .get(id)
            .filter(|l| l.owner == owner)
            .map(|l| l.tx.subscribe())
    }

    /// Wait up to `wait` for a job to reach a terminal state.
    pub async fn wait(&self, owner: &str, id: &str, wait: Duration) -> Option<JobView> {
        if let Some(mut rx) = self.subscribe(owner, id) {
            let deadline = tokio::time::Instant::now() + wait;
            loop {
                let v = rx.borrow().clone();
                if v.status.is_terminal() {
                    return Some(v);
                }
                match tokio::time::timeout_at(deadline, rx.changed()).await {
                    Ok(Ok(())) => continue,
                    Ok(Err(_)) => break, // sender dropped: finished
                    Err(_) => return Some(rx.borrow().clone()),
                }
            }
        }
        self.get(owner, id)
    }

    pub fn cancel(&self, owner: &str, id: &str) -> Option<JobView> {
        let live = self.live.lock().unwrap();
        if let Some(l) = live.get(id) {
            if l.owner != owner {
                return None;
            }
            l.cancel.cancel();
            return Some(l.tx.borrow().clone());
        }
        drop(live);
        self.get(owner, id)
    }

    pub fn cancel_all(&self) {
        for l in self.live.lock().unwrap().values() {
            l.cancel.cancel();
        }
    }
}
