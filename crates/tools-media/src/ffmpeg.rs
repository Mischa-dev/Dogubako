//! Shared FFmpeg / FFprobe helpers: probing, running with progress,
//! capability (encoder / filter / hwaccel) detection, encoder selection and
//! small utilities used by every media tool.

use dogubako_core::prelude::*;
use dogubako_core::process::parse_timestamp;
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Dependencies of every FFmpeg based tool.
pub const FF: &[&str] = &["ffmpeg", "ffprobe"];

pub const VIDEO_ACCEPT: &[&str] = &[
    "video/*", ".mkv", ".mp4", ".mov", ".webm", ".avi", ".m4v", ".ts", ".mts", ".m2ts", ".flv",
    ".wmv", ".3gp", ".mpg", ".mpeg", ".ogv", ".gif",
];
pub const AUDIO_ACCEPT: &[&str] = &[
    "audio/*", ".mp3", ".m4a", ".aac", ".wav", ".flac", ".ogg", ".oga", ".opus", ".wma", ".aiff",
    ".aif", ".mka", ".ac3", ".amr", ".caf",
];
pub const MEDIA_ACCEPT: &[&str] = &[
    "audio/*", "video/*", ".mp3", ".m4a", ".aac", ".wav", ".flac", ".ogg", ".oga", ".opus", ".wma",
    ".aiff", ".aif", ".mka", ".ac3", ".amr", ".caf", ".mkv", ".mp4", ".mov", ".webm", ".avi",
    ".m4v", ".ts", ".mts", ".m2ts", ".flv", ".wmv", ".3gp", ".mpg", ".mpeg", ".ogv",
];
pub const SUB_ACCEPT: &[&str] = &[".srt", ".vtt", ".ass", ".ssa"];

/// Turn a path into an argument that can never be mistaken for an option or
/// protocol: absolute paths are used as-is, relative ones are made absolute
/// (or prefixed with `./`).
pub fn arg_path(p: &Path) -> OsString {
    if p.is_absolute() {
        return p.as_os_str().to_owned();
    }
    match std::path::absolute(p) {
        Ok(a) => a.into_os_string(),
        Err(_) => {
            let mut s = OsString::from("./");
            s.push(p);
            s
        }
    }
}

/// Seconds formatted for FFmpeg arguments.
pub fn secs(v: f64) -> String {
    format!("{:.3}", v.max(0.0))
}

