//! Safe external process execution.
//!
//! * Argument vectors only — never a shell.
//! * Cleared environment with a small allow-list.
//! * Isolated working directory and private TMPDIR.
//! * Wall-clock timeout + cancellation, killing the whole process group.
//! * Unix resource limits (address space, CPU time, file size).
//! * Optional bubblewrap sandbox on Linux.
//! * Bounded stdout / stderr capture.

use crate::error::{ToolError, ToolResult};
use crate::policy::SandboxMode;
use crate::tool::ToolContext;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

const STDERR_TAIL: usize = 16 * 1024;

const ENV_PASSTHROUGH: &[&str] = &[
    "PATH",
    "SYSTEMROOT",
    "SystemRoot",
    "WINDIR",
    "PATHEXT",
    "COMSPEC",
    "TESSDATA_PREFIX",
    "FONTCONFIG_PATH",
    "FONTCONFIG_FILE",
    "LD_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "XDG_DATA_DIRS",
    "MAGICK_CONFIGURE_PATH",
    "MAGICK_HOME",
    "GS_LIB",
    "PROGRAMFILES",
    "LOCALAPPDATA",
];

#[cfg(unix)]
macro_rules! set_rlimit {
    ($res:expr, $v:expr) => {{
        let v = $v;
        let lim = libc::rlimit {
            rlim_cur: v as libc::rlim_t,
            rlim_max: v as libc::rlim_t,
        };
        libc::setrlimit($res, &lim);
    }};
}

type LineFn = Box<dyn FnMut(&str) + Send + 'static>;

pub struct ExecOutput {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
}

impl ExecOutput {
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

/// Builder for an external command. Create with [`ToolContext::command`].
pub struct Exec {
    ctx: ToolContext,
    program: PathBuf,
    display: String,
    args: Vec<OsString>,
    stdin: Option<Vec<u8>>,
    timeout: Option<Duration>,
    env: Vec<(String, OsString)>,
    cwd: Option<PathBuf>,
    on_stdout: Option<LineFn>,
    on_stderr: Option<LineFn>,
    ok_codes: Vec<i32>,
    ro_binds: Vec<PathBuf>,
    allow_network: bool,
}

impl Exec {
    pub(crate) fn new(ctx: ToolContext, program: PathBuf, display: String) -> Self {
        Exec {
            ctx,
            program,
            display,
            args: vec![],
            stdin: None,
            timeout: None,
            env: vec![],
            cwd: None,
            on_stdout: None,
            on_stderr: None,
            ok_codes: vec![0],
            ro_binds: vec![],
            allow_network: false,
        }
    }

