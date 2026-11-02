//! Helpers shared by the system tools.

use dogubako_core::prelude::*;

/// Note added to every result when the engine runs as a hosted server.
pub fn finish(ctx: &ToolContext, mut data: Value, text: String) -> ToolOutput {
    let hosted = ctx.environment() == Environment::Server;
    if let Value::Object(m) = &mut data {
        m.insert("environment".into(), json!(ctx.environment()));
        if hosted {
            m.insert("host_note".into(), json!("Running hosted: these values describe the server that runs Dogubako, not your device."));
        }
    }
    let mut out = ToolOutput::text(text).with_data(data);
    if hosted {
        out = out.warn("Values describe the server host, not your device.");
    }
    out
}

pub fn rfc3339(unix: u64) -> Option<String> {
    chrono::DateTime::from_timestamp(unix as i64, 0)
        .map(|d| d.with_timezone(&chrono::Local).to_rfc3339())
}

pub fn human_duration(secs: u64) -> String {
    let (d, h, m, s) = (
        secs / 86400,
        secs % 86400 / 3600,
        secs % 3600 / 60,
        secs % 60,
    );
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

pub fn pct(part: u64, total: u64) -> Option<f64> {
    if total == 0 {
        None
    } else {
        Some((part as f64 / total as f64 * 1000.0).round() / 10.0)
    }
}

pub fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

pub fn table(columns: &[&str], rows: Vec<Vec<Value>>) -> Value {
    json!({ "columns": columns, "rows": rows })
}

pub fn sys_err(what: &str) -> impl Fn(sysinfo::Error) -> ToolError + '_ {
    move |e| match e {
        sysinfo::Error::Unsupported => {
            ToolError::unsupported(format!("{what} is not supported on this platform"))
        }
        other => ToolError::failed(format!("{what}: {other}")),
    }
}

/// Bytes value + human string.
pub fn bytes(v: u64) -> Value {
    json!({ "bytes": v, "human": human_size(v) })
}

/// Read a sysfs-style text file (trimmed), None if missing/unreadable/empty.
pub fn read_trim(path: &str) -> Option<String> {
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Is a program available on PATH? (existence only; never executed)
pub fn on_path(name: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for cand in [name.to_string(), format!("{name}.exe")] {
            let p = dir.join(&cand);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// CPU feature flags relevant to local ML / media workloads.
pub fn cpu_features() -> Vec<&'static str> {
    #[allow(unused_mut)]
    let mut f: Vec<&'static str> = vec![];
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    {
        macro_rules! chk {
            ($($name:tt),*) => { $( if std::arch::is_x86_feature_detected!($name) { f.push($name); } )* };
        }
        chk!(
            "sse2",
            "sse4.1",
            "sse4.2",
            "popcnt",
            "aes",
            "pclmulqdq",
            "avx",
            "avx2",
            "fma",
            "bmi1",
            "bmi2",
            "f16c",
            "sha",
            "avx512f",
            "avx512bw",
            "avx512vl",
            "avx512vnni",
            "vaes"
        );
    }
    #[cfg(target_arch = "aarch64")]
    {
        macro_rules! chk {
            ($($name:tt),*) => { $( if std::arch::is_aarch64_feature_detected!($name) { f.push($name); } )* };
        }
        chk!(
            "neon", "aes", "sha2", "sha3", "crc", "dotprod", "fp16", "i8mm", "bf16", "sve", "sve2"
        );
    }
    f
}