/// `HH:MM:SS.mmm` for display.
pub fn hms(v: f64) -> String {
    let v = v.max(0.0);
    let ms = (v * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}.{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

/// Parse an optional timestamp parameter (`90`, `1:30`, `00:01:30.5`).
pub fn opt_time(p: &Params, name: &str) -> ToolResult<Option<f64>> {
    match p.opt_str(name).map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => match parse_timestamp(s) {
            Some(v) if v.is_finite() && v >= 0.0 => Ok(Some(v)),
            _ => Err(ToolError::invalid(
                name,
                format!("'{s}' is not a timestamp (use seconds or HH:MM:SS.mmm)"),
            )),
        },
    }
}

/// Parse a comma/space separated list of timestamps.
pub fn time_list(p: &Params, name: &str) -> ToolResult<Vec<f64>> {
    let raw = p.opt_str(name).unwrap_or("");
    let mut out = vec![];
    for part in raw
        .split([',', ';', '\n', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        match parse_timestamp(part) {
            Some(v) if v.is_finite() && v >= 0.0 => out.push(v),
            _ => {
                return Err(ToolError::invalid(
                    name,
                    format!("'{part}' is not a timestamp"),
                ))
            }
        }
    }
    Ok(out)
}

/// Fraction span of file `i` of `n` in a batch.
pub fn span(i: usize, n: usize) -> (f32, f32) {
    let n = n.max(1) as f32;
    (i as f32 / n, (i as f32 + 1.0) / n)
}

/// Sub-span inside a span.
pub fn sub(s: (f32, f32), a: f32, b: f32) -> (f32, f32) {
    (s.0 + (s.1 - s.0) * a, s.0 + (s.1 - s.0) * b)
}

/// A unique output path named after an input file.
pub fn out_path(ctx: &ToolContext, file: &InputFile, suffix: &str, ext: &str) -> PathBuf {
    ctx.output_path(&suffixed(&file.name, suffix, Some(ext)).replace('%', "_"))
}

/// Lower-case extension of a path.
pub fn ext_of(p: &Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Running ffmpeg

/// Captured result of an ffmpeg run.
pub struct FfOut {
    /// Full stderr (only when `verbose()` was requested), line by line.
    pub log: Vec<String>,
}

/// Builder for an ffmpeg invocation with progress reporting.
#[derive(Clone)]
pub struct Ff {
    pre: Vec<OsString>,
    args: Vec<OsString>,
    duration: Option<f64>,
    span: (f32, f32),
    label: String,
    cwd: Option<PathBuf>,
    verbose: bool,
    progress: bool,
}

impl Default for Ff {
    fn default() -> Self {
        Self::new()
    }
}

impl Ff {
    pub fn new() -> Self {
        Ff {
            pre: vec![],
            args: vec![],
            duration: None,
            span: (0.0, 1.0),
            label: String::new(),
            cwd: None,
            verbose: false,
            progress: true,
        }
    }
    pub fn a(mut self, s: impl AsRef<OsStr>) -> Self {
        self.args.push(s.as_ref().to_os_string());
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
    pub fn input(self, p: &Path) -> Self {
        self.a("-i").a(arg_path(p))
    }
    pub fn out(self, p: &Path) -> Self {
        self.a(arg_path(p))
    }
    pub fn null_out(self) -> Self {
        self.args(["-f", "null", "-"])
    }
    /// Expected *output* duration in seconds (for progress fractions).
    pub fn duration(mut self, d: Option<f64>) -> Self {
        self.duration = d.filter(|d| *d > 0.0);
        self
    }
    pub fn span(mut self, s: (f32, f32), label: impl Into<String>) -> Self {
        self.span = s;
        self.label = label.into();
        self
    }
    pub fn cwd(mut self, p: impl Into<PathBuf>) -> Self {
        self.cwd = Some(p.into());
        self
    }
    /// Info-level log, captured completely (for detection filters).
    pub fn verbose(mut self) -> Self {
        self.verbose = true;
        self
    }
    pub fn no_progress(mut self) -> Self {
        self.progress = false;
        self
    }
    /// Arguments placed before everything else (global options).
    pub fn global(mut self, s: impl AsRef<OsStr>) -> Self {
        self.pre.push(s.as_ref().to_os_string());
        self
    }

    pub async fn run(self, ctx: &ToolContext) -> ToolResult<FfOut> {
        ctx.check_cancelled()?;
        let mut cmd = ctx
            .command("ffmpeg")?
            .args([
                "-hide_banner",
                "-nostdin",
                "-y",
                "-nostats",
                "-loglevel",
                if self.verbose { "info" } else { "error" },
            ])
            .args(&self.pre);
        if self.progress {
            cmd = cmd.args(["-progress", "pipe:1"]);
            let c = ctx.clone();
            let (lo, hi) = self.span;
            let dur = self.duration;
            let label = self.label.clone();
            c.progress(lo, label.clone());
            let mut last = -1.0f32;
            cmd = cmd.on_stdout_line(move |line| {
                let t = if let Some(v) = line.strip_prefix("out_time_us=") {
                    v.trim().parse::<f64>().ok().map(|us| us / 1e6)
                } else if let Some(v) = line.strip_prefix("out_time_ms=") {
                    // Despite the name, FFmpeg reports microseconds here.
                    v.trim().parse::<f64>().ok().map(|us| us / 1e6)
                } else {
                    if line.trim() == "progress=end" {
                        c.progress(hi, label.clone());
                    }
                    None
                };
                if let (Some(t), Some(d)) = (t, dur) {
                    let f = (t / d).clamp(0.0, 1.0) as f32;
                    if f - last >= 0.005 {
                        last = f;
                        let msg = if label.is_empty() {
                            format!("{:.0}%", f * 100.0)
                        } else {
                            format!("{label} – {:.0}%", f * 100.0)
                        };
                        c.progress(lo + (hi - lo) * f, msg);
                    }
                }
            });
        }
        let log = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
        if self.verbose {
            let l = log.clone();
            cmd = cmd.on_stderr_line(move |line| {
                let mut g = l.lock().unwrap();
                if g.len() < 2_000_000 {
                    g.push(line.to_string());
                }
            });
        }
        if let Some(c) = &self.cwd {
            cmd = cmd.current_dir(c);
        }
        cmd.args(&self.args).run().await?;
        let log = std::mem::take(&mut *log.lock().unwrap());
        Ok(FfOut { log })
    }
}

// ---------------------------------------------------------------------------
// Probing

#[derive(Debug, Clone)]
pub struct Probe {
    pub raw: Value,
}

impl Probe {
    pub fn streams(&self) -> &[Value] {
        self.raw["streams"]
            .as_array()
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }
    pub fn format(&self) -> &Value {
        &self.raw["format"]
    }
    pub fn duration(&self) -> Option<f64> {
        num(&self.format()["duration"])
            .filter(|d| *d > 0.0)
            .or_else(|| {
                self.streams()
                    .iter()
                    .filter_map(|s| num(&s["duration"]))
                    .fold(None, |a: Option<f64>, d| Some(a.map_or(d, |a| a.max(d))))
            })
    }
    fn is_attached_pic(s: &Value) -> bool {
        s["disposition"]["attached_pic"].as_i64() == Some(1)
    }
    /// First real video stream (not cover art).
    pub fn video(&self) -> Option<&Value> {
        self.streams()
            .iter()
            .find(|s| s["codec_type"] == "video" && !Self::is_attached_pic(s))
    }
    pub fn has_cover(&self) -> bool {
        self.streams()
            .iter()
            .any(|s| s["codec_type"] == "video" && Self::is_attached_pic(s))
    }
    pub fn audio(&self) -> Option<&Value> {
        self.streams().iter().find(|s| s["codec_type"] == "audio")
    }
    pub fn audio_streams(&self) -> Vec<&Value> {
        self.streams()
            .iter()
            .filter(|s| s["codec_type"] == "audio")
            .collect()
    }
    pub fn subtitle_streams(&self) -> Vec<&Value> {
        self.streams()
            .iter()
            .filter(|s| s["codec_type"] == "subtitle")
            .collect()
    }
    pub fn video_codec(&self) -> Option<&str> {
        self.video().and_then(|v| v["codec_name"].as_str())
    }
    pub fn audio_codec(&self) -> Option<&str> {
        self.audio().and_then(|v| v["codec_name"].as_str())
    }
    /// Rotation from the display matrix (degrees, as reported by ffprobe).
    pub fn rotation(&self) -> i64 {
        let Some(v) = self.video() else { return 0 };
        if let Some(list) = v["side_data_list"].as_array() {
            for sd in list {
                if let Some(r) = sd["rotation"].as_f64() {
                    return r.round() as i64;
                }
            }
        }
        v["tags"]["rotate"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
    /// Coded dimensions.
    pub fn coded_dims(&self) -> Option<(u32, u32)> {
        let v = self.video()?;
        Some((v["width"].as_u64()? as u32, v["height"].as_u64()? as u32))
    }
    /// Displayed dimensions (after applying rotation, which FFmpeg does
    /// automatically when filtering).
    pub fn dims(&self) -> Option<(u32, u32)> {
        let (w, h) = self.coded_dims()?;
        if self.rotation().rem_euclid(180) == 90 {
            Some((h, w))
        } else {
            Some((w, h))
        }
    }
    pub fn fps(&self) -> Option<f64> {
        let v = self.video()?;
        ratio(v["avg_frame_rate"].as_str()?)
            .or_else(|| ratio(v["r_frame_rate"].as_str()?))
            .filter(|f| *f > 0.0 && *f < 1000.0)
    }
    pub fn sample_rate(&self) -> Option<u32> {
        self.audio()
            .and_then(|a| a["sample_rate"].as_str())
            .and_then(|s| s.parse().ok())
    }
    pub fn channels(&self) -> Option<u32> {
        self.audio()
            .and_then(|a| a["channels"].as_u64())
            .map(|c| c as u32)
    }
    pub fn audio_bitrate(&self) -> Option<u64> {
        self.audio()
            .and_then(|a| num(&a["bit_rate"]))
            .map(|b| b as u64)
    }
    pub fn bitrate(&self) -> Option<u64> {
        num(&self.format()["bit_rate"]).map(|b| b as u64)
    }
}

pub fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

pub fn ratio(s: &str) -> Option<f64> {
    let (a, b) = s.split_once('/')?;
    let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
    if b == 0.0 {
        None
    } else {
        Some(a / b)
    }
}

/// Run ffprobe on a file. Unreadable files become invalid-input errors.
pub async fn probe(ctx: &ToolContext, path: &Path) -> ToolResult<Probe> {
    let out = ctx
        .command("ffprobe")?
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
            "-i",
        ])
        .arg(arg_path(path))
        .timeout(Duration::from_secs(120))
        .run_unchecked()
        .await?;
    if !out.success() {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let msg = out
            .stderr
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .unwrap_or("unknown error")
            .to_string();
        return Err(ToolError::input(format!(
            "'{name}' is not a readable media file: {msg}"
        )));
    }
    let raw: Value = serde_json::from_slice(&out.stdout).ctx("parsing ffprobe output")?;
    Ok(Probe { raw })
}

/// Probe and require a video stream.
pub async fn probe_video(ctx: &ToolContext, f: &InputFile) -> ToolResult<Probe> {
    let p = probe(ctx, &f.path).await?;
    if p.video().is_none() {
        return Err(ToolError::input(format!(
            "'{}' has no video stream",
            f.name
        )));
    }
    Ok(p)
}

/// Probe and require an audio stream.
pub async fn probe_audio(ctx: &ToolContext, f: &InputFile) -> ToolResult<Probe> {
    let p = probe(ctx, &f.path).await?;
    if p.audio().is_none() {
        return Err(ToolError::input(format!(
            "'{}' has no audio stream",
            f.name
        )));
    }
    Ok(p)
}

// ---------------------------------------------------------------------------
// Capabilities

#[derive(Debug, Default)]
pub struct Caps {
    pub encoders: HashSet<String>,
    pub filters: HashSet<String>,
    pub hwaccels: Vec<String>,
}

impl Caps {
    pub fn enc(&self, name: &str) -> bool {
        self.encoders.contains(name)
    }
    pub fn filter(&self, name: &str) -> bool {
        self.filters.contains(name)
    }
}

fn caps_cache() -> &'static Mutex<HashMap<PathBuf, Arc<Caps>>> {
    static C: OnceLock<Mutex<HashMap<PathBuf, Arc<Caps>>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// Parse `ffmpeg -encoders` / `-filters` style listings into names.
pub fn parse_listing(text: &str, flag_len: usize) -> HashSet<String> {
    let mut out = HashSet::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(flags), Some(name)) = (it.next(), it.next()) else {
            continue;
        };
        if flags.len() == flag_len
            && flags
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '.' || c == '|')
            && name != "="
        {
            out.insert(name.to_string());
        }
    }
    out
}

/// Fail with a clear `unsupported` error when this FFmpeg build lacks a filter.
pub async fn need_filter(ctx: &ToolContext, filter: &str, purpose: &str) -> ToolResult<()> {
    if caps(ctx).await?.filter(filter) {
        Ok(())
    } else {
        Err(ToolError::unsupported(format!(
            "this FFmpeg build has no '{filter}' filter, which is needed for {purpose}; install an FFmpeg build that includes it"
        )))
    }
}

/// Fail with a clear `unsupported` error when this FFmpeg build lacks an encoder.
pub async fn need_encoder(ctx: &ToolContext, encoder: &str) -> ToolResult<()> {
    if caps(ctx).await?.enc(encoder) {
        Ok(())
    } else {
        Err(ToolError::unsupported(format!(
            "this FFmpeg build has no '{encoder}' encoder"
        )))
    }
}

pub async fn caps(ctx: &ToolContext) -> ToolResult<Arc<Caps>> {
    let key = ctx.deps().path("ffmpeg").unwrap_or_default();
    if let Some(c) = caps_cache().lock().unwrap().get(&key) {
        return Ok(c.clone());
    }
    let run = |args: &'static [&'static str]| async move {
        ctx.command("ffmpeg")?
            .args(["-hide_banner"])
            .args(args)
            .timeout(Duration::from_secs(30))
            .run()
            .await
            .map(|o| o.stdout_text())
    };
    let enc = run(&["-encoders"]).await?;
    let fil = run(&["-filters"]).await?;
    let hw = run(&["-hwaccels"]).await.unwrap_or_default();
    let caps = Arc::new(Caps {
        encoders: parse_listing(&enc, 6),
        filters: parse_listing(&fil, 3),
        hwaccels: hw
            .lines()
            .skip_while(|l| !l.contains("acceleration methods"))
            .skip(1)
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect(),
    });
    caps_cache().lock().unwrap().insert(key, caps.clone());
    Ok(caps)
}