    pub fn arg(mut self, a: impl AsRef<OsStr>) -> Self {
        self.args.push(a.as_ref().to_os_string());
        self
    }
    pub fn args<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(it.into_iter().map(|s| s.as_ref().to_os_string()));
        self
    }
    /// Add an argument only when `cond` is true.
    pub fn arg_if(self, cond: bool, a: impl AsRef<OsStr>) -> Self {
        if cond {
            self.arg(a)
        } else {
            self
        }
    }
    pub fn stdin(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(data.into());
        self
    }
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }
    pub fn env(mut self, k: &str, v: impl AsRef<OsStr>) -> Self {
        self.env.push((k.to_string(), v.as_ref().to_os_string()));
        self
    }
    pub fn current_dir(mut self, p: impl Into<PathBuf>) -> Self {
        self.cwd = Some(p.into());
        self
    }
    /// Stream stdout lines to a callback instead of capturing.
    pub fn on_stdout_line(mut self, f: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_stdout = Some(Box::new(f));
        self
    }
    /// Observe stderr lines (stderr is still captured, tail only).
    pub fn on_stderr_line(mut self, f: impl FnMut(&str) + Send + 'static) -> Self {
        self.on_stderr = Some(Box::new(f));
        self
    }
    /// Extra exit codes considered successful.
    pub fn ok_codes(mut self, codes: &[i32]) -> Self {
        self.ok_codes.extend_from_slice(codes);
        self
    }
    /// Additional read-only path to expose inside the sandbox.
    pub fn readable(mut self, p: impl Into<PathBuf>) -> Self {
        self.ro_binds.push(p.into());
        self
    }
    /// Allow network inside the sandbox (default: denied when sandboxed).
    pub fn allow_network(mut self) -> Self {
        self.allow_network = true;
        self
    }

    /// Run and require a successful exit status.
    pub async fn run(self) -> ToolResult<ExecOutput> {
        let ok = self.ok_codes.clone();
        let display = self.display.clone();
        let out = self.run_unchecked().await?;
        match out.exit_code {
            Some(c) if ok.contains(&c) => Ok(out),
            code => Err(ToolError::External {
                program: display,
                exit_code: code,
                stderr: summarize_stderr(&out.stderr),
            }),
        }
    }

    /// Run and return output whatever the exit status.
    pub async fn run_unchecked(mut self) -> ToolResult<ExecOutput> {
        self.ctx.check_cancelled()?;
        let ctx = self.ctx.clone();
        let limits = ctx.limits().clone();
        let work = ctx.work_dir().to_path_buf();
        let tmp = work.join(".tmp");
        let _ = std::fs::create_dir_all(&tmp);
        let cwd = self.cwd.clone().unwrap_or_else(|| work.clone());

        let (program, mut args): (PathBuf, Vec<OsString>) = match limits.sandbox {
            SandboxMode::Bwrap if cfg!(target_os = "linux") => {
                let bwrap =
                    ctx.deps()
                        .path("bwrap")
                        .ok_or_else(|| ToolError::MissingDependency {
                            dependency: "bwrap".into(),
                            hint: "sandbox mode 'bwrap' requires bubblewrap".into(),
                        })?;
                let mut a: Vec<OsString> = vec![];
                let mut push = |s: &str| a.push(s.into());
                push("--die-with-parent");
                push("--new-session");
                push("--unshare-pid");
                push("--unshare-ipc");
                push("--unshare-uts");
                if !self.allow_network {
                    push("--unshare-net");
                }
                push("--proc");
                push("/proc");
                push("--dev");
                push("/dev");
                push("--tmpfs");
                push("/tmp");
                for sys in [
                    "/usr", "/bin", "/sbin", "/lib", "/lib64", "/lib32", "/etc", "/opt", "/nix",
                ] {
                    if Path::new(sys).exists() {
                        a.push("--ro-bind".into());
                        a.push(sys.into());
                        a.push(sys.into());
                    }
                }
                let mut ro: Vec<PathBuf> = ctx.input_paths().to_vec();
                ro.extend(self.ro_binds.iter().cloned());
                ro.push(ctx.models_dir().to_path_buf());
                if let Some(parent) = self.program.parent() {
                    ro.push(parent.to_path_buf());
                }
                for p in ro {
                    if p.exists() {
                        a.push("--ro-bind".into());
                        a.push(p.clone().into());
                        a.push(p.into());
                    }
                }
                for rw in [work.clone(), ctx.out_dir().to_path_buf()] {
                    a.push("--bind".into());
                    a.push(rw.clone().into());
                    a.push(rw.into());
                }
                a.push("--chdir".into());
                a.push(cwd.clone().into());
                a.push("--".into());
                a.push(self.program.clone().into());
                (bwrap, a)
            }
            _ => (self.program.clone(), vec![]),
        };
        args.append(&mut self.args);

        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args)
            .current_dir(&cwd)
            .env_clear()
            .stdin(if self.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for k in ENV_PASSTHROUGH {
            if let Some(v) = std::env::var_os(k) {
                cmd.env(k, v);
            }
        }
        cmd.env("HOME", &work)
            .env("USERPROFILE", &work)
            .env("TMPDIR", &tmp)
            .env("TEMP", &tmp)
            .env("TMP", &tmp)
            .env("LANG", "C.UTF-8")
            .env("LC_ALL", "C.UTF-8")
            .env("OMP_THREAD_LIMIT", "8");
        for (k, v) in &self.env {
            cmd.env(k, v);
        }
        #[cfg(unix)]
        {
            cmd.process_group(0);
            let mem = limits.max_memory_mb;
            let cpu = limits.max_cpu_secs;
            let fsize = limits.max_file_size_mb;
            unsafe {
                cmd.pre_exec(move || {
                    set_rlimit!(libc::RLIMIT_CORE, 0u64);
                    if let Some(m) = mem {
                        set_rlimit!(libc::RLIMIT_AS, m.saturating_mul(1024 * 1024));
                    }
                    if let Some(c) = cpu {
                        set_rlimit!(libc::RLIMIT_CPU, c);
                    }
                    if let Some(f) = fsize {
                        set_rlimit!(libc::RLIMIT_FSIZE, f.saturating_mul(1024 * 1024));
                    }
                    Ok(())
                });
            }
        }

        // Argument values are never logged: some carry secrets (e.g. archive
        // passwords), and redaction by position cannot be done reliably.
        tracing::debug!(program = %self.display, arg_count = args.len(), "spawning");
        let mut child = cmd.spawn().map_err(|e| ToolError::External {
            program: self.display.clone(),
            exit_code: None,
            stderr: format!("failed to start: {e}"),
        })?;
        let pid = child.id();
        let _guard = KillGuard(pid);

        if let Some(data) = self.stdin.take() {
            let mut sin = child.stdin.take().expect("stdin piped");
            tokio::spawn(async move {
                let _ = sin.write_all(&data).await;
                let _ = sin.shutdown().await;
            });
        }

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");
        let max_stdout = limits.max_stdout_bytes;
        let on_stdout = self.on_stdout.take();
        let stdout_task = tokio::spawn(async move {
            match on_stdout {
                Some(mut f) => {
                    let mut lines = BufReader::new(stdout).lines();
                    while let Ok(Some(l)) = lines.next_line().await {
                        f(&l);
                    }
                    Ok(Vec::new())
                }
                None => {
                    let mut buf = Vec::new();
                    let mut reader = stdout.take(max_stdout as u64 + 1);
                    reader.read_to_end(&mut buf).await.ok();
                    if buf.len() > max_stdout {
                        Err(())
                    } else {
                        Ok(buf)
                    }
                }
            }
        });
        let mut on_stderr = self.on_stderr.take();
        let stderr_task = tokio::spawn(async move {
            let mut tail = String::new();
            let mut reader = BufReader::new(stderr);
            let mut line = Vec::new();
            loop {
                line.clear();
                // Split on \n and \r (ffmpeg uses \r for status lines).
                match read_until_any(&mut reader, &mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        let s = String::from_utf8_lossy(&line);
                        let s = s.trim_end_matches(['\r', '\n']);
                        if let Some(f) = on_stderr.as_mut() {
                            f(s);
                        }
                        tail.push_str(s);
                        tail.push('\n');
                        if tail.len() > STDERR_TAIL * 2 {
                            let cut = tail.len() - STDERR_TAIL;
                            let cut = (cut..tail.len())
                                .find(|i| tail.is_char_boundary(*i))
                                .unwrap_or(cut);
                            tail.drain(..cut);
                        }
                    }
                    Err(_) => break,
                }
            }
            tail
        });

        let timeout = self
            .timeout
            .unwrap_or(Duration::from_secs(limits.max_timeout_secs));
        let cancel = ctx.cancel_token();
        let status = tokio::select! {
            s = child.wait() => s.map_err(|e| ToolError::failed(format!("waiting for {}: {e}", self.display)))?,
            _ = cancel.cancelled() => {
                kill_tree(pid);
                let _ = child.kill().await;
                return Err(ToolError::Cancelled);
            }
            _ = tokio::time::sleep(timeout) => {
                kill_tree(pid);
                let _ = child.kill().await;
                return Err(ToolError::Timeout { seconds: timeout.as_secs() });
            }
        };
        let stdout = stdout_task
            .await
            .map_err(|e| ToolError::failed(e.to_string()))?
            .map_err(|_| {
                ToolError::limit(format!(
                    "{} produced more than {max_stdout} bytes of output",
                    self.display
                ))
            })?;
        let stderr = stderr_task.await.unwrap_or_default();
        #[cfg(unix)]
        let exit_code = {
            use std::os::unix::process::ExitStatusExt;
            status.code().or_else(|| status.signal().map(|s| 128 + s))
        };
        #[cfg(not(unix))]
        let exit_code = status.code();
        if let (Some(_), Some(code)) = (limits.max_memory_mb, exit_code) {
            if code == 128 + 9
                || stderr.contains("std::bad_alloc")
                || stderr.contains("Cannot allocate memory")
            {
                return Err(ToolError::limit(format!(
                    "{} exceeded its memory limit",
                    self.display
                )));
            }
        }
        Ok(ExecOutput {
            exit_code,
            stdout,
            stderr,
        })
    }
}