// ---------------------------------------------------------------------------
// Hardware encoders

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwApi {
    Nvenc,
    Qsv,
    Vaapi,
    VideoToolbox,
    Amf,
}

impl HwApi {
    pub const ALL: [HwApi; 5] = [
        HwApi::Nvenc,
        HwApi::Qsv,
        HwApi::Vaapi,
        HwApi::VideoToolbox,
        HwApi::Amf,
    ];
    pub fn id(&self) -> &'static str {
        match self {
            HwApi::Nvenc => "nvenc",
            HwApi::Qsv => "qsv",
            HwApi::Vaapi => "vaapi",
            HwApi::VideoToolbox => "videotoolbox",
            HwApi::Amf => "amf",
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            HwApi::Nvenc => "NVIDIA NVENC",
            HwApi::Qsv => "Intel Quick Sync",
            HwApi::Vaapi => "VA-API (Linux)",
            HwApi::VideoToolbox => "Apple VideoToolbox",
            HwApi::Amf => "AMD AMF",
        }
    }
    pub fn parse(s: &str) -> Option<HwApi> {
        HwApi::ALL.iter().copied().find(|h| h.id() == s)
    }
    /// Encoder name for a codec family (`h264`, `hevc`, `av1`, `vp9`).
    pub fn encoder(&self, family: &str) -> String {
        format!("{family}_{}", self.id())
    }
}

pub fn vaapi_device() -> Option<PathBuf> {
    (128..136)
        .map(|i| PathBuf::from(format!("/dev/dri/renderD{i}")))
        .find(|p| p.exists())
}

fn hw_cache() -> &'static Mutex<HashMap<String, bool>> {
    static C: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// Whether a (compiled-in) hardware encoder actually works on this machine,
/// determined by a tiny test encode. Results are cached.
pub async fn hw_encoder_works(ctx: &ToolContext, encoder: &str) -> bool {
    if let Some(v) = hw_cache().lock().unwrap().get(encoder) {
        return *v;
    }
    let Ok(c) = caps(ctx).await else { return false };
    let mut ok = c.enc(encoder);
    if ok {
        let mut cmd = match ctx.command("ffmpeg") {
            Ok(c) => c.args(["-hide_banner", "-nostdin", "-loglevel", "error"]),
            Err(_) => return false,
        };
        let vaapi = encoder.ends_with("_vaapi");
        if vaapi {
            match vaapi_device() {
                Some(d) => cmd = cmd.arg("-vaapi_device").arg(d),
                None => ok = false,
            }
        }
        if ok {
            cmd = cmd.args(["-f", "lavfi", "-i", "color=black:s=320x240:r=25:d=0.4"]);
            if vaapi {
                cmd = cmd.args(["-vf", "format=nv12,hwupload"]);
            }
            ok = cmd
                .args(["-c:v", encoder, "-frames:v", "5", "-f", "null", "-"])
                .timeout(Duration::from_secs(20))
                .run()
                .await
                .is_ok();
        }
    }
    hw_cache().lock().unwrap().insert(encoder.to_string(), ok);
    ok
}

// ---------------------------------------------------------------------------
// Encoder selection

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VCodec {
    H264,
    H265,
    Vp9,
    Av1,
    Vp8,
    Mpeg4,
    Prores,
}