async fn read_until_any<R: tokio::io::AsyncBufRead + Unpin>(
    r: &mut R,
    out: &mut Vec<u8>,
) -> std::io::Result<usize> {
    let mut total = 0;
    loop {
        let (done, used) = {
            let available = r.fill_buf().await?;
            if available.is_empty() {
                return Ok(total);
            }
            match available.iter().position(|b| *b == b'\n' || *b == b'\r') {
                Some(i) => {
                    out.extend_from_slice(&available[..=i]);
                    (true, i + 1)
                }
                None => {
                    out.extend_from_slice(available);
                    (false, available.len())
                }
            }
        };
        r.consume(used);
        total += used;
        if done || out.len() > 64 * 1024 {
            return Ok(total);
        }
    }
}

fn summarize_stderr(s: &str) -> String {
    let lines: Vec<&str> = s.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(12);
    lines[start..].join("\n")
}

/// Kill a process group (Unix) when the guard is dropped, e.g. because the
/// tool future was aborted by timeout or cancellation.
struct KillGuard(Option<u32>);

impl Drop for KillGuard {
    fn drop(&mut self) {
        kill_tree(self.0);
    }
}

fn kill_tree(pid: Option<u32>) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        unsafe {
            // Negative pid => process group created via process_group(0).
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    let _ = pid;
}

/// Parse FFmpeg-style `HH:MM:SS.ms` or seconds into seconds.
pub fn parse_timestamp(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(v) = s.parse::<f64>() {
        return Some(v);
    }
    let parts: Vec<&str> = s.split(':').collect();
    let mut total = 0.0;
    for p in &parts {
        total = total * 60.0 + p.parse::<f64>().ok()?;
    }
    Some(total)
}