impl VCodec {
    pub fn parse(s: &str) -> Option<VCodec> {
        Some(match s {
            "h264" => VCodec::H264,
            "h265" | "hevc" => VCodec::H265,
            "vp9" => VCodec::Vp9,
            "av1" => VCodec::Av1,
            "vp8" => VCodec::Vp8,
            "mpeg4" => VCodec::Mpeg4,
            "prores" => VCodec::Prores,
            _ => return None,
        })
    }
    /// From an ffprobe codec name.
    pub fn from_probe(s: &str) -> Option<VCodec> {
        match s {
            "hevc" => Some(VCodec::H265),
            "prores" => Some(VCodec::Prores),
            other => VCodec::parse(other),
        }
    }
    pub fn id(&self) -> &'static str {
        match self {
            VCodec::H264 => "h264",
            VCodec::H265 => "h265",
            VCodec::Vp9 => "vp9",
            VCodec::Av1 => "av1",
            VCodec::Vp8 => "vp8",
            VCodec::Mpeg4 => "mpeg4",
            VCodec::Prores => "prores",
        }
    }
    fn hw_family(&self) -> Option<&'static str> {
        match self {
            VCodec::H264 => Some("h264"),
            VCodec::H265 => Some("hevc"),
            VCodec::Av1 => Some("av1"),
            VCodec::Vp9 => Some("vp9"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ACodec {
    Aac,
    Mp3,
    Opus,
    Vorbis,
    Flac,
    Pcm,
    Ac3,
    Alac,
}

impl ACodec {
    pub fn parse(s: &str) -> Option<ACodec> {
        Some(match s {
            "aac" => ACodec::Aac,
            "mp3" => ACodec::Mp3,
            "opus" => ACodec::Opus,
            "vorbis" => ACodec::Vorbis,
            "flac" => ACodec::Flac,
            "pcm" | "wav" => ACodec::Pcm,
            "ac3" => ACodec::Ac3,
            "alac" => ACodec::Alac,
            _ => return None,
        })
    }
    pub fn encoder(&self) -> &'static str {
        match self {
            ACodec::Aac => "aac",
            ACodec::Mp3 => "libmp3lame",
            ACodec::Opus => "libopus",
            ACodec::Vorbis => "libvorbis",
            ACodec::Flac => "flac",
            ACodec::Pcm => "pcm_s16le",
            ACodec::Ac3 => "ac3",
            ACodec::Alac => "alac",
        }
    }
    pub fn lossless(&self) -> bool {
        matches!(self, ACodec::Flac | ACodec::Pcm | ACodec::Alac)
    }
    pub fn args(&self, kbps: u32) -> Vec<String> {
        let mut v = vec!["-c:a".to_string(), self.encoder().to_string()];
        if !self.lossless() {
            v.push("-b:a".into());
            v.push(format!("{kbps}k"));
        }
        v
    }
}

/// Containers supported for video output.
pub const VIDEO_CONTAINERS: &[&str] = &["mp4", "mkv", "webm", "mov", "avi"];

pub fn container_defaults(fmt: &str) -> (VCodec, ACodec) {
    match fmt {
        "webm" => (VCodec::Vp9, ACodec::Opus),
        "avi" => (VCodec::Mpeg4, ACodec::Mp3),
        _ => (VCodec::H264, ACodec::Aac),
    }
}

pub fn video_ok_in(fmt: &str, v: VCodec) -> bool {
    match fmt {
        "mp4" => matches!(
            v,
            VCodec::H264 | VCodec::H265 | VCodec::Av1 | VCodec::Mpeg4 | VCodec::Vp9
        ),
        "webm" => matches!(v, VCodec::Vp9 | VCodec::Av1 | VCodec::Vp8),
        "mov" => matches!(
            v,
            VCodec::H264 | VCodec::H265 | VCodec::Prores | VCodec::Mpeg4
        ),
        "avi" => matches!(v, VCodec::Mpeg4 | VCodec::H264),
        _ => true,
    }
}

pub fn audio_ok_in(fmt: &str, a: ACodec) -> bool {
    match fmt {
        "mp4" => matches!(
            a,
            ACodec::Aac | ACodec::Mp3 | ACodec::Opus | ACodec::Flac | ACodec::Ac3 | ACodec::Alac
        ),
        "webm" => matches!(a, ACodec::Opus | ACodec::Vorbis),
        "mov" => matches!(
            a,
            ACodec::Aac | ACodec::Mp3 | ACodec::Pcm | ACodec::Alac | ACodec::Ac3
        ),
        "avi" => matches!(a, ACodec::Mp3 | ACodec::Pcm | ACodec::Ac3),
        _ => true,
    }
}

/// Whether a probed audio codec can be stream-copied into a container.
pub fn audio_copy_ok(fmt: &str, probe_codec: &str) -> bool {
    let a = match probe_codec {
        c if c.starts_with("pcm_") => Some(ACodec::Pcm),
        other => ACodec::parse(other),
    };
    match a {
        Some(a) => audio_ok_in(fmt, a),
        None => fmt == "mkv",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    High,
    Medium,
    Low,
}

impl Quality {
    pub fn parse(s: &str) -> Quality {
        match s {
            "high" => Quality::High,
            "low" => Quality::Low,
            _ => Quality::Medium,
        }
    }
    fn pick<T>(&self, h: T, m: T, l: T) -> T {
        match self {
            Quality::High => h,
            Quality::Medium => m,
            Quality::Low => l,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    Fast,
    Medium,
    Slow,
}

impl Speed {
    pub fn parse(s: &str) -> Speed {
        match s {
            "fast" => Speed::Fast,
            "slow" => Speed::Slow,
            _ => Speed::Medium,
        }
    }
    fn pick<T>(&self, f: T, m: T, s: T) -> T {
        match self {
            Speed::Fast => f,
            Speed::Medium => m,
            Speed::Slow => s,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EncodeOpts {
    pub quality: Quality,
    /// Codec-native quality value (CRF / CQ / QP) overriding `quality`.
    pub crf: Option<i64>,
    pub speed: Speed,
    /// Hardware preference: "none", "auto" or an [`HwApi`] id.
    pub hw: String,
    /// Target average bitrate (kbit/s) instead of constant quality.
    pub bitrate_kbps: Option<u64>,
}

impl Default for EncodeOpts {
    fn default() -> Self {
        EncodeOpts {
            quality: Quality::Medium,
            crf: None,
            speed: Speed::Medium,
            hw: "none".into(),
            bitrate_kbps: None,
        }
    }
}

impl EncodeOpts {
    pub fn from_params(p: &Params) -> Self {
        EncodeOpts {
            quality: Quality::parse(p.str_or("quality", "medium")),
            crf: if p.str_or("quality", "medium") == "custom" {
                p.opt_int("crf")
            } else {
                None
            },
            speed: Speed::parse(p.str_or("speed", "medium")),
            hw: p.str_or("hardware", "none").to_string(),
            bitrate_kbps: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct VideoEnc {
    pub encoder: String,
    pub args: Vec<String>,
    /// Arguments placed before the first input (e.g. VAAPI device).
    pub pre_input: Vec<String>,
    /// Filter to append to the video filter chain (e.g. VAAPI upload).
    pub vf_suffix: Option<String>,
    pub hardware: Option<String>,
    /// Whether the encoder supports FFmpeg's generic `-pass` option.
    pub two_pass: bool,
}

impl VideoEnc {
    /// Build the final `-vf` value from filters (+ hardware suffix).
    pub fn vf(&self, mut chain: Vec<String>) -> Option<String> {
        if let Some(s) = &self.vf_suffix {
            chain.push(s.clone());
        }
        if chain.is_empty() {
            None
        } else {
            Some(chain.join(","))
        }
    }
}

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

/// Choose an encoder (hardware when requested and working, else software)
/// and build its quality arguments.
pub async fn video_encoder(
    ctx: &ToolContext,
    codec: VCodec,
    o: &EncodeOpts,
    warnings: &mut Vec<String>,
) -> ToolResult<VideoEnc> {
    let caps = caps(ctx).await?;
    if o.hw != "none" {
        if let Some(family) = codec.hw_family() {
            let apis: Vec<HwApi> = if o.hw == "auto" {
                HwApi::ALL.to_vec()
            } else {
                HwApi::parse(&o.hw).into_iter().collect()
            };
            for api in apis {
                let enc = api.encoder(family);
                if hw_encoder_works(ctx, &enc).await {
                    return Ok(hw_args(api, &enc, o));
                }
            }
            warnings.push(
                format!(
                    "no working {} hardware encoder for {} — used the software encoder instead",
                    if o.hw == "auto" { "" } else { &o.hw },
                    codec.id()
                )
                .replace("  ", " "),
            );
        } else {
            warnings.push(format!(
                "hardware encoding is not available for {}; used software",
                codec.id()
            ));
        }
    }
    let q = o.quality;
    let sp = o.speed;
    let br = o.bitrate_kbps.map(|b| format!("{b}k"));
    let mut a: Vec<String>;
    let enc: &str;
    let mut two_pass = true;
    match codec {
        VCodec::H264 | VCodec::H265 => {
            let h265 = codec == VCodec::H265;
            enc = if h265 { "libx265" } else { "libx264" };
            if !caps.enc(enc) {
                return Err(ToolError::unsupported(format!(
                    "this FFmpeg build has no {enc} encoder"
                )));
            }
            a = s(&[
                "-c:v",
                enc,
                "-preset",
                sp.pick("veryfast", "medium", "slow"),
                "-pix_fmt",
                "yuv420p",
            ]);
            match &br {
                Some(b) => a.extend(s(&["-b:v", b])),
                None => {
                    let crf = o.crf.unwrap_or(if h265 {
                        q.pick(20, 26, 30)
                    } else {
                        q.pick(18, 23, 28)
                    });
                    a.extend(["-crf".into(), crf.to_string()]);
                }
            }
            if h265 {
                a.extend(s(&["-x265-params", "log-level=error"]));
                two_pass = false;
            }
        }
        VCodec::Vp9 | VCodec::Vp8 => {
            let vp9 = codec == VCodec::Vp9;
            enc = if vp9 { "libvpx-vp9" } else { "libvpx" };
            if !caps.enc(enc) {
                return Err(ToolError::unsupported(format!(
                    "this FFmpeg build has no {enc} encoder"
                )));
            }
            a = s(&[
                "-c:v",
                enc,
                "-deadline",
                "good",
                "-cpu-used",
                sp.pick("4", "2", "1"),
                "-pix_fmt",
                "yuv420p",
            ]);
            if vp9 {
                a.extend(s(&["-row-mt", "1"]));
            }
            match &br {
                Some(b) => a.extend(s(&["-b:v", b])),
                None => {
                    let crf = o.crf.unwrap_or(q.pick(24, 32, 40));
                    a.extend([
                        "-crf".into(),
                        crf.to_string(),
                        "-b:v".into(),
                        if vp9 { "0".into() } else { "8M".into() },
                    ]);
                }
            }
        }
        VCodec::Av1 => {
            let crf = o.crf.unwrap_or(q.pick(24, 32, 40));
            if caps.enc("libsvtav1") {
                enc = "libsvtav1";
                a = s(&[
                    "-c:v",
                    enc,
                    "-preset",
                    sp.pick("10", "8", "5"),
                    "-pix_fmt",
                    "yuv420p",
                ]);
                match &br {
                    Some(b) => a.extend(s(&["-b:v", b])),
                    None => a.extend(["-crf".into(), crf.to_string()]),
                }
                two_pass = false;
            } else if caps.enc("libaom-av1") {
                enc = "libaom-av1";
                a = s(&[
                    "-c:v",
                    enc,
                    "-cpu-used",
                    sp.pick("8", "6", "4"),
                    "-row-mt",
                    "1",
                    "-pix_fmt",
                    "yuv420p",
                ]);
                match &br {
                    Some(b) => a.extend(s(&["-b:v", b])),
                    None => a.extend(["-crf".into(), crf.to_string(), "-b:v".into(), "0".into()]),
                }
            } else if caps.enc("librav1e") {
                enc = "librav1e";
                a = s(&["-c:v", enc, "-speed", sp.pick("10", "6", "4")]);
                match &br {
                    Some(b) => a.extend(s(&["-b:v", b])),
                    None => a.extend(["-qp".into(), (crf * 4).min(255).to_string()]),
                }
                two_pass = false;
            } else {
                return Err(ToolError::unsupported(
                    "this FFmpeg build has no AV1 encoder (libsvtav1, libaom-av1 or librav1e)",
                ));
            }
        }
        VCodec::Mpeg4 => {
            enc = "mpeg4";
            a = s(&["-c:v", "mpeg4", "-pix_fmt", "yuv420p"]);
            match &br {
                Some(b) => a.extend(s(&["-b:v", b])),
                None => a.extend([
                    "-q:v".into(),
                    o.crf.unwrap_or(q.pick(2, 5, 9)).clamp(1, 31).to_string(),
                ]),
            }
            if !caps.enc("mpeg4") {
                return Err(ToolError::unsupported(
                    "this FFmpeg build has no mpeg4 encoder",
                ));
            }
        }
        VCodec::Prores => {
            enc = "prores_ks";
            if !caps.enc(enc) {
                return Err(ToolError::unsupported(
                    "this FFmpeg build has no prores_ks encoder",
                ));
            }
            a = s(&[
                "-c:v",
                enc,
                "-profile:v",
                q.pick("3", "2", "0"),
                "-pix_fmt",
                "yuv422p10le",
                "-vendor",
                "apl0",
            ]);
            two_pass = false;
        }
    }
    Ok(VideoEnc {
        encoder: enc.to_string(),
        args: a,
        pre_input: vec![],
        vf_suffix: None,
        hardware: None,
        two_pass,
    })
}

fn hw_args(api: HwApi, enc: &str, o: &EncodeOpts) -> VideoEnc {
    let q = o.quality;
    let cq = o.crf.unwrap_or(q.pick(20, 25, 30));
    let mut a = s(&["-c:v", enc]);
    let mut pre = vec![];
    let mut suffix = None;
    match api {
        HwApi::Nvenc => {
            a.extend(s(&["-preset", o.speed.pick("p2", "p4", "p6")]));
            match o.bitrate_kbps {
                Some(b) => a.extend(["-b:v".into(), format!("{b}k")]),
                None => a.extend([
                    "-rc".into(),
                    "vbr".into(),
                    "-cq".into(),
                    cq.to_string(),
                    "-b:v".into(),
                    "0".into(),
                ]),
            }
        }
        HwApi::Qsv => match o.bitrate_kbps {
            Some(b) => a.extend(["-b:v".into(), format!("{b}k")]),
            None => a.extend(["-global_quality".into(), cq.to_string()]),
        },
        HwApi::Vaapi => {
            if let Some(d) = vaapi_device() {
                pre = vec!["-vaapi_device".to_string(), d.to_string_lossy().to_string()];
            }
            suffix = Some("format=nv12,hwupload".to_string());
            match o.bitrate_kbps {
                Some(b) => a.extend(["-b:v".into(), format!("{b}k")]),
                None => a.extend(["-qp".into(), cq.to_string()]),
            }
        }
        HwApi::VideoToolbox => match o.bitrate_kbps {
            Some(b) => a.extend(["-b:v".into(), format!("{b}k")]),
            None => a.extend(["-q:v".into(), (100 - cq * 2).clamp(1, 100).to_string()]),
        },
        HwApi::Amf => match o.bitrate_kbps {
            Some(b) => a.extend(["-b:v".into(), format!("{b}k")]),
            None => a.extend([
                "-rc".into(),
                "cqp".into(),
                "-qp_i".into(),
                cq.to_string(),
                "-qp_p".into(),
                cq.to_string(),
            ]),
        },
    }
    if o.bitrate_kbps.is_none() && !matches!(api, HwApi::Vaapi) {
        a.extend(s(&["-pix_fmt", "yuv420p"]));
    }
    VideoEnc {
        encoder: enc.to_string(),
        args: a,
        pre_input: pre,
        vf_suffix: suffix,
        hardware: Some(api.label().to_string()),
        two_pass: false,
    }
}

/// Container-specific muxer flags.
pub fn mux_args(fmt: &str, codec: Option<VCodec>) -> Vec<String> {
    let mut v = vec![];
    if fmt == "mp4" || fmt == "mov" {
        v.extend(s(&["-movflags", "+faststart"]));
        if codec == Some(VCodec::H265) {
            v.extend(s(&["-tag:v", "hvc1"]));
        }
    }
    v
}

/// Output container for re-encoding tools: keep the input's container when
/// supported, otherwise MP4.
pub fn same_container(f: &InputFile) -> &'static str {
    match f.extension().as_str() {
        "mkv" => "mkv",
        "webm" => "webm",
        "mov" => "mov",
        "avi" => "avi",
        _ => "mp4",
    }
}

/// Encoder arguments for tools that filter video and otherwise keep the
/// file "as is": same container, the source codec where possible, and audio
/// stream-copied when compatible (and `copy_audio`).
pub struct Reencode {
    pub ext: &'static str,
    pub venc: VideoEnc,
    pub vcodec: VCodec,
    pub audio: Vec<String>,
}

impl Reencode {
    pub fn tail(&self) -> Vec<String> {
        let mut v = self.venc.args.clone();
        v.extend(self.audio.clone());
        v.extend(mux_args(self.ext, Some(self.vcodec)));
        v
    }
}

pub async fn reencode(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    p: &Params,
    copy_audio: bool,
    warnings: &mut Vec<String>,
) -> ToolResult<Reencode> {
    let ext = same_container(f);
    let (dv, da) = container_defaults(ext);
    let caps = caps(ctx).await?;
    let vcodec = pr
        .video_codec()
        .and_then(VCodec::from_probe)
        .filter(|c| {
            video_ok_in(ext, *c) && !matches!(c, VCodec::Prores)
                || (*c == VCodec::Prores && ext == "mov")
        })
        .filter(|c| match c {
            VCodec::H265 => caps.enc("libx265"),
            VCodec::Av1 => caps.enc("libsvtav1") || caps.enc("libaom-av1"),
            VCodec::Vp9 => caps.enc("libvpx-vp9"),
            VCodec::Vp8 => caps.enc("libvpx"),
            _ => true,
        })
        .unwrap_or(dv);
    let mut o = EncodeOpts::from_params(p);
    o.hw = "none".into();
    let venc = video_encoder(ctx, vcodec, &o, warnings).await?;
    let audio = match pr.audio_codec() {
        None => vec![],
        Some(c) if copy_audio && audio_copy_ok(ext, c) => s(&["-c:a", "copy"]),
        Some(_) => da.args(if da == ACodec::Opus { 128 } else { 192 }),
    };
    Ok(Reencode {
        ext,
        venc,
        vcodec,
        audio,
    })
}

/// Audio encoder arguments for writing a given extension, preserving the
/// source's quality class.
pub fn audio_args_for_ext(ext: &str, pr: Option<&Probe>) -> Vec<String> {
    let src_br = pr.and_then(|p| p.audio_bitrate()).unwrap_or(192_000);
    let kbps = ((src_br / 1000).clamp(96, 320) as u32).max(128);
    let src_codec = pr.and_then(|p| p.audio_codec()).unwrap_or("");
    let fmt = pr
        .and_then(|p| p.audio())
        .and_then(|a| a["sample_fmt"].as_str())
        .unwrap_or("s16");
    let pcm = |le: bool| {
        let depth = if fmt.starts_with("s32")
            || fmt.starts_with("flt")
            || fmt.starts_with("dbl")
            || src_codec.contains("24")
        {
            24
        } else {
            16
        };
        format!("pcm_s{depth}{}", if le { "le" } else { "be" })
    };
    let v: Vec<String> = match ext {
        "mp3" => vec![
            "-c:a".into(),
            "libmp3lame".into(),
            "-b:a".into(),
            format!("{kbps}k"),
        ],
        "m4a" | "aac" | "mp4" | "mov" | "m4b" => ACodec::Aac.args(kbps),
        "ogg" | "oga" => {
            if src_codec == "opus" {
                ACodec::Opus.args(kbps.min(256))
            } else {
                vec!["-c:a".into(), "libvorbis".into(), "-q:a".into(), "6".into()]
            }
        }
        "opus" | "webm" => ACodec::Opus.args(kbps.min(256)),
        "flac" => vec!["-c:a".into(), "flac".into()],
        "wav" => vec!["-c:a".into(), pcm(true)],
        "aiff" | "aif" => vec!["-c:a".into(), pcm(false)],
        "wma" | "wmv" => vec![
            "-c:a".into(),
            "wmav2".into(),
            "-b:a".into(),
            format!("{kbps}k"),
        ],
        "ac3" => vec!["-c:a".into(), "ac3".into(), "-b:a".into(), "384k".into()],
        "avi" => ACodec::Mp3.args(kbps),
        "caf" => vec!["-c:a".into(), "alac".into()],
        "mka" | "mkv" => {
            if matches!(src_codec, "flac" | "alac") || src_codec.starts_with("pcm") {
                vec!["-c:a".into(), "flac".into()]
            } else {
                ACodec::Opus.args(kbps.min(256))
            }
        }
        _ => ACodec::Aac.args(kbps),
    };
    v
}

/// Output extension for audio processing: keep the input's when FFmpeg can
/// write it, otherwise fall back to a sensible one.
pub fn audio_out_ext(f: &InputFile, pr: &Probe) -> String {
    let e = f.extension();
    match e.as_str() {
        "mp3" | "m4a" | "aac" | "ogg" | "oga" | "opus" | "flac" | "wav" | "aiff" | "aif"
        | "wma" | "ac3" | "mka" | "caf" | "m4b" => e,
        "mp4" | "mov" | "mkv" | "webm" | "avi" if pr.video().is_some() => e,
        _ => {
            if pr.video().is_some() {
                "mp4".into()
            } else {
                "m4a".into()
            }
        }
    }
}

/// Apply an audio filter chain to a file, keeping any video / cover art
/// stream-copied when the container is unchanged.
pub async fn filter_audio_file(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    af: &str,
    out_ext: &str,
    out: &Path,
    dur: Option<f64>,
    sp: (f32, f32),
) -> ToolResult<()> {
    let same = out_ext == f.extension();
    let mut ff = Ff::new().input(&f.path);
    if same
        && pr.streams().iter().any(|s| s["codec_type"] == "video")
        && out_ext != "ogg"
        && out_ext != "opus"
    {
        ff = ff.args(["-map", "0:v?", "-c:v", "copy"]);
    } else {
        ff = ff.a("-vn");
    }
    ff = ff
        .args(["-map", "0:a:0", "-map_metadata", "0", "-af", af])
        .args(audio_args_for_ext(out_ext, Some(pr)));
    if out_ext == "mp4" || out_ext == "m4a" || out_ext == "mov" {
        ff = ff.args(["-movflags", "+faststart"]);
    }
    ff.out(out)
        .duration(dur)
        .span(sp, format!("Processing {}", f.name))
        .run(ctx)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Common parameters

pub fn quality_param() -> ParamSpec {
    ParamSpec::choice(
        "quality",
        &[
            ("high", "High (larger file)"),
            ("medium", "Balanced"),
            ("low", "Low (smaller file)"),
        ],
    )
    .default_str("medium")
    .describe("Encoding quality used when the video must be re-encoded")
    .advanced()
}

pub fn hardware_param() -> ParamSpec {
    ParamSpec::choice(
        "hardware",
        &[
            ("none", "Software only"),
            ("auto", "Auto (use GPU if available)"),
            ("nvenc", "NVIDIA NVENC"),
            ("qsv", "Intel Quick Sync"),
            ("vaapi", "VA-API"),
            ("videotoolbox", "Apple VideoToolbox"),
            ("amf", "AMD AMF"),
        ],
    )
    .default_str("none")
    .describe("Use a hardware encoder when present (falls back to software)")
    .advanced()
}

pub fn video_input(multiple: bool) -> ParamSpec {
    let p = if multiple {
        ParamSpec::files("input")
    } else {
        ParamSpec::file("input")
    };
    p.label(if multiple { "Videos" } else { "Video" })
        .accept(VIDEO_ACCEPT)
        .required()
        .primary()
}

pub fn audio_input(multiple: bool) -> ParamSpec {
    let p = if multiple {
        ParamSpec::files("input")
    } else {
        ParamSpec::file("input")
    };
    p.label(if multiple {
        "Audio files"
    } else {
        "Audio file"
    })
    .accept(MEDIA_ACCEPT)
    .required()
    .primary()
}

pub fn video_spec(id: &str, name: &str) -> ToolSpec {
    ToolSpec::new(id, name, Category::Video)
        .requires(FF)
        .output(OutputKind::Files)
        .progress()
        .timeout(4 * 3600)
}

pub fn audio_spec(id: &str, name: &str) -> ToolSpec {
    ToolSpec::new(id, name, Category::Audio)
        .requires(FF)
        .output(OutputKind::Files)
        .progress()
        .timeout(3 * 3600)
}

// ---------------------------------------------------------------------------
// Results

/// Collects per-file results for batch tools.
pub struct Report {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub artifacts: Vec<Artifact>,
    pub warnings: Vec<String>,
    pub items: Vec<Value>,
}

impl Report {
    pub fn new(cols: &[&str]) -> Self {
        Report {
            columns: cols.iter().map(|c| c.to_string()).collect(),
            rows: vec![],
            artifacts: vec![],
            warnings: vec![],
            items: vec![],
        }
    }
    /// Standard row: input, output, sizes.
    pub fn file(
        &mut self,
        ctx: &ToolContext,
        input: &InputFile,
        out: &Path,
        extra: Vec<Value>,
        item: Value,
    ) -> ToolResult<()> {
        let a = ctx.artifact(out)?;
        let before = input.size();
        let mut row = vec![
            json!(input.name),
            json!(a.name),
            json!(human_size(before)),
            json!(human_size(a.size)),
        ];
        row.extend(extra);
        self.rows.push(row);
        let mut it = json!({"input": input.name, "output": a.name, "size_before": before, "size_after": a.size});
        if let (Value::Object(m), Value::Object(extra)) = (&mut it, item) {
            m.extend(extra);
        }
        self.items.push(it);
        self.artifacts.push(a);
        Ok(())
    }
    pub fn finish(mut self) -> ToolOutput {
        self.warnings.dedup();
        let mut out = ToolOutput::files(self.artifacts).with_data(json!({
            "table": {"columns": self.columns, "rows": self.rows},
            "files": self.items,
        }));
        out.warnings = self.warnings;
        out
    }
}

pub const FILE_COLS: [&str; 4] = ["Input", "Output", "Size before", "Size after"];

impl Report {
    /// Report with the standard file columns plus extras.
    pub fn files(extra: &[&str]) -> Self {
        let mut c: Vec<&str> = FILE_COLS.to_vec();
        c.extend_from_slice(extra);
        Report::new(&c)
    }
}

/// Zip a list of files into an output archive.
pub fn zip_files(ctx: &ToolContext, files: &[PathBuf], name: &str) -> ToolResult<Artifact> {
    use std::io::Write;
    let out = ctx.output_path(name);
    let f = std::fs::File::create(&out)?;
    let mut z = zip::ZipWriter::new(f);
    let opts =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for p in files {
        ctx.check_cancelled()?;
        let n = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        z.start_file(n, opts).ctx("writing zip")?;
        let data = std::fs::read(p)?;
        z.write_all(&data)?;
    }
    z.finish().ctx("writing zip")?;
    ctx.artifact(out)
}

/// Move files from a scratch directory into the output directory.
pub fn publish(ctx: &ToolContext, files: &[PathBuf]) -> ToolResult<Vec<Artifact>> {
    let mut v = vec![];
    for p in files {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "file".into());
        let dest = ctx.output_path(&name);
        if std::fs::rename(p, &dest).is_err() {
            std::fs::copy(p, &dest)?;
        }
        v.push(ctx.artifact(&dest)?);
    }
    Ok(v)
}

/// Escape a value for use inside a single-quoted FFmpeg filter option.
pub fn filter_quote(s: &str) -> String {
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "'\\''"))
}

/// `#rrggbb` → `0xRRGGBB` for FFmpeg colors (with optional alpha).
pub fn ff_color(hex: &str) -> String {
    let h = hex.trim_start_matches('#');
    format!("0x{}", h.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_parse() {
        let enc = "Encoders:\n V..... = Video\n A..... = Audio\n ------\n V....D libx264              libx264 H.264\n A....D aac                  AAC\n";
        let s = parse_listing(enc, 6);
        assert!(s.contains("libx264") && s.contains("aac") && !s.contains("="));
        let fil = "Filters:\n  T.. = Timeline support\n TSC afftdn            A->A       Denoise\n ... anull  A->A x\n";
        let f = parse_listing(fil, 3);
        assert!(f.contains("afftdn") && f.contains("anull"));
    }

    #[test]
    fn times() {
        assert_eq!(hms(3725.5), "01:02:05.500");
        assert_eq!(secs(1.23456), "1.235");
        assert_eq!(
            ratio("30000/1001").map(|v| (v * 100.0).round()),
            Some(2997.0)
        );
        assert_eq!(ratio("0/0"), None);
        assert_eq!(arg_path(Path::new("/a/b")), OsString::from("/a/b"));
        assert!(Path::new(&arg_path(Path::new("-x.mp4"))).is_absolute());
    }

    #[test]
    fn combos() {
        assert!(video_ok_in("webm", VCodec::Vp9));
        assert!(!video_ok_in("webm", VCodec::H264));
        assert!(audio_copy_ok("mp4", "aac"));
        assert!(!audio_copy_ok("webm", "aac"));
        assert!(audio_copy_ok("mkv", "dts"));
    }
}
