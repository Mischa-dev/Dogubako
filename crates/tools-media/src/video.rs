//! Video tools built on FFmpeg.

use crate::ffmpeg::*;
use dogubako_core::prelude::*;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        convert(),
        compress(),
        resize(),
        fps(),
        trim(),
        cut(),
        crop(),
        rotate(),
        merge(),
        extract_audio_tool("video.extract_audio", Category::Video),
        remove_audio(),
        replace_audio(),
        extract_frames(),
        thumbnail(),
        to_gif(),
        from_gif(),
        speed(),
        stabilize(),
        info(),
        preview(),
        hw_encoders(),
        watermark(),
        contact_sheet(),
    ]
}

// ---------------------------------------------------------------------------
// Shared: simple "filter and re-encode" batch runner

#[derive(Default)]
struct VfJob {
    vf: Vec<String>,
    af: Option<String>,
    copy_audio: bool,
    drop_audio: bool,
    out_dur: Option<f64>,
    pre_input: Vec<String>,
    post_input: Vec<String>,
    extra_out: Vec<String>,
    cwd: Option<PathBuf>,
    row: Vec<Value>,
    item: Value,
    warnings: Vec<String>,
}

type JobFut = Pin<Box<dyn Future<Output = ToolResult<VfJob>> + Send>>;

async fn vf_batch<F>(
    ctx: ToolContext,
    p: Params,
    suffix: &'static str,
    label: &'static str,
    extra_cols: &[&str],
    build: F,
) -> ToolResult<ToolOutput>
where
    F: Fn(ToolContext, Params, InputFile, Probe, (f32, f32)) -> JobFut,
{
    let files = p.files("input").to_vec();
    let mut rep = Report::files(extra_cols);
    for (i, f) in files.iter().enumerate() {
        let sp = span(i, files.len());
        let pr = probe_video(&ctx, f).await?;
        let job = build(ctx.clone(), p.clone(), f.clone(), pr.clone(), sp).await?;
        rep.warnings.extend(job.warnings.iter().cloned());
        let re = reencode(
            &ctx,
            f,
            &pr,
            &p,
            job.copy_audio && job.af.is_none(),
            &mut rep.warnings,
        )
        .await?;
        let out = out_path(&ctx, f, suffix, re.ext);
        let mut ff = Ff::new()
            .args(&job.pre_input)
            .input(&f.path)
            .args(&job.post_input)
            .args(["-map", "0:V:0"]);
        if !job.drop_audio && pr.audio().is_some() {
            ff = ff.args(["-map", "0:a?"]);
        }
        if let Some(vf) = re.venc.vf(job.vf.clone()) {
            ff = ff.args(["-vf", &vf]);
        }
        if job.drop_audio {
            ff = ff.a("-an");
        } else if let Some(af) = &job.af {
            ff = ff.args(["-af", af]);
        }
        ff = ff.args(re.tail()).args(&job.extra_out).a("-sn").out(&out);
        if let Some(c) = &job.cwd {
            ff = ff.cwd(c);
        }
        let dur = job.out_dur.or(pr.duration());
        ff.duration(dur)
            .span(sp, format!("{label} {}", f.name))
            .run(&ctx)
            .await?;
        rep.file(&ctx, f, &out, job.row, job.item)?;
    }
    Ok(rep.finish())
}

fn even(v: f64) -> u32 {
    ((v / 2.0).round() as u32 * 2).max(2)
}

fn require_dur(pr: &Probe, f: &InputFile) -> ToolResult<f64> {
    pr.duration().ok_or_else(|| {
        ToolError::input(format!("could not determine the duration of '{}'", f.name))
    })
}

/// A time parameter that may also be a percentage of the duration ("25%").
fn time_or_percent(p: &Params, name: &str, dur: Option<f64>) -> ToolResult<Option<f64>> {
    if let Some(s) = p.opt_str(name).map(str::trim) {
        if let Some(pct) = s.strip_suffix('%') {
            let v: f64 = pct
                .trim()
                .parse()
                .map_err(|_| ToolError::invalid(name, format!("'{s}' is not a percentage")))?;
            if !(0.0..=100.0).contains(&v) {
                return Err(ToolError::invalid(
                    name,
                    "percentage must be between 0 and 100",
                ));
            }
            return Ok(dur.map(|d| d * v / 100.0));
        }
    }
    opt_time(p, name)
}

fn atempo_chain(mut f: f64) -> String {
    let mut parts = vec![];
    while f > 2.0 {
        parts.push("atempo=2.0".to_string());
        f /= 2.0;
    }
    while f < 0.5 {
        parts.push("atempo=0.5".to_string());
        f /= 0.5;
    }
    parts.push(format!("atempo={f:.6}"));
    parts.join(",")
}

pub(crate) fn speed_chain(f: f64) -> String {
    atempo_chain(f)
}

// ---------------------------------------------------------------------------
// video.convert

fn convert() -> Arc<dyn Tool> {
    let spec = video_spec("video.convert", "Convert Video")
        .describe("Convert videos between containers and codecs (MP4/H.264, H.265, WebM/VP9, AV1, MKV, MOV, AVI, GIF) with quality control and optional GPU encoding")
        .keywords(&["transcode", "mp4", "webm", "mkv", "mov", "avi", "h264", "h265", "hevc", "vp9", "av1", "prores", "format"])
        .param(video_input(true))
        .param(ParamSpec::choice("format", &[("mp4", "MP4"), ("webm", "WebM"), ("mkv", "MKV (Matroska)"), ("mov", "MOV (QuickTime)"), ("avi", "AVI"), ("gif", "Animated GIF")]).default_str("mp4"))
        .param(
            ParamSpec::choice(
                "video_codec",
                &[("auto", "Auto (best for container)"), ("h264", "H.264 / AVC"), ("h265", "H.265 / HEVC"), ("vp9", "VP9"), ("av1", "AV1"), ("vp8", "VP8"), ("mpeg4", "MPEG-4 Part 2"), ("prores", "Apple ProRes"), ("copy", "Copy (no re-encode)")],
            )
            .default_str("auto")
            .show_if("format", &["mp4", "webm", "mkv", "mov", "avi"]),
        )
        .param(
            ParamSpec::choice(
                "audio_codec",
                &[("auto", "Auto"), ("aac", "AAC"), ("mp3", "MP3"), ("opus", "Opus"), ("vorbis", "Vorbis"), ("flac", "FLAC"), ("pcm", "PCM (uncompressed)"), ("ac3", "AC-3"), ("copy", "Copy"), ("none", "Remove audio")],
            )
            .default_str("auto")
            .show_if("format", &["mp4", "webm", "mkv", "mov", "avi"]),
        )
        .param(ParamSpec::choice("quality", &[("high", "High"), ("medium", "Balanced"), ("low", "Small file"), ("custom", "Custom CRF")]).default_str("medium"))
        .param(ParamSpec::integer("crf").range(0.0, 63.0).default_int(23).describe("Codec-native constant quality value (lower = better)").show_if("quality", &["custom"]).advanced())
        .param(ParamSpec::choice("speed", &[("fast", "Fast"), ("medium", "Medium"), ("slow", "Slow (smaller)")]).default_str("medium").advanced())
        .param(ParamSpec::integer("audio_bitrate").label("Audio bitrate (kbit/s)").range(32.0, 512.0).default_int(160).advanced())
        .param(hardware_param());
    async_tool(spec, |ctx, p| async move {
        let fmt = p.str("format")?.to_string();
        let files = p.files("input").to_vec();
        let mut rep = Report::files(&["Video", "Audio"]);
        for (i, f) in files.iter().enumerate() {
            let sp = span(i, files.len());
            let pr = probe_video(&ctx, f).await?;
            if fmt == "gif" {
                let out = out_path(&ctx, f, "converted", "gif");
                let w = pr.dims().map(|d| d.0.min(480)).unwrap_or(480);
                let g = GifOpts {
                    fps: 12,
                    width: w,
                    start: None,
                    duration: None,
                    loop_: 0,
                    dither: "sierra2_4a".into(),
                    colors: 256,
                    stats: "diff".into(),
                    format: "gif".into(),
                };
                make_gif(&ctx, f, &pr, &g, &out, sp).await?;
                rep.file(
                    &ctx,
                    f,
                    &out,
                    vec![json!("gif"), json!("-")],
                    json!({"format": "gif"}),
                )?;
                continue;
            }
            let (dv, da) = container_defaults(&fmt);
            let mut ff = Ff::new();
            let vsel = p.str("video_codec")?;
            let (vargs, vname, vcodec, venc) = if vsel == "copy" {
                let src = pr.video_codec().unwrap_or("?");
                if let Some(c) = VCodec::from_probe(src) {
                    if !video_ok_in(&fmt, c) {
                        return Err(ToolError::invalid("video_codec", format!("{src} video cannot be copied into {fmt}; choose a codec to re-encode")));
                    }
                } else if fmt != "mkv" {
                    return Err(ToolError::invalid(
                        "video_codec",
                        format!("{src} video cannot be copied into {fmt}; choose a codec or MKV"),
                    ));
                }
                (
                    vec!["-c:v".to_string(), "copy".to_string()],
                    format!("copy ({src})"),
                    VCodec::from_probe(src),
                    None,
                )
            } else {
                let codec = if vsel == "auto" {
                    dv
                } else {
                    VCodec::parse(vsel)
                        .ok_or_else(|| ToolError::invalid("video_codec", "unknown codec"))?
                };
                if !video_ok_in(&fmt, codec) {
                    return Err(ToolError::invalid(
                        "video_codec",
                        format!("{} is not supported in {fmt}", codec.id()),
                    ));
                }
                let o = EncodeOpts::from_params(&p);
                let enc = video_encoder(&ctx, codec, &o, &mut rep.warnings).await?;
                let name = match &enc.hardware {
                    Some(h) => format!("{} ({h})", enc.encoder),
                    None => enc.encoder.clone(),
                };
                (enc.args.clone(), name, Some(codec), Some(enc))
            };
            if let Some(e) = &venc {
                ff = ff.args(&e.pre_input);
            }
            ff = ff.input(&f.path).args(["-map", "0:V:0"]);
            let asel = p.str("audio_codec")?;
            let has_audio = pr.audio().is_some();
            let (aargs, aname): (Vec<String>, String) = if !has_audio || asel == "none" {
                (vec!["-an".into()], "none".into())
            } else if asel == "copy" {
                let src = pr.audio_codec().unwrap_or("?");
                if !audio_copy_ok(&fmt, src) {
                    return Err(ToolError::invalid(
                        "audio_codec",
                        format!("{src} audio cannot be copied into {fmt}; choose an audio codec"),
                    ));
                }
                (vec!["-c:a".into(), "copy".into()], format!("copy ({src})"))
            } else {
                let ac = if asel == "auto" {
                    da
                } else {
                    ACodec::parse(asel)
                        .ok_or_else(|| ToolError::invalid("audio_codec", "unknown codec"))?
                };
                if !audio_ok_in(&fmt, ac) {
                    return Err(ToolError::invalid(
                        "audio_codec",
                        format!("{asel} audio is not supported in {fmt}"),
                    ));
                }
                (
                    ac.args(p.int_or("audio_bitrate", 160) as u32),
                    ac.encoder().to_string(),
                )
            };
            if has_audio && asel != "none" {
                ff = ff.args(["-map", "0:a?"]);
            }
            if let Some(e) = &venc {
                if let Some(vf) = e.vf(vec![]) {
                    ff = ff.args(["-vf", &vf]);
                }
            }
            let out = out_path(&ctx, f, "converted", &fmt);
            ff = ff
                .args(vargs)
                .args(aargs)
                .args(["-map_metadata", "0", "-sn"])
                .args(mux_args(&fmt, vcodec))
                .out(&out);
            ff.duration(pr.duration())
                .span(sp, format!("Converting {}", f.name))
                .run(&ctx)
                .await?;
            rep.file(&ctx, f, &out, vec![json!(vname), json!(aname)], json!({"video_encoder": vname, "audio_encoder": aname, "hardware": venc.and_then(|e| e.hardware)}))?;
        }
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// video.compress

fn compress() -> Arc<dyn Tool> {
    let spec = video_spec("video.compress", "Compress Video")
        .describe("Shrink videos by quality level or to a target file size (two-pass bitrate), optionally capping the resolution")
        .keywords(&["reduce size", "smaller", "shrink", "target size", "bitrate", "2-pass", "email", "discord"])
        .param(video_input(true))
        .param(ParamSpec::choice("mode", &[("quality", "By quality level"), ("size", "To a target file size")]).default_str("quality"))
        .param(ParamSpec::choice("level", &[("light", "Light (best quality)"), ("balanced", "Balanced"), ("strong", "Strong (smallest)")]).default_str("balanced").show_if("mode", &["quality"]))
        .param(ParamSpec::number("target_mb").label("Target size (MB)").range(0.1, 100000.0).default_num(25.0).show_if("mode", &["size"]))
        .param(ParamSpec::choice("max_resolution", &[("keep", "Keep"), ("2160", "4K (2160p)"), ("1440", "1440p"), ("1080", "1080p"), ("720", "720p"), ("480", "480p"), ("360", "360p")]).default_str("keep").describe("Cap the shorter side of the video"))
        .param(ParamSpec::choice("codec", &[("h264", "H.264 (MP4, most compatible)"), ("h265", "H.265 (MP4, smaller)"), ("av1", "AV1 (MP4, smallest, slow)"), ("vp9", "VP9 (WebM)")]).default_str("h264"))
        .param(ParamSpec::integer("audio_kbps").label("Audio bitrate (kbit/s)").range(16.0, 320.0).default_int(128).advanced())
        .param(ParamSpec::choice("speed", &[("fast", "Fast"), ("medium", "Medium"), ("slow", "Slow (smaller)")]).default_str("medium").advanced())
        .param(hardware_param());
    async_tool(spec, |ctx, p| async move {
        let codec = VCodec::parse(p.str("codec")?).unwrap_or(VCodec::H264);
        let fmt = if codec == VCodec::Vp9 { "webm" } else { "mp4" };
        let aud = if codec == VCodec::Vp9 {
            ACodec::Opus
        } else {
            ACodec::Aac
        };
        let akbps = p.int_or("audio_kbps", 128) as u64;
        let files = p.files("input").to_vec();
        let mut rep = Report::files(&["Reduction", "Method"]);
        for (i, f) in files.iter().enumerate() {
            let sp = span(i, files.len());
            let pr = probe_video(&ctx, f).await?;
            let mut o = EncodeOpts::from_params(&p);
            o.crf = None;
            let level = p.str_or("level", "balanced");
            let pick = |a: i64, b: i64, c: i64| match level {
                "light" => a,
                "strong" => c,
                _ => b,
            };
            let mut method;
            let size_mode = p.str("mode")? == "size";
            // In size mode the audio bitrate is part of the budget.
            let mut budget_audio_kbps = None;
            if size_mode {
                let dur = require_dur(&pr, f)?;
                let target = p.num("target_mb")?;
                let total_kbps = target * 8.0 * 1024.0 * 1024.0 / 1000.0 / dur;
                let a = if pr.audio().is_some() {
                    akbps.min((total_kbps * 0.25) as u64).max(16)
                } else {
                    0
                };
                let v = total_kbps * 0.97 - a as f64;
                if v < 40.0 {
                    return Err(ToolError::invalid("target_mb", format!("{target} MB is too small for {:.0} s of video (needs at least ~{:.1} MB)", dur, (40.0 + a as f64) * dur * 1000.0 / 8.0 / 1024.0 / 1024.0 / 0.97)));
                }
                o.bitrate_kbps = Some(v as u64);
                budget_audio_kbps = Some(a);
                method = format!("{} kbit/s video", v as u64);
            } else {
                o.crf = Some(match codec {
                    VCodec::H265 => pick(24, 28, 32),
                    VCodec::Av1 => pick(30, 36, 42),
                    VCodec::Vp9 => pick(31, 36, 42),
                    _ => pick(21, 25, 30),
                });
                method = format!("{level} quality");
            }
            let enc = video_encoder(&ctx, codec, &o, &mut rep.warnings).await?;
            let mut vf = vec![];
            if let (Ok(cap), Some((w, h))) = (p.str("max_resolution")?.parse::<u32>(), pr.dims()) {
                if w.min(h) > cap {
                    vf.push(if w >= h {
                        format!("scale=-2:{cap}:flags=lanczos")
                    } else {
                        format!("scale={cap}:-2:flags=lanczos")
                    });
                    method.push_str(&format!(", {cap}p"));
                }
            }
            let vf = enc.vf(vf);
            let audio_kbps = budget_audio_kbps.unwrap_or(akbps);
            let aargs = if pr.audio().is_some() {
                aud.args(audio_kbps as u32)
            } else {
                vec!["-an".into()]
            };
            let out = out_path(&ctx, f, "compressed", fmt);
            let base = |ff: Ff| {
                let mut ff = ff
                    .args(&enc.pre_input)
                    .input(&f.path)
                    .args(["-map", "0:V:0"]);
                if let Some(vf) = &vf {
                    ff = ff.args(["-vf", vf]);
                }
                ff.args(&enc.args)
            };
            if size_mode && enc.two_pass && enc.hardware.is_none() {
                let log = ctx.temp_path(&format!("pass-{i}"));
                let logs = log.to_string_lossy().to_string();
                base(Ff::new())
                    .args([
                        "-pass",
                        "1",
                        "-passlogfile",
                        &logs,
                        "-an",
                        "-f",
                        "null",
                        "-",
                    ])
                    .duration(pr.duration())
                    .span(
                        sub(sp, 0.0, 0.5),
                        format!("Analyzing {} (pass 1/2)", f.name),
                    )
                    .run(&ctx)
                    .await?;
                let mut ff = base(Ff::new()).args(["-pass", "2", "-passlogfile", &logs]);
                if pr.audio().is_some() {
                    ff = ff.args(["-map", "0:a:0"]);
                }
                ff.args(&aargs)
                    .args(["-map_metadata", "0"])
                    .args(mux_args(fmt, Some(codec)))
                    .out(&out)
                    .duration(pr.duration())
                    .span(sub(sp, 0.5, 1.0), format!("Encoding {} (pass 2/2)", f.name))
                    .run(&ctx)
                    .await?;
                method.push_str(", 2-pass");
            } else {
                let mut ff = base(Ff::new());
                if size_mode {
                    if let Some(b) = o.bitrate_kbps {
                        ff = ff.args([
                            "-maxrate",
                            &format!("{}k", b * 3 / 2),
                            "-bufsize",
                            &format!("{}k", b * 2),
                        ]);
                    }
                }
                if pr.audio().is_some() {
                    ff = ff.args(["-map", "0:a:0"]);
                }
                ff.args(&aargs)
                    .args(["-map_metadata", "0"])
                    .args(mux_args(fmt, Some(codec)))
                    .out(&out)
                    .duration(pr.duration())
                    .span(sp, format!("Compressing {}", f.name))
                    .run(&ctx)
                    .await?;
            }
            let after = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
            let before = f.size().max(1);
            let red = 100.0 - after as f64 * 100.0 / before as f64;
            if after >= before {
                rep.warnings.push(format!(
                    "{}: the compressed file is not smaller than the original",
                    f.name
                ));
            }
            if let Some(h) = &enc.hardware {
                method.push_str(&format!(", {h}"));
            }
            rep.file(&ctx, f, &out, vec![json!(format!("{red:.1}%")), json!(method)], json!({"reduction_percent": (red * 10.0).round() / 10.0, "method": method, "encoder": enc.encoder}))?;
        }
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// video.resize

fn resize() -> Arc<dyn Tool> {
    let spec = video_spec("video.resize", "Resize Video")
        .describe("Scale videos to a preset (4K…240p), custom size or percentage, keeping aspect ratio with fit, fill (crop) or pad modes")
        .keywords(&["scale", "resolution", "1080p", "720p", "downscale", "upscale", "dimensions", "letterbox"])
        .param(video_input(true))
        .param(
            ParamSpec::choice("size", &[("2160", "4K (2160p)"), ("1440", "1440p"), ("1080", "1080p"), ("720", "720p"), ("480", "480p"), ("360", "360p"), ("240", "240p"), ("custom", "Custom width/height"), ("percent", "Percentage")])
                .default_str("720")
                .describe("Presets set the shorter side of the video"),
        )
        .param(ParamSpec::integer("width").range(16.0, 8192.0).show_if("size", &["custom"]).describe("Leave empty to derive from height"))
        .param(ParamSpec::integer("height").range(16.0, 8192.0).show_if("size", &["custom"]).describe("Leave empty to derive from width"))
        .param(ParamSpec::number("percent").range(1.0, 400.0).default_num(50.0).show_if("size", &["percent"]))
        .param(ParamSpec::choice("fit", &[("fit", "Fit (keep aspect)"), ("fill", "Fill (crop to exact size)"), ("pad", "Pad (letterbox to exact size)"), ("stretch", "Stretch")]).default_str("fit").show_if("size", &["custom"]))
        .param(ParamSpec::color("pad_color").default_str("#000000").show_if("fit", &["pad"]).advanced())
        .param(ParamSpec::boolean("upscale").describe("Allow making videos larger than the source").advanced())
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        vf_batch(ctx, p, "resized", "Resizing", &["Resolution"], |_ctx, p, f, pr, _sp| {
            Box::pin(async move {
                let (sw, sh) = pr.dims().ok_or_else(|| ToolError::input(format!("unknown dimensions for '{}'", f.name)))?;
                let (sw_f, sh_f) = (sw as f64, sh as f64);
                let size = p.str("size")?;
                let mut fit = "fit".to_string();
                let (mut w, mut h) = match size {
                    "custom" => {
                        fit = p.str_or("fit", "fit").to_string();
                        match (p.opt_int("width"), p.opt_int("height")) {
                            (Some(w), Some(h)) => (w as f64, h as f64),
                            (Some(w), None) => (w as f64, w as f64 * sh_f / sw_f),
                            (None, Some(h)) => (h as f64 * sw_f / sh_f, h as f64),
                            (None, None) => return Err(ToolError::invalid("width", "give a width, a height or both")),
                        }
                    }
                    "percent" => {
                        let k = p.num("percent")? / 100.0;
                        (sw_f * k, sh_f * k)
                    }
                    preset => {
                        let short: f64 = preset.parse().map_err(|_| ToolError::invalid("size", "unknown preset"))?;
                        let k = short / sw_f.min(sh_f);
                        (sw_f * k, sh_f * k)
                    }
                };
                let mut warnings = vec![];
                if fit == "fit" && size == "custom" && p.opt_int("width").is_some() && p.opt_int("height").is_some() {
                    let k = (w / sw_f).min(h / sh_f);
                    w = sw_f * k;
                    h = sh_f * k;
                }
                if !p.bool("upscale") && (w > sw_f || h > sh_f) && (fit == "fit" || fit == "stretch") {
                    let k = (sw_f / w).min(sh_f / h);
                    w *= k;
                    h *= k;
                    warnings.push(format!("{}: target is larger than the source; kept {}x{} (enable upscale to enlarge)", f.name, even(w), even(h)));
                }
                let (w, h) = (even(w), even(h));
                let vf = match fit.as_str() {
                    "fill" => vec![format!("scale={w}:{h}:force_original_aspect_ratio=increase:flags=lanczos"), format!("crop={w}:{h}"), "setsar=1".into()],
                    "pad" => vec![
                        format!("scale={w}:{h}:force_original_aspect_ratio=decrease:flags=lanczos"),
                        format!("pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color={}", ff_color(p.str_or("pad_color", "#000000"))),
                        "setsar=1".into(),
                    ],
                    _ => vec![format!("scale={w}:{h}:flags=lanczos"), "setsar=1".into()],
                };
                Ok(VfJob { vf, copy_audio: true, row: vec![json!(format!("{sw}x{sh} → {w}x{h}"))], item: json!({"width": w, "height": h}), warnings, ..Default::default() })
            })
        })
        .await
    })
}

// ---------------------------------------------------------------------------
// video.fps

fn fps() -> Arc<dyn Tool> {
    let spec = video_spec("video.fps", "Change Frame Rate")
        .describe("Change a video's frame rate by dropping/duplicating, blending or motion-interpolating frames")
        .keywords(&["framerate", "fps", "60fps", "30fps", "24fps", "interpolate", "slow motion", "smooth"])
        .param(video_input(true))
        .param(ParamSpec::number("fps").label("Frames per second").range(1.0, 240.0).default_num(30.0).required())
        .param(ParamSpec::choice("method", &[("drop", "Drop / duplicate frames (fast)"), ("blend", "Blend frames"), ("interpolate", "Motion interpolation (slow)")]).default_str("drop"))
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        vf_batch(ctx, p, "fps", "Converting", &["Frame rate"], |_ctx, p, _f, pr, _sp| {
            Box::pin(async move {
                let fps = p.num("fps")?;
                let vf = match p.str("method")? {
                    "blend" => format!("framerate=fps={fps}"),
                    "interpolate" => format!("minterpolate=fps={fps}:mi_mode=mci:mc_mode=aobmc:me_mode=bidir:vsbmc=1"),
                    _ => format!("fps={fps}"),
                };
                let src = pr.fps().map(|f| format!("{f:.3}")).unwrap_or_else(|| "?".into());
                Ok(VfJob { vf: vec![vf], copy_audio: true, row: vec![json!(format!("{src} → {fps}"))], item: json!({"fps": fps}), ..Default::default() })
            })
        })
        .await
    })
}

// ---------------------------------------------------------------------------
// video.trim & video.cut

fn trim_range(p: &Params, dur: Option<f64>) -> ToolResult<(f64, Option<f64>)> {
    let start = opt_time(p, "start")?.unwrap_or(0.0);
    let end = opt_time(p, "end")?;
    let len = opt_time(p, "duration")?;
    if let Some(d) = dur {
        if start >= d {
            return Err(ToolError::invalid(
                "start",
                format!(
                    "start {} is beyond the end of the media ({})",
                    hms(start),
                    hms(d)
                ),
            ));
        }
    }
    let len = match (end, len) {
        (Some(e), _) if e <= start => {
            return Err(ToolError::invalid("end", "end must be after start"))
        }
        (Some(e), _) => Some(e - start),
        (None, Some(l)) if l <= 0.0 => {
            return Err(ToolError::invalid("duration", "duration must be positive"))
        }
        (None, Some(l)) => Some(l),
        (None, None) => None,
    };
    if start == 0.0 && len.is_none() {
        return Err(ToolError::input("give a start, end or duration"));
    }
    let len = match (len, dur) {
        (Some(l), Some(d)) => Some(l.min(d - start)),
        (l, _) => l,
    };
    Ok((start, len))
}

/// Write one segment `[start, start+len)` of a video.
#[allow(clippy::too_many_arguments)]
async fn write_segment(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    p: &Params,
    start: f64,
    len: Option<f64>,
    copy: bool,
    out_ext: &str,
    out: &Path,
    sp: (f32, f32),
    warnings: &mut Vec<String>,
) -> ToolResult<()> {
    let mut ff = Ff::new();
    if start > 0.0 {
        ff = ff.args(["-ss", &secs(start)]);
    }
    ff = ff.input(&f.path);
    if let Some(l) = len {
        ff = ff.args(["-t", &secs(l)]);
    }
    let out_dur = len.or(pr.duration().map(|d| d - start));
    if copy {
        ff = ff.args([
            "-map",
            "0:V?",
            "-map",
            "0:a?",
            "-c",
            "copy",
            "-avoid_negative_ts",
            "make_zero",
            "-map_metadata",
            "0",
        ]);
    } else {
        let re = reencode(ctx, f, pr, p, false, warnings).await?;
        let _ = out_ext;
        ff = ff
            .args(["-map", "0:V:0", "-map", "0:a?"])
            .args(re.tail())
            .a("-sn");
    }
    ff.out(out)
        .duration(out_dur)
        .span(sp, format!("Cutting {}", f.name))
        .run(ctx)
        .await?;
    Ok(())
}

fn cut_mode_param() -> ParamSpec {
    ParamSpec::choice(
        "mode",
        &[
            ("accurate", "Accurate (re-encode)"),
            ("copy", "Fast (stream copy, cuts on keyframes)"),
        ],
    )
    .default_str("accurate")
}

fn trim() -> Arc<dyn Tool> {
    let spec = video_spec("video.trim", "Trim Video")
        .describe("Keep only part of a video by start/end time or duration — fast keyframe stream copy or frame-accurate re-encode")
        .keywords(&["cut", "clip", "shorten", "start", "end", "extract part", "segment"])
        .param(video_input(true))
        .param(ParamSpec::text("start").placeholder("00:00:10 or 10").describe("Start time (seconds or HH:MM:SS.mmm)"))
        .param(ParamSpec::text("end").placeholder("00:01:00").describe("End time (takes precedence over duration)"))
        .param(ParamSpec::text("duration").placeholder("30").describe("Length of the clip"))
        .param(cut_mode_param())
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let mut rep = Report::files(&["Range"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let (start, len) = trim_range(&p, pr.duration())?;
            let copy = p.str("mode")? == "copy";
            let ext = same_container(f);
            let ext = if copy
                && !f.extension().is_empty()
                && VIDEO_CONTAINERS.contains(&f.extension().as_str())
            {
                f.extension()
            } else {
                ext.to_string()
            };
            let out = out_path(&ctx, f, "trimmed", &ext);
            write_segment(
                &ctx,
                f,
                &pr,
                &p,
                start,
                len,
                copy,
                &ext,
                &out,
                span(i, files.len()),
                &mut rep.warnings,
            )
            .await?;
            let end = len.map(|l| start + l).or(pr.duration()).unwrap_or(start);
            rep.file(
                &ctx,
                f,
                &out,
                vec![json!(format!("{} – {}", hms(start), hms(end)))],
                json!({"start": start, "end": end}),
            )?;
        }
        Ok(rep.finish())
    })
}

fn cut() -> Arc<dyn Tool> {
    let spec = video_spec("video.cut", "Cut / Split Video")
        .describe("Remove a section from a video, or split it into several files at timestamps or every N seconds")
        .keywords(&["split", "remove part", "delete segment", "chop", "chunks", "segments"])
        .param(video_input(false))
        .param(ParamSpec::choice("operation", &[("remove", "Remove a section"), ("split", "Split at timestamps"), ("every", "Split every N seconds")]).default_str("remove"))
        .param(ParamSpec::text("start").placeholder("00:00:10").describe("Start of the section to remove").show_if("operation", &["remove"]))
        .param(ParamSpec::text("end").placeholder("00:00:20").describe("End of the section to remove").show_if("operation", &["remove"]))
        .param(ParamSpec::text("points").placeholder("00:01:00, 00:02:30").describe("Split points (comma separated)").show_if("operation", &["split"]))
        .param(ParamSpec::number("every_seconds").range(1.0, 86400.0).default_num(60.0).show_if("operation", &["every"]))
        .param(cut_mode_param())
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let pr = probe_video(&ctx, &f).await?;
        let dur = require_dur(&pr, &f)?;
        let copy = p.str("mode")? == "copy";
        let ext = if copy && VIDEO_CONTAINERS.contains(&f.extension().as_str()) {
            f.extension()
        } else {
            same_container(&f).to_string()
        };
        let mut warnings = vec![];
        match p.str("operation")? {
            "remove" => {
                let s = opt_time(&p, "start")?
                    .ok_or_else(|| ToolError::invalid("start", "is required"))?;
                let e = opt_time(&p, "end")?
                    .ok_or_else(|| ToolError::invalid("end", "is required"))?
                    .min(dur);
                if e <= s || s >= dur {
                    return Err(ToolError::invalid(
                        "end",
                        "the section must have a positive length inside the video",
                    ));
                }
                let keep: Vec<(f64, f64)> = [(0.0, s), (e, dur)]
                    .into_iter()
                    .filter(|(a, b)| b - a > 0.04)
                    .collect();
                if keep.is_empty() {
                    return Err(ToolError::input(
                        "removing this section would leave nothing",
                    ));
                }
                let out = out_path(&ctx, &f, "cut", &ext);
                let total: f64 = keep.iter().map(|(a, b)| b - a).sum();
                if copy {
                    let dir = ctx.temp_dir("cut")?;
                    let mut parts = vec![];
                    for (k, (a, b)) in keep.iter().enumerate() {
                        let part = dir.join(format!("part{k}.{ext}"));
                        write_segment(
                            &ctx,
                            &f,
                            &pr,
                            &p,
                            *a,
                            Some(b - a),
                            true,
                            &ext,
                            &part,
                            sub((0.0, 1.0), k as f32 * 0.45, (k as f32 + 1.0) * 0.45),
                            &mut warnings,
                        )
                        .await?;
                        parts.push(part);
                    }
                    concat_copy(&ctx, &parts, &out, Some(total), (0.9, 1.0)).await?;
                } else {
                    let has_a = pr.audio().is_some();
                    let mut fc = vec![];
                    let mut labels = String::new();
                    for (k, (a, b)) in keep.iter().enumerate() {
                        fc.push(format!(
                            "[0:v]trim=start={}:end={},setpts=PTS-STARTPTS[v{k}]",
                            secs(*a),
                            secs(*b)
                        ));
                        labels.push_str(&format!("[v{k}]"));
                        if has_a {
                            fc.push(format!(
                                "[0:a]atrim=start={}:end={},asetpts=PTS-STARTPTS[a{k}]",
                                secs(*a),
                                secs(*b)
                            ));
                            labels.push_str(&format!("[a{k}]"));
                        }
                    }
                    fc.push(format!(
                        "{labels}concat=n={}:v=1:a={}[v]{}",
                        keep.len(),
                        has_a as u8,
                        if has_a { "[a]" } else { "" }
                    ));
                    let re = reencode(&ctx, &f, &pr, &p, false, &mut warnings).await?;
                    let mut ff = Ff::new().input(&f.path).args([
                        "-filter_complex",
                        &fc.join(";"),
                        "-map",
                        "[v]",
                    ]);
                    if has_a {
                        ff = ff.args(["-map", "[a]"]);
                    }
                    ff.args(re.tail())
                        .out(&out)
                        .duration(Some(total))
                        .span((0.0, 1.0), format!("Cutting {}", f.name))
                        .run(&ctx)
                        .await?;
                }
                let a = ctx.artifact(&out)?;
                let mut o = ToolOutput::files(vec![a])
                    .with_data(json!({"removed": {"start": s, "end": e}, "duration": total}));
                o.warnings = warnings;
                Ok(o)
            }
            op => {
                let mut points: Vec<f64> = if op == "split" {
                    let v = time_list(&p, "points")?;
                    if v.is_empty() {
                        return Err(ToolError::invalid(
                            "points",
                            "give at least one split point",
                        ));
                    }
                    v
                } else {
                    let n = p.num("every_seconds")?;
                    if dur / n > 1000.0 {
                        return Err(ToolError::invalid(
                            "every_seconds",
                            "this would create more than 1000 files",
                        ));
                    }
                    (1..)
                        .map(|k| k as f64 * n)
                        .take_while(|t| *t < dur - 0.05)
                        .collect()
                };
                points.retain(|t| *t > 0.05 && *t < dur - 0.05);
                points.sort_by(|a, b| a.partial_cmp(b).unwrap());
                points.dedup_by(|a, b| (*a - *b).abs() < 0.05);
                let mut bounds = vec![0.0];
                bounds.extend(points);
                bounds.push(dur);
                let n = bounds.len() - 1;
                let mut rep = Report::new(&["Part", "File", "Start", "End", "Size"]);
                for k in 0..n {
                    let (a, b) = (bounds[k], bounds[k + 1]);
                    let out = out_path(&ctx, &f, &format!("part{:03}", k + 1), &ext);
                    write_segment(
                        &ctx,
                        &f,
                        &pr,
                        &p,
                        a,
                        if k + 1 == n { None } else { Some(b - a) },
                        copy,
                        &ext,
                        &out,
                        span(k, n),
                        &mut rep.warnings,
                    )
                    .await?;
                    let art = ctx.artifact(&out)?;
                    rep.rows.push(vec![
                        json!(k + 1),
                        json!(art.name),
                        json!(hms(a)),
                        json!(hms(b)),
                        json!(human_size(art.size)),
                    ]);
                    rep.items
                        .push(json!({"part": k + 1, "output": art.name, "start": a, "end": b}));
                    rep.artifacts.push(art);
                }
                rep.warnings.extend(warnings);
                Ok(rep.finish())
            }
        }
    })
}

/// Concatenate files with the concat demuxer (stream copy).
pub(crate) async fn concat_copy(
    ctx: &ToolContext,
    parts: &[PathBuf],
    out: &Path,
    dur: Option<f64>,
    sp: (f32, f32),
) -> ToolResult<()> {
    let list = ctx.temp_path(&format!("concat-{}.txt", dogubako_core::util::random_id(4)));
    let mut body = String::from("ffconcat version 1.0\n");
    for p in parts {
        let abs = arg_path(p).to_string_lossy().to_string();
        if abs.contains('\n') || abs.contains('\r') {
            return Err(ToolError::input(
                "file names with line breaks cannot be concatenated",
            ));
        }
        body.push_str(&format!("file '{}'\n", abs.replace('\'', "'\\''")));
    }
    std::fs::write(&list, body)?;
    Ff::new()
        .args(["-f", "concat", "-safe", "0"])
        .input(&list)
        .args([
            "-map",
            "0:V?",
            "-map",
            "0:a?",
            "-c",
            "copy",
            "-movflags",
            "+faststart",
        ])
        .out(out)
        .duration(dur)
        .span(sp, "Joining")
        .run(ctx)
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// video.crop

fn crop() -> Arc<dyn Tool> {
    let spec = video_spec("video.crop", "Crop Video")
        .describe("Crop videos to a rectangle, a centered aspect ratio (16:9, 9:16, 1:1…) or automatically remove black bars (cropdetect)")
        .keywords(&["crop", "black bars", "letterbox", "aspect ratio", "square", "vertical", "9:16", "cropdetect"])
        .param(video_input(true))
        .param(ParamSpec::choice("mode", &[("aspect", "Aspect ratio (centered)"), ("manual", "Rectangle"), ("auto", "Auto-detect black bars")]).default_str("aspect"))
        .param(ParamSpec::choice_of("aspect", &["16:9", "9:16", "1:1", "4:3", "3:4", "4:5", "21:9", "2:1"]).default_str("1:1").show_if("mode", &["aspect"]))
        .param(ParamSpec::integer("x").min(0.0).default_int(0).show_if("mode", &["manual"]))
        .param(ParamSpec::integer("y").min(0.0).default_int(0).show_if("mode", &["manual"]))
        .param(ParamSpec::integer("width").min(2.0).show_if("mode", &["manual"]))
        .param(ParamSpec::integer("height").min(2.0).show_if("mode", &["manual"]))
        .param(ParamSpec::integer("threshold").range(0.0, 255.0).default_int(24).describe("Black level threshold for auto-detection").show_if("mode", &["auto"]).advanced())
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        vf_batch(
            ctx,
            p,
            "cropped",
            "Cropping",
            &["Crop"],
            |ctx, p, f, pr, sp| {
                Box::pin(async move {
                    let (sw, sh) = pr
                        .dims()
                        .ok_or_else(|| ToolError::input("unknown video dimensions"))?;
                    let (w, h, x, y) = match p.str("mode")? {
                        "manual" => {
                            let w = p
                                .opt_int("width")
                                .ok_or_else(|| ToolError::invalid("width", "is required"))?
                                as u32;
                            let h = p
                                .opt_int("height")
                                .ok_or_else(|| ToolError::invalid("height", "is required"))?
                                as u32;
                            let x = p.int_or("x", 0) as u32;
                            let y = p.int_or("y", 0) as u32;
                            if x + w > sw || y + h > sh {
                                return Err(ToolError::input(format!(
                                    "crop {w}x{h}+{x}+{y} exceeds the video size {sw}x{sh}"
                                )));
                            }
                            (w, h, x, y)
                        }
                        "aspect" => {
                            let (a, b) = p
                                .str("aspect")?
                                .split_once(':')
                                .ok_or_else(|| ToolError::invalid("aspect", "bad ratio"))?;
                            let r: f64 =
                                a.parse::<f64>().unwrap_or(1.0) / b.parse::<f64>().unwrap_or(1.0);
                            let (mut w, mut h) = (sw as f64, sw as f64 / r);
                            if h > sh as f64 {
                                h = sh as f64;
                                w = h * r;
                            }
                            let (w, h) = (w.floor() as u32, h.floor() as u32);
                            (w, h, (sw - w) / 2, (sh - h) / 2)
                        }
                        _ => {
                            detect_crop(&ctx, &f, &pr, p.int_or("threshold", 24), sub(sp, 0.0, 0.2))
                                .await?
                                .unwrap_or((sw, sh, 0, 0))
                        }
                    };
                    let (w, h) = (w & !1, h & !1);
                    if w < 2 || h < 2 {
                        return Err(ToolError::input("crop area is too small"));
                    }
                    let mut warnings = vec![];
                    if (w, h) == (sw & !1, sh & !1) && p.str("mode")? == "auto" {
                        warnings.push(format!("{}: no black bars detected", f.name));
                    }
                    Ok(VfJob {
                        vf: vec![format!("crop={w}:{h}:{x}:{y}")],
                        copy_audio: true,
                        row: vec![json!(format!("{w}x{h} at {x},{y}"))],
                        item: json!({"x": x, "y": y, "width": w, "height": h}),
                        warnings,
                        ..Default::default()
                    })
                })
            },
        )
        .await
    })
}

/// Run cropdetect over a sample of the video, returning (w, h, x, y).
async fn detect_crop(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    limit: i64,
    sp: (f32, f32),
) -> ToolResult<Option<(u32, u32, u32, u32)>> {
    let dur = pr.duration().unwrap_or(0.0);
    let start = if dur > 30.0 { dur * 0.1 } else { 0.0 };
    let len = if dur > 0.0 {
        (dur - start).min(60.0)
    } else {
        60.0
    };
    let out = Ff::new()
        .args(["-ss", &secs(start)])
        .input(&f.path)
        .args([
            "-t",
            &secs(len),
            "-an",
            "-sn",
            "-vf",
            &format!("fps=4,cropdetect=limit={limit}:round=2:reset=0"),
        ])
        .null_out()
        .verbose()
        .duration(Some(len))
        .span(sp, format!("Detecting borders in {}", f.name))
        .run(ctx)
        .await?;
    Ok(out.log.iter().rev().find_map(|l| parse_crop(l)))
}

pub(crate) fn parse_crop(line: &str) -> Option<(u32, u32, u32, u32)> {
    let i = line.rfind("crop=")?;
    let v: Vec<u32> = line[i + 5..]
        .split_whitespace()
        .next()?
        .split(':')
        .map(|s| s.parse().ok())
        .collect::<Option<Vec<_>>>()?;
    if v.len() == 4 && v[0] > 0 && v[1] > 0 {
        Some((v[0], v[1], v[2], v[3]))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// video.rotate

fn rotate() -> Arc<dyn Tool> {
    let spec = video_spec("video.rotate", "Rotate / Flip Video")
        .describe("Rotate videos by 90/180/270 degrees or mirror them; optionally change only the rotation metadata (instant, lossless)")
        .keywords(&["rotate", "flip", "mirror", "portrait", "landscape", "orientation", "sideways", "upside down"])
        .param(video_input(true))
        .param(ParamSpec::choice("rotation", &[("cw90", "90° clockwise"), ("ccw90", "90° counter-clockwise"), ("180", "180°"), ("hflip", "Flip horizontally"), ("vflip", "Flip vertically")]).default_str("cw90"))
        .param(ParamSpec::choice("method", &[("reencode", "Re-encode (works everywhere)"), ("metadata", "Metadata only (lossless, instant)")]).default_str("reencode"))
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        let rot = p.str("rotation")?.to_string();
        if p.str("method")? == "metadata" {
            let files = p.files("input").to_vec();
            let mut rep = Report::files(&["Rotation"]);
            for (i, f) in files.iter().enumerate() {
                let pr = probe_video(&ctx, f).await?;
                let cur = pr.rotation();
                let ext = if VIDEO_CONTAINERS.contains(&f.extension().as_str())
                    && f.extension() != "avi"
                    && f.extension() != "webm"
                {
                    f.extension()
                } else {
                    "mp4".into()
                };
                let out = out_path(&ctx, f, "rotated", &ext);
                let mut ff = Ff::new();
                let newrot = match rot.as_str() {
                    "cw90" => cur - 90,
                    "ccw90" => cur + 90,
                    "180" => cur + 180,
                    _ => cur,
                };
                let newrot = (newrot + 180).rem_euclid(360) - 180;
                ff = ff.args(["-display_rotation:v:0", &newrot.to_string()]);
                if rot == "hflip" {
                    ff = ff.args(["-display_hflip:v:0"]);
                } else if rot == "vflip" {
                    ff = ff.args(["-display_vflip:v:0"]);
                }
                ff.input(&f.path)
                    .args(["-map", "0", "-dn", "-c", "copy", "-map_metadata", "0"])
                    .out(&out)
                    .duration(pr.duration())
                    .span(span(i, files.len()), format!("Rotating {}", f.name))
                    .run(&ctx)
                    .await?;
                rep.file(
                    &ctx,
                    f,
                    &out,
                    vec![json!(format!("{cur}° → {newrot}° (metadata)"))],
                    json!({"rotation_metadata": newrot}),
                )?;
            }
            return Ok(rep.finish());
        }
        vf_batch(
            ctx,
            p,
            "rotated",
            "Rotating",
            &["Rotation"],
            move |_ctx, _p, _f, _pr, _sp| {
                let rot = rot.clone();
                Box::pin(async move {
                    let vf: Vec<String> = match rot.as_str() {
                        "cw90" => vec!["transpose=clock".into()],
                        "ccw90" => vec!["transpose=cclock".into()],
                        "180" => vec!["hflip".into(), "vflip".into()],
                        "hflip" => vec!["hflip".into()],
                        _ => vec!["vflip".into()],
                    };
                    Ok(VfJob {
                        vf,
                        copy_audio: true,
                        row: vec![json!(rot)],
                        item: json!({"rotation": rot}),
                        ..Default::default()
                    })
                })
            },
        )
        .await
    })
}

// ---------------------------------------------------------------------------
// video.merge

fn merge() -> Arc<dyn Tool> {
    let spec = video_spec("video.merge", "Merge Videos")
        .describe("Join videos end to end — lossless stream copy when they match, or re-encode to a common resolution and frame rate")
        .keywords(&["join", "concatenate", "combine", "append", "concat"])
        .param(video_input(true).describe("Videos in the order they should be joined (at least two)"))
        .param(ParamSpec::choice("mode", &[("auto", "Auto (copy when compatible)"), ("copy", "Stream copy (identical formats only)"), ("reencode", "Re-encode (any inputs)")]).default_str("auto"))
        .param(ParamSpec::choice("resolution", &[("first", "Same as first video"), ("largest", "Largest input"), ("1080", "1080p"), ("720", "720p"), ("480", "480p")]).default_str("first").show_if("mode", &["auto", "reencode"]))
        .param(ParamSpec::choice("format", &[("same", "Same as first video"), ("mp4", "MP4"), ("mkv", "MKV"), ("webm", "WebM"), ("mov", "MOV")]).default_str("same"))
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        if files.len() < 2 {
            return Err(ToolError::invalid(
                "input",
                "give at least two videos to merge",
            ));
        }
        let mut probes = vec![];
        for f in &files {
            probes.push(probe_video(&ctx, f).await?);
        }
        let total: f64 = probes.iter().filter_map(|p| p.duration()).sum();
        let sig = |pr: &Probe| {
            let a = pr.audio();
            (
                pr.video_codec().map(str::to_string),
                pr.coded_dims(),
                pr.video()
                    .and_then(|v| v["pix_fmt"].as_str())
                    .map(str::to_string),
                pr.fps().map(|f| (f * 100.0).round() as i64),
                a.and_then(|a| a["codec_name"].as_str()).map(str::to_string),
                pr.sample_rate(),
                pr.channels(),
            )
        };
        let compatible = probes.windows(2).all(|w| sig(&w[0]) == sig(&w[1]));
        let mode = p.str("mode")?;
        if mode == "copy" && !compatible {
            return Err(ToolError::input("the videos differ in codec, resolution, frame rate or audio format — use re-encode mode"));
        }
        let fmt = match p.str("format")? {
            "same" => same_container(&files[0]).to_string(),
            other => other.to_string(),
        };
        let first = &files[0];
        let out = ctx.output_path(&suffixed(&first.name, "merged", Some(&fmt)).replace('%', "_"));
        let mut warnings = vec![];
        let method = if mode != "reencode" && compatible {
            let vc = probes[0].video_codec().and_then(VCodec::from_probe);
            if let Some(c) = vc {
                if !video_ok_in(&fmt, c) {
                    return Err(ToolError::invalid(
                        "format",
                        format!(
                            "{} video cannot be stored in {fmt} without re-encoding",
                            c.id()
                        ),
                    ));
                }
            }
            let paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
            concat_copy(&ctx, &paths, &out, Some(total), (0.0, 1.0)).await?;
            "stream copy"
        } else {
            let dims: Vec<(u32, u32)> = probes.iter().filter_map(|p| p.dims()).collect();
            let (w, h) = match p.str_or("resolution", "first") {
                "largest" => *dims
                    .iter()
                    .max_by_key(|(w, h)| w * h)
                    .unwrap_or(&(1280, 720)),
                "first" => dims.first().copied().unwrap_or((1280, 720)),
                r => {
                    let short: f64 = r.parse().unwrap_or(720.0);
                    let (fw, fh) = dims.first().copied().unwrap_or((1280, 720));
                    let k = short / fw.min(fh) as f64;
                    (even(fw as f64 * k), even(fh as f64 * k))
                }
            };
            let (w, h) = (w & !1, h & !1);
            let fps = probes[0].fps().unwrap_or(30.0).min(120.0);
            let any_audio = probes.iter().any(|p| p.audio().is_some());
            let mut ff = Ff::new();
            for f in &files {
                ff = ff.input(&f.path);
            }
            let mut fc = vec![];
            let mut labels = String::new();
            for (i, pr) in probes.iter().enumerate() {
                fc.push(format!("[{i}:v:0]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps:.4},format=yuv420p[v{i}]"));
                labels.push_str(&format!("[v{i}]"));
                if any_audio {
                    if pr.audio().is_some() {
                        fc.push(format!("[{i}:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a{i}]"));
                    } else {
                        fc.push(format!(
                            "anullsrc=r=48000:cl=stereo,atrim=duration={}[a{i}]",
                            secs(pr.duration().unwrap_or(1.0))
                        ));
                    }
                    labels.push_str(&format!("[a{i}]"));
                }
            }
            fc.push(format!(
                "{labels}concat=n={}:v=1:a={}[v]{}",
                files.len(),
                any_audio as u8,
                if any_audio { "[a]" } else { "" }
            ));
            let (dv, da) = container_defaults(&fmt);
            let o = EncodeOpts::from_params(&p);
            let enc = video_encoder(
                &ctx,
                dv,
                &EncodeOpts {
                    hw: "none".into(),
                    ..o
                },
                &mut warnings,
            )
            .await?;
            ff = ff.args(["-filter_complex", &fc.join(";"), "-map", "[v]"]);
            if any_audio {
                ff = ff.args(["-map", "[a]"]).args(da.args(192));
            }
            ff.args(&enc.args)
                .args(mux_args(&fmt, Some(dv)))
                .out(&out)
                .duration(Some(total))
                .span((0.0, 1.0), "Merging")
                .run(&ctx)
                .await?;
            "re-encoded"
        };
        let a = ctx.artifact(&out)?;
        let rows: Vec<Value> = files
            .iter()
            .zip(&probes)
            .enumerate()
            .map(|(i, (f, pr))| {
                json!([
                    i + 1,
                    f.name,
                    pr.duration().map(hms).unwrap_or_default(),
                    pr.dims()
                        .map(|(w, h)| format!("{w}x{h}"))
                        .unwrap_or_default()
                ])
            })
            .collect();
        let mut o = ToolOutput::files(vec![a]).with_data(json!({"method": method, "duration": total, "table": {"columns": ["#", "File", "Duration", "Resolution"], "rows": rows}}));
        o.warnings = warnings;
        Ok(o)
    })
}

// ---------------------------------------------------------------------------
// Audio extraction (shared with audio.extract_from_video)

pub(crate) fn extract_audio_tool(id: &str, cat: Category) -> Arc<dyn Tool> {
    let spec = ToolSpec::new(id, "Extract Audio from Video", cat)
        .requires(FF)
        .output(OutputKind::Files)
        .progress()
        .timeout(3 * 3600)
        .describe("Save the soundtrack of videos as audio — lossless stream copy or MP3, AAC, WAV, FLAC, Opus or Ogg")
        .keywords(&["rip audio", "soundtrack", "video to mp3", "mp4 to mp3", "demux", "audio track"])
        .param(video_input(true))
        .param(ParamSpec::choice("format", &[("copy", "Original (no re-encode)"), ("mp3", "MP3"), ("m4a", "AAC (M4A)"), ("wav", "WAV"), ("flac", "FLAC"), ("opus", "Opus"), ("ogg", "Ogg Vorbis")]).default_str("mp3"))
        .param(ParamSpec::integer("bitrate").label("Bitrate (kbit/s)").range(32.0, 320.0).default_int(192).show_if("format", &["mp3", "m4a", "opus"]))
        .param(ParamSpec::integer("track").range(0.0, 64.0).default_int(0).describe("Audio track number (0 = first)").advanced());
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let fmt = p.str("format")?.to_string();
        let kbps = p.int_or("bitrate", 192) as u32;
        let track = p.int_or("track", 0) as usize;
        let mut rep = Report::files(&["Codec"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_audio(&ctx, f).await?;
            let streams = pr.audio_streams();
            let st = streams.get(track).ok_or_else(|| {
                ToolError::invalid(
                    "track",
                    format!("'{}' has only {} audio track(s)", f.name, streams.len()),
                )
            })?;
            let src = st["codec_name"].as_str().unwrap_or("unknown").to_string();
            let (ext, args): (String, Vec<String>) = match fmt.as_str() {
                "copy" => {
                    let ext = match src.as_str() {
                        "aac" | "alac" => "m4a",
                        "mp3" => "mp3",
                        "opus" => "opus",
                        "vorbis" => "ogg",
                        "flac" => "flac",
                        "ac3" => "ac3",
                        "eac3" => "eac3",
                        c if c.starts_with("pcm_") => "wav",
                        _ => "mka",
                    };
                    (ext.into(), vec!["-c:a".into(), "copy".into()])
                }
                "mp3" => ("mp3".into(), ACodec::Mp3.args(kbps)),
                "m4a" => ("m4a".into(), ACodec::Aac.args(kbps)),
                "opus" => ("opus".into(), ACodec::Opus.args(kbps.min(256))),
                "ogg" => (
                    "ogg".into(),
                    vec!["-c:a".into(), "libvorbis".into(), "-q:a".into(), "6".into()],
                ),
                "flac" => ("flac".into(), vec!["-c:a".into(), "flac".into()]),
                _ => ("wav".into(), audio_args_for_ext("wav", Some(&pr))),
            };
            let out = out_path(&ctx, f, "audio", &ext);
            let mut ff = Ff::new()
                .input(&f.path)
                .args([
                    "-map",
                    &format!("0:a:{track}"),
                    "-vn",
                    "-sn",
                    "-dn",
                    "-map_metadata",
                    "0",
                ])
                .args(args);
            if ext == "m4a" {
                ff = ff.args(["-movflags", "+faststart"]);
            }
            ff.out(&out)
                .duration(pr.duration())
                .span(
                    span(i, files.len()),
                    format!("Extracting audio from {}", f.name),
                )
                .run(&ctx)
                .await?;
            rep.file(
                &ctx,
                f,
                &out,
                vec![json!(if fmt == "copy" {
                    format!("{src} (copied)")
                } else {
                    fmt.clone()
                })],
                json!({"source_codec": src}),
            )?;
        }
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// video.remove_audio / video.replace_audio

fn remove_audio() -> Arc<dyn Tool> {
    let spec = video_spec("video.remove_audio", "Remove Audio")
        .describe("Strip all audio tracks from videos without re-encoding the picture")
        .keywords(&["mute", "silent", "strip audio", "no sound", "remove sound"])
        .param(video_input(true));
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let mut rep = Report::files(&[]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let ext = if VIDEO_CONTAINERS.contains(&f.extension().as_str()) {
                f.extension()
            } else {
                "mkv".into()
            };
            let out = out_path(&ctx, f, "muted", &ext);
            Ff::new()
                .input(&f.path)
                .args([
                    "-map",
                    "0",
                    "-map",
                    "-0:a",
                    "-dn",
                    "-c",
                    "copy",
                    "-map_metadata",
                    "0",
                ])
                .out(&out)
                .duration(pr.duration())
                .span(span(i, files.len()), format!("Muting {}", f.name))
                .run(&ctx)
                .await?;
            rep.file(&ctx, f, &out, vec![], json!({}))?;
        }
        Ok(rep.finish())
    })
}

fn replace_audio() -> Arc<dyn Tool> {
    let spec = video_spec("video.replace_audio", "Replace Audio")
        .describe("Put a new soundtrack on videos — end at the shorter stream, loop or pad the audio, delay it, or mix it with the original sound")
        .keywords(&["add music", "background music", "soundtrack", "dub", "voiceover", "mix audio"])
        .param(video_input(true))
        .param(ParamSpec::file("audio").label("New audio").accept(MEDIA_ACCEPT).required())
        .param(ParamSpec::choice("fit", &[("shortest", "End with the shorter stream"), ("loop", "Loop audio to video length"), ("pad", "Pad audio with silence to video length")]).default_str("pad"))
        .param(ParamSpec::boolean("mix_original").describe("Mix the new audio with the video's existing sound instead of replacing it"))
        .param(ParamSpec::number("original_volume").range(0.0, 4.0).default_num(0.3).show_if_bool("mix_original", true))
        .param(ParamSpec::number("new_volume").range(0.0, 4.0).default_num(1.0).advanced())
        .param(ParamSpec::text("offset").placeholder("0").describe("Delay before the new audio starts").advanced());
    async_tool(spec, |ctx, p| async move {
        let audio = p.file("audio")?.clone();
        let apr = probe_audio(&ctx, &audio).await?;
        let files = p.files("input").to_vec();
        let fit = p.str("fit")?.to_string();
        let offset = opt_time(&p, "offset")?.unwrap_or(0.0);
        let mut rep = Report::files(&[]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let ext = same_container(f);
            let (_, da) = container_defaults(ext);
            let mut ff = Ff::new().input(&f.path);
            if fit == "loop" {
                ff = ff.args(["-stream_loop", "-1"]);
            }
            ff = ff.input(&audio.path);
            let mut chain = format!("[1:a:0]volume={:.3}", p.num_or("new_volume", 1.0));
            if offset > 0.0 {
                chain.push_str(&format!(
                    ",adelay=delays={}:all=1",
                    (offset * 1000.0).round() as i64
                ));
            }
            if fit == "pad" {
                chain.push_str(",apad");
            }
            chain.push_str("[n]");
            let mix = p.bool("mix_original") && pr.audio().is_some();
            let fc = if mix {
                format!("{chain};[0:a:0]volume={:.3}[o];[o][n]amix=inputs=2:duration=first:dropout_transition=0:normalize=0[a]", p.num_or("original_volume", 0.3))
            } else {
                chain.replace("[n]", "[a]")
            };
            let out = out_path(&ctx, f, "new-audio", ext);
            ff = ff
                .args([
                    "-filter_complex",
                    &fc,
                    "-map",
                    "0:V:0",
                    "-map",
                    "[a]",
                    "-c:v",
                    "copy",
                ])
                .args(da.args(192))
                .args(mux_args(ext, None));
            if fit != "pad" || !mix {
                ff = ff.a("-shortest");
            }
            let dur = match fit.as_str() {
                "shortest" => match (pr.duration(), apr.duration()) {
                    (Some(a), Some(b)) => Some(a.min(b + offset)),
                    (a, _) => a,
                },
                _ => pr.duration(),
            };
            ff.out(&out)
                .duration(dur)
                .span(
                    span(i, files.len()),
                    format!("Replacing audio of {}", f.name),
                )
                .run(&ctx)
                .await?;
            rep.file(&ctx, f, &out, vec![], json!({"mixed": mix}))?;
        }
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// video.extract_frames

fn image_args(fmt: &str) -> Vec<String> {
    match fmt {
        "jpg" => vec!["-q:v".into(), "2".into()],
        "webp" => vec![
            "-c:v".into(),
            "libwebp".into(),
            "-quality".into(),
            "90".into(),
        ],
        _ => vec![],
    }
}

fn extract_frames() -> Arc<dyn Tool> {
    let spec = video_spec("video.extract_frames", "Extract Frames")
        .describe("Export frames as images: every N seconds, at a frame rate, at specific timestamps, at scene changes, keyframes only, or every frame — optionally zipped")
        .keywords(&["frames", "images", "screenshots", "stills", "png sequence", "scene detection", "keyframes"])
        .param(video_input(true))
        .param(
            ParamSpec::choice("mode", &[("interval", "Every N seconds"), ("fps", "Frames per second"), ("timestamps", "At timestamps"), ("scene", "At scene changes"), ("keyframes", "Keyframes only"), ("all", "Every frame")])
                .default_str("interval"),
        )
        .param(ParamSpec::number("interval").label("Interval (seconds)").range(0.01, 86400.0).default_num(1.0).show_if("mode", &["interval"]))
        .param(ParamSpec::number("fps").range(0.001, 240.0).default_num(1.0).show_if("mode", &["fps"]))
        .param(ParamSpec::text("timestamps").placeholder("0:05, 0:30, 1:12.5").show_if("mode", &["timestamps"]))
        .param(ParamSpec::number("scene_threshold").range(0.01, 1.0).default_num(0.3).describe("Scene change sensitivity (lower = more frames)").show_if("mode", &["scene"]))
        .param(ParamSpec::choice("format", &[("png", "PNG"), ("jpg", "JPEG"), ("webp", "WebP")]).default_str("png"))
        .param(ParamSpec::integer("width").range(16.0, 8192.0).describe("Scale frames to this width (keeps aspect)").advanced())
        .param(ParamSpec::integer("max_frames").range(1.0, 20000.0).default_int(300))
        .param(ParamSpec::boolean("zip").describe("Return a single ZIP archive (automatic above 100 frames)"));
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let fmt = p.str("format")?.to_string();
        let max = p.int_or("max_frames", 300);
        let mode = p.str("mode")?.to_string();
        let mut artifacts = vec![];
        let mut warnings = vec![];
        let mut frames_data = vec![];
        let mut rows = vec![];
        for (i, f) in files.iter().enumerate() {
            let sp = span(i, files.len());
            let pr = probe_video(&ctx, f).await?;
            let dir = ctx.temp_dir(&format!("frames-{i}"))?;
            let stem = f.stem().replace('%', "_");
            let scale = p
                .opt_int("width")
                .map(|w| format!("scale={w}:-2:flags=lanczos"));
            let mut produced: Vec<(PathBuf, f64)> = vec![];
            if mode == "timestamps" {
                let ts = time_list(&p, "timestamps")?;
                if ts.is_empty() {
                    return Err(ToolError::invalid(
                        "timestamps",
                        "give at least one timestamp",
                    ));
                }
                for (k, t) in ts.iter().take(max as usize).enumerate() {
                    if pr.duration().map(|d| *t > d).unwrap_or(false) {
                        warnings.push(format!("{}: {} is beyond the end", f.name, hms(*t)));
                        continue;
                    }
                    let path = dir.join(format!("{stem}-{}.{fmt}", hms(*t).replace(':', "-")));
                    let mut ff = Ff::new().args(["-ss", &secs(*t)]).input(&f.path).args([
                        "-frames:v",
                        "1",
                        "-update",
                        "1",
                        "-an",
                    ]);
                    if let Some(s) = &scale {
                        ff = ff.args(["-vf", s]);
                    }
                    ff.args(image_args(&fmt))
                        .out(&path)
                        .no_progress()
                        .run(&ctx)
                        .await?;
                    ctx.progress(
                        sp.0 + (sp.1 - sp.0) * (k + 1) as f32 / ts.len() as f32,
                        format!("Frame {}/{}", k + 1, ts.len()),
                    );
                    if path.exists() {
                        produced.push((path, *t));
                    }
                }
            } else {
                let mut vf: Vec<String> = vec![];
                let mut pre: Vec<String> = vec![];
                let mut post: Vec<String> = vec!["-fps_mode".into(), "vfr".into()];
                match mode.as_str() {
                    "interval" => vf.push(format!("fps=1/{}", p.num("interval")?)),
                    "fps" => vf.push(format!("fps={}", p.num("fps")?)),
                    "scene" => vf.push(format!("select='gt(scene,{})'", p.num("scene_threshold")?)),
                    "keyframes" => pre = vec!["-skip_frame".into(), "nokey".into()],
                    _ => post = vec!["-fps_mode".into(), "passthrough".into()],
                }
                if let Some(s) = &scale {
                    vf.push(s.clone());
                }
                vf.push("showinfo".into());
                let pattern = dir.join(format!("frame-%06d.{fmt}"));
                let out = Ff::new()
                    .args(&pre)
                    .input(&f.path)
                    .args(["-an", "-sn", "-vf", &vf.join(",")])
                    .args(&post)
                    .args(["-frames:v", &max.to_string()])
                    .args(image_args(&fmt))
                    .out(&pattern)
                    .verbose()
                    .duration(pr.duration())
                    .span(sp, format!("Extracting frames from {}", f.name))
                    .run(&ctx)
                    .await?;
                let times: Vec<f64> = out
                    .log
                    .iter()
                    .filter(|l| l.contains("Parsed_showinfo") && l.contains("pts_time:"))
                    .filter_map(|l| {
                        l.split("pts_time:")
                            .nth(1)?
                            .split_whitespace()
                            .next()?
                            .parse()
                            .ok()
                    })
                    .collect();
                let mut names: Vec<PathBuf> = std::fs::read_dir(&dir)?
                    .filter_map(|e| e.ok().map(|e| e.path()))
                    .collect();
                names.sort();
                for (k, path) in names.into_iter().enumerate() {
                    let t = times.get(k).copied().unwrap_or(0.0);
                    let new = dir.join(format!("{stem}-{:05}.{fmt}", k + 1));
                    std::fs::rename(&path, &new)?;
                    produced.push((new, t));
                }
                if produced.len() as i64 >= max {
                    warnings.push(format!(
                        "{}: stopped at the maximum of {max} frames",
                        f.name
                    ));
                }
            }
            if produced.is_empty() {
                warnings.push(format!("{}: no frames matched", f.name));
            }
            for (path, t) in &produced {
                let n = path.file_name().unwrap().to_string_lossy().to_string();
                rows.push(json!([f.name, n, hms(*t)]));
                frames_data.push(json!({"input": f.name, "file": n, "time": t}));
            }
            let paths: Vec<PathBuf> = produced.into_iter().map(|(p, _)| p).collect();
            if p.bool("zip") || paths.len() > 100 {
                if !paths.is_empty() {
                    artifacts.push(zip_files(&ctx, &paths, &format!("{stem}-frames.zip"))?);
                }
            } else {
                artifacts.extend(publish(&ctx, &paths)?);
            }
        }
        let mut o = ToolOutput::files(artifacts).with_data(json!({"count": frames_data.len(), "frames": frames_data, "table": {"columns": ["Video", "Frame", "Time"], "rows": rows}}));
        o.warnings = warnings;
        Ok(o)
    })
}

// ---------------------------------------------------------------------------
// video.thumbnail

fn thumbnail() -> Arc<dyn Tool> {
    let spec = video_spec("video.thumbnail", "Video Thumbnail")
        .describe("Grab a single frame as an image — at a time or percentage, or let FFmpeg's thumbnail filter pick the most representative frame")
        .keywords(&["poster", "cover", "preview image", "screenshot", "still", "frame grab"])
        .param(video_input(true))
        .param(ParamSpec::choice("mode", &[("best", "Most representative frame"), ("time", "At a specific time")]).default_str("best"))
        .param(ParamSpec::text("time").default_str("10%").placeholder("10% or 00:00:05").describe("Time or percentage of the duration (for 'best', where to start looking)"))
        .param(ParamSpec::integer("width").range(16.0, 8192.0).describe("Scale to this width (keeps aspect)"))
        .param(ParamSpec::choice("format", &[("jpg", "JPEG"), ("png", "PNG"), ("webp", "WebP")]).default_str("jpg"));
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let fmt = p.str("format")?.to_string();
        let mut rep = Report::new(&["Video", "Image", "Time"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let dur = pr.duration();
            let mut t = time_or_percent(&p, "time", dur)?.unwrap_or(0.0);
            if let Some(d) = dur {
                if t >= d {
                    t = (d * 0.1).max(0.0);
                }
            }
            let mut vf = vec![];
            if p.str("mode")? == "best" {
                vf.push("thumbnail=n=120".to_string());
            }
            if let Some(w) = p.opt_int("width") {
                vf.push(format!("scale={w}:-2:flags=lanczos"));
            }
            let out = out_path(&ctx, f, "thumbnail", &fmt);
            let mut ff = Ff::new().args(["-ss", &secs(t)]).input(&f.path).args([
                "-an",
                "-sn",
                "-frames:v",
                "1",
                "-update",
                "1",
            ]);
            if !vf.is_empty() {
                ff = ff.args(["-vf", &vf.join(",")]);
            }
            ff.args(image_args(&fmt))
                .out(&out)
                .no_progress()
                .run(&ctx)
                .await?;
            ctx.progress(span(i, files.len()).1, format!("Thumbnail {}", f.name));
            let a = ctx.artifact(&out)?;
            rep.rows
                .push(vec![json!(f.name), json!(a.name), json!(hms(t))]);
            rep.items
                .push(json!({"input": f.name, "output": a.name, "time": t}));
            rep.artifacts.push(a);
        }
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// GIF

pub(crate) struct GifOpts {
    pub fps: u32,
    pub width: u32,
    pub start: Option<f64>,
    pub duration: Option<f64>,
    /// GIF muxer semantics: 0 = forever, -1 = play once, N = repeat N times.
    pub loop_: i64,
    pub dither: String,
    pub colors: u32,
    pub stats: String,
    pub format: String,
}

pub(crate) async fn make_gif(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    g: &GifOpts,
    out: &Path,
    sp: (f32, f32),
) -> ToolResult<()> {
    let mut input = Ff::new();
    if let Some(s) = g.start {
        input = input.args(["-ss", &secs(s)]);
    }
    if let Some(d) = g.duration {
        input = input.args(["-t", &secs(d)]);
    }
    let dur = g
        .duration
        .or(pr.duration().map(|d| d - g.start.unwrap_or(0.0)));
    let scale = if g.width > 0 {
        format!(",scale={}:-1:flags=lanczos", g.width)
    } else {
        String::new()
    };
    let base = format!("fps={}{}", g.fps, scale);
    match g.format.as_str() {
        "gif" => {
            let palette = ctx.temp_path(&format!(
                "palette-{}.png",
                dogubako_core::util::random_id(4)
            ));
            input
                .clone()
                .input(&f.path)
                .args([
                    "-vf",
                    &format!(
                        "{base},palettegen=max_colors={}:stats_mode={}",
                        g.colors, g.stats
                    ),
                    "-frames:v",
                    "1",
                    "-update",
                    "1",
                ])
                .out(&palette)
                .duration(dur)
                .span(
                    sub(sp, 0.0, 0.4),
                    format!("Building palette for {}", f.name),
                )
                .run(ctx)
                .await?;
            let dither = if g.dither == "none" {
                "none".to_string()
            } else if g.dither == "bayer" {
                "bayer:bayer_scale=3".to_string()
            } else {
                g.dither.clone()
            };
            let diff = if g.stats == "diff" {
                ":diff_mode=rectangle"
            } else {
                ""
            };
            input
                .input(&f.path)
                .input(&palette)
                .args([
                    "-lavfi",
                    &format!("[0:v]{base}[x];[x][1:v]paletteuse=dither={dither}{diff}"),
                    "-loop",
                    &g.loop_.to_string(),
                ])
                .out(out)
                .duration(dur)
                .span(sub(sp, 0.4, 1.0), format!("Encoding {}", f.name))
                .run(ctx)
                .await?;
        }
        "webp" => {
            crate::ffmpeg::need_encoder(ctx, "libwebp_anim").await?;
            let lp = if g.loop_ < 0 { 1 } else { g.loop_ };
            input
                .input(&f.path)
                .args([
                    "-vf",
                    &base,
                    "-an",
                    "-c:v",
                    "libwebp_anim",
                    "-quality",
                    "75",
                    "-compression_level",
                    "4",
                    "-loop",
                    &lp.to_string(),
                ])
                .out(out)
                .duration(dur)
                .span(sp, format!("Encoding {}", f.name))
                .run(ctx)
                .await?;
        }
        _ => {
            let plays = if g.loop_ < 0 { 1 } else { g.loop_ };
            input
                .input(&f.path)
                .args([
                    "-vf",
                    &base,
                    "-an",
                    "-c:v",
                    "apng",
                    "-plays",
                    &plays.to_string(),
                    "-f",
                    "apng",
                ])
                .out(out)
                .duration(dur)
                .span(sp, format!("Encoding {}", f.name))
                .run(ctx)
                .await?;
        }
    }
    Ok(())
}

fn to_gif() -> Arc<dyn Tool> {
    let spec = video_spec("video.to_gif", "Video to GIF")
        .describe("Turn video clips into high-quality animated GIFs (two-pass palette), animated WebP or APNG with frame rate, width, range, looping and dithering control")
        .keywords(&["gif", "animated", "meme", "webp", "apng", "palette", "convert to gif"])
        .param(video_input(true))
        .param(ParamSpec::text("start").placeholder("0").describe("Start time"))
        .param(ParamSpec::text("duration").placeholder("5").describe("Length (empty = until the end)"))
        .param(ParamSpec::integer("fps").range(1.0, 50.0).default_int(12))
        .param(ParamSpec::integer("width").range(0.0, 4096.0).default_int(480).describe("Output width in pixels (0 = original)"))
        .param(ParamSpec::choice("loop", &[("forever", "Loop forever"), ("once", "Play once"), ("3", "3 times"), ("5", "5 times")]).default_str("forever"))
        .param(ParamSpec::choice("format", &[("gif", "GIF"), ("webp", "Animated WebP"), ("apng", "Animated PNG")]).default_str("gif"))
        .param(ParamSpec::choice("dither", &[("sierra2_4a", "Sierra (default)"), ("floyd_steinberg", "Floyd–Steinberg"), ("bayer", "Bayer (ordered)"), ("none", "None")]).default_str("sierra2_4a").show_if("format", &["gif"]).advanced())
        .param(ParamSpec::integer("colors").range(2.0, 256.0).default_int(256).show_if("format", &["gif"]).advanced())
        .param(ParamSpec::choice("palette_mode", &[("diff", "Optimize for motion"), ("full", "Whole frame")]).default_str("diff").show_if("format", &["gif"]).advanced());
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let fmt = p.str("format")?.to_string();
        let lp = p.str("loop")?;
        let loop_ = match lp {
            "forever" => 0,
            "once" => -1,
            n => n.parse::<i64>().unwrap_or(0) - 1,
        };
        let mut rep = Report::files(&[]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let mut w = p.int_or("width", 480) as u32;
            if let Some((sw, _)) = pr.dims() {
                if w > sw {
                    w = sw;
                }
            }
            let start = opt_time(&p, "start")?;
            if let (Some(s), Some(d)) = (start, pr.duration()) {
                if s >= d {
                    return Err(ToolError::invalid(
                        "start",
                        "start is beyond the end of the video",
                    ));
                }
            }
            let g = GifOpts {
                fps: p.int_or("fps", 12) as u32,
                width: w,
                start,
                duration: opt_time(&p, "duration")?.filter(|d| *d > 0.0),
                loop_: if fmt == "gif" { loop_ } else { loop_.max(-1) },
                dither: p.str_or("dither", "sierra2_4a").to_string(),
                colors: p.int_or("colors", 256) as u32,
                stats: p.str_or("palette_mode", "diff").to_string(),
                format: fmt.clone(),
            };
            let g = if fmt != "gif" {
                GifOpts {
                    loop_: if loop_ == -1 {
                        -1
                    } else if loop_ == 0 {
                        0
                    } else {
                        loop_ + 1
                    },
                    ..g
                }
            } else {
                g
            };
            let ext = if fmt == "apng" { "png" } else { fmt.as_str() };
            let out = out_path(&ctx, f, "anim", ext);
            make_gif(&ctx, f, &pr, &g, &out, span(i, files.len())).await?;
            rep.file(&ctx, f, &out, vec![], json!({"width": w, "fps": g.fps}))?;
        }
        Ok(rep.finish())
    })
}

fn from_gif() -> Arc<dyn Tool> {
    let spec = video_spec("video.from_gif", "GIF to Video")
        .describe("Convert animated GIF / WebP / APNG files to MP4 (H.264) or WebM (VP9) — much smaller and widely playable")
        .keywords(&["gif to mp4", "gif to webm", "animated", "convert gif"])
        .param(ParamSpec::files("input").label("Animations").accept(&["image/gif", ".gif", ".webp", ".apng", ".png"]).required().primary())
        .param(ParamSpec::choice("format", &[("mp4", "MP4 (H.264)"), ("webm", "WebM (VP9)")]).default_str("mp4"))
        .param(ParamSpec::integer("repeat").range(0.0, 100.0).default_int(0).describe("Extra times to repeat the animation"))
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let fmt = p.str("format")?.to_string();
        let mut rep = Report::files(&[]);
        let (codec, _) = container_defaults(&fmt);
        let mut warnings = vec![];
        let enc = video_encoder(&ctx, codec, &EncodeOpts::from_params(&p), &mut warnings).await?;
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let rep_n = p.int_or("repeat", 0);
            let out = out_path(&ctx, f, "video", &fmt);
            Ff::new()
                .args(["-stream_loop", &rep_n.to_string()])
                .input(&f.path)
                .args([
                    "-vf",
                    "scale=trunc(iw/2)*2:trunc(ih/2)*2,format=yuv420p",
                    "-an",
                ])
                .args(&enc.args)
                .args(mux_args(&fmt, Some(codec)))
                .out(&out)
                .duration(pr.duration().map(|d| d * (rep_n + 1) as f64))
                .span(span(i, files.len()), format!("Converting {}", f.name))
                .run(&ctx)
                .await?;
            rep.file(&ctx, f, &out, vec![], json!({}))?;
        }
        rep.warnings.extend(warnings);
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// video.speed

fn speed() -> Arc<dyn Tool> {
    let spec = video_spec("video.speed", "Change Video Speed")
        .describe("Speed up or slow down videos (0.25×–4×), adjusting the audio tempo without changing pitch")
        .keywords(&["fast forward", "slow motion", "timelapse", "playback speed", "speed up", "slow down"])
        .param(video_input(true))
        .param(ParamSpec::number("factor").range(0.25, 4.0).step(0.05).default_num(2.0).required().describe("2 = twice as fast, 0.5 = half speed"))
        .param(ParamSpec::choice("audio", &[("keep", "Keep (tempo adjusted, same pitch)"), ("mute", "Remove audio")]).default_str("keep"))
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        vf_batch(
            ctx,
            p,
            "speed",
            "Changing speed of",
            &["Speed"],
            |_ctx, p, _f, pr, _sp| {
                Box::pin(async move {
                    let k = p.num("factor")?;
                    let mute = p.str("audio")? == "mute";
                    Ok(VfJob {
                        vf: vec![format!("setpts=PTS/{k:.6}")],
                        af: if mute { None } else { Some(atempo_chain(k)) },
                        drop_audio: mute,
                        out_dur: pr.duration().map(|d| d / k),
                        row: vec![json!(format!("{k}×"))],
                        item: json!({"factor": k}),
                        ..Default::default()
                    })
                })
            },
        )
        .await
    })
}

// ---------------------------------------------------------------------------
// video.stabilize

fn stabilize() -> Arc<dyn Tool> {
    let spec = video_spec("video.stabilize", "Stabilize Video")
        .describe("Reduce camera shake with two-pass vid.stab (vidstabdetect/vidstabtransform) when available, otherwise FFmpeg's deshake filter")
        .keywords(&["shaky", "stabilise", "deshake", "smooth", "handheld", "vidstab", "gimbal"])
        .param(video_input(true))
        .param(ParamSpec::choice("strength", &[("low", "Low"), ("medium", "Medium"), ("high", "High")]).default_str("medium"))
        .param(ParamSpec::choice("method", &[("auto", "Auto"), ("vidstab", "vid.stab (two-pass)"), ("deshake", "deshake (single pass)")]).default_str("auto").advanced())
        .param(ParamSpec::choice("borders", &[("zoom", "Zoom to hide borders"), ("black", "Keep black borders")]).default_str("zoom").advanced())
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        let caps = caps(&ctx).await?;
        let has_vidstab = caps.filter("vidstabdetect") && caps.filter("vidstabtransform");
        let method = match p.str("method")? {
            "vidstab" if !has_vidstab => {
                return Err(ToolError::unsupported(
                    "this FFmpeg build has no vid.stab filters; use the deshake method",
                ))
            }
            "auto" => {
                if has_vidstab {
                    "vidstab"
                } else {
                    "deshake"
                }
            }
            m => m,
        }
        .to_string();
        vf_batch(ctx, p, "stabilized", "Stabilizing", &["Method"], move |ctx, p, f, pr, sp| {
            let method = method.clone();
            Box::pin(async move {
                let strength = p.str("strength")?.to_string();
                let zoom = p.str_or("borders", "zoom") == "zoom";
                if method == "vidstab" {
                    let dir = ctx.temp_dir(&format!("stab-{}", dogubako_core::util::random_id(4)))?;
                    let (shaky, smooth) = match strength.as_str() {
                        "low" => (4, 10),
                        "high" => (10, 45),
                        _ => (6, 25),
                    };
                    Ff::new()
                        .input(&f.path)
                        .args(["-an", "-sn", "-vf", &format!("vidstabdetect=shakiness={shaky}:accuracy=15:result=transforms.trf")])
                        .null_out()
                        .cwd(&dir)
                        .duration(pr.duration())
                        .span(sub(sp, 0.0, 0.4), format!("Analyzing motion in {}", f.name))
                        .run(&ctx)
                        .await?;
                    let vf = vec![format!("vidstabtransform=input=transforms.trf:smoothing={smooth}:optzoom={}:interpol=bicubic", if zoom { 1 } else { 0 }), "unsharp=5:5:0.8:3:3:0.4".to_string()];
                    Ok(VfJob { vf, copy_audio: true, cwd: Some(dir), row: vec![json!("vid.stab")], item: json!({"method": "vidstab"}), ..Default::default() })
                } else {
                    let r = match strength.as_str() {
                        "low" => 16,
                        "high" => 64,
                        _ => 32,
                    };
                    let edge = if zoom { "mirror" } else { "blank" };
                    Ok(VfJob { vf: vec![format!("deshake=rx={r}:ry={r}:edge={edge}")], copy_audio: true, row: vec![json!("deshake")], item: json!({"method": "deshake"}), ..Default::default() })
                }
            })
        })
        .await
    })
}

// ---------------------------------------------------------------------------
// video.info

pub(crate) fn stream_details(s: &Value) -> String {
    match s["codec_type"].as_str().unwrap_or("") {
        "video" => {
            let fps = s["avg_frame_rate"]
                .as_str()
                .and_then(ratio)
                .map(|f| format!(", {f:.3} fps"))
                .unwrap_or_default();
            format!(
                "{}x{}{}{}",
                s["width"],
                s["height"],
                fps,
                s["pix_fmt"]
                    .as_str()
                    .map(|p| format!(", {p}"))
                    .unwrap_or_default()
            )
        }
        "audio" => format!(
            "{} Hz, {} ch{}",
            s["sample_rate"].as_str().unwrap_or("?"),
            s["channels"],
            s["channel_layout"]
                .as_str()
                .map(|l| format!(" ({l})"))
                .unwrap_or_default()
        ),
        "subtitle" => s["tags"]["title"].as_str().unwrap_or("").to_string(),
        _ => String::new(),
    }
}

pub(crate) fn media_summary(pr: &Probe, name: &str) -> (Value, Vec<Value>, String) {
    let fmt = pr.format();
    let dur = pr.duration();
    let mut md = format!("### {name}\n\n| Property | Value |\n|---|---|\n");
    let mut summary = serde_json::Map::new();
    summary.insert(
        "container".into(),
        json!(fmt["format_long_name"]
            .as_str()
            .or(fmt["format_name"].as_str())),
    );
    summary.insert("duration".into(), json!(dur));
    summary.insert("duration_hms".into(), json!(dur.map(hms)));
    summary.insert("size".into(), json!(num(&fmt["size"]).map(|s| s as u64)));
    summary.insert("bitrate".into(), json!(pr.bitrate()));
    if let Some((w, h)) = pr.dims() {
        summary.insert("width".into(), json!(w));
        summary.insert("height".into(), json!(h));
    }
    summary.insert("fps".into(), json!(pr.fps()));
    summary.insert("video_codec".into(), json!(pr.video_codec()));
    summary.insert("audio_codec".into(), json!(pr.audio_codec()));
    summary.insert("rotation".into(), json!(pr.rotation()));
    for (k, v) in &summary {
        if !v.is_null() {
            let s = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            md.push_str(&format!("| {k} | {s} |\n"));
        }
    }
    md.push_str(
        "\n| # | Type | Codec | Details | Language | Bitrate |\n|---|---|---|---|---|---|\n",
    );
    let mut rows = vec![];
    for s in pr.streams() {
        let br = num(&s["bit_rate"])
            .map(|b| format!("{:.0} kb/s", b / 1000.0))
            .unwrap_or_default();
        let row = vec![
            json!(name),
            json!(s["index"]),
            json!(s["codec_type"].as_str().unwrap_or("")),
            json!(s["codec_long_name"]
                .as_str()
                .or(s["codec_name"].as_str())
                .unwrap_or("")),
            json!(stream_details(s)),
            json!(s["tags"]["language"].as_str().unwrap_or("")),
            json!(br),
        ];
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            s["index"],
            row[2].as_str().unwrap(),
            row[3].as_str().unwrap(),
            row[4].as_str().unwrap(),
            row[5].as_str().unwrap(),
            row[6].as_str().unwrap()
        ));
        rows.push(Value::Array(row));
    }
    (Value::Object(summary), rows, md)
}

fn info() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("video.info", "Video Info", Category::Video)
        .requires(FF)
        .describe("Inspect media files with ffprobe: container, duration, bitrate, every stream (codec, resolution, frame rate, audio layout, language), chapters and tags")
        .keywords(&["metadata", "ffprobe", "mediainfo", "codec", "resolution", "bitrate", "streams", "properties"])
        .param(ParamSpec::files("input").label("Media files").accept(MEDIA_ACCEPT).required().primary())
        .output(OutputKind::Data)
        .timeout(300);
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let mut all_rows = vec![];
        let mut md = String::new();
        let mut items = vec![];
        for f in &files {
            let pr = probe(&ctx, &f.path).await?;
            let (summary, rows, m) = media_summary(&pr, &f.name);
            all_rows.extend(rows);
            md.push_str(&m);
            let chapters: Vec<Value> = pr.raw["chapters"].as_array().cloned().unwrap_or_default().iter().map(|c| json!({"start": num(&c["start_time"]), "end": num(&c["end_time"]), "title": c["tags"]["title"]})).collect();
            if !chapters.is_empty() {
                md.push_str("\n**Chapters**\n\n");
                for c in &chapters {
                    md.push_str(&format!(
                        "- {} {}\n",
                        c["start"].as_f64().map(hms).unwrap_or_default(),
                        c["title"].as_str().unwrap_or("")
                    ));
                }
            }
            md.push('\n');
            items.push(json!({"file": f.name, "summary": summary, "format": pr.format(), "streams": pr.streams(), "chapters": chapters}));
        }
        let mut data = json!({"table": {"columns": ["File", "#", "Type", "Codec", "Details", "Language", "Bitrate"], "rows": all_rows}});
        if items.len() == 1 {
            let it = items.remove(0);
            for k in ["summary", "format", "streams", "chapters"] {
                data[k] = it[k].clone();
            }
        } else {
            data["files"] = Value::Array(items);
        }
        Ok(ToolOutput::text_as(md, "text/markdown").with_data(data))
    })
}

// ---------------------------------------------------------------------------
// video.preview

fn preview() -> Arc<dyn Tool> {
    let spec = video_spec("video.preview", "Video Preview Clip")
        .describe("Make a short preview / teaser made of N clips sampled evenly across each video")
        .keywords(&[
            "teaser",
            "trailer",
            "highlight",
            "summary",
            "sample",
            "hover preview",
        ])
        .param(video_input(true))
        .param(
            ParamSpec::integer("segments")
                .range(1.0, 30.0)
                .default_int(6),
        )
        .param(
            ParamSpec::number("segment_seconds")
                .range(0.5, 30.0)
                .default_num(2.0),
        )
        .param(
            ParamSpec::integer("width")
                .range(0.0, 3840.0)
                .default_int(640)
                .describe("Output width (0 = original)"),
        )
        .param(ParamSpec::boolean("audio").default_bool(true))
        .param(ParamSpec::choice("format", &[("mp4", "MP4"), ("webm", "WebM")]).default_str("mp4"))
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let fmt = p.str("format")?.to_string();
        let mut rep = Report::files(&["Segments"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let dur = require_dur(&pr, f)?;
            let n = p.int_or("segments", 6) as usize;
            let len = p.num_or("segment_seconds", 2.0);
            let (n, len) = if dur <= n as f64 * len {
                (1, dur)
            } else {
                (n, len)
            };
            let audio = p.bool("audio") && pr.audio().is_some();
            let w = p.int_or("width", 640);
            let mut ff = Ff::new();
            let mut fc = vec![];
            let mut labels = String::new();
            let starts: Vec<f64> = (0..n)
                .map(|k| {
                    if n == 1 {
                        0.0
                    } else {
                        ((dur - len) * (k as f64 + 0.5) / n as f64).max(0.0)
                    }
                })
                .collect();
            for (k, s) in starts.iter().enumerate() {
                ff = ff.args(["-ss", &secs(*s), "-t", &secs(len)]).input(&f.path);
                let scale = if w > 0 {
                    format!("scale={w}:-2:flags=lanczos,")
                } else {
                    "scale=trunc(iw/2)*2:trunc(ih/2)*2,".into()
                };
                fc.push(format!(
                    "[{k}:v:0]{scale}setsar=1,fps=30,format=yuv420p[v{k}]"
                ));
                labels.push_str(&format!("[v{k}]"));
                if audio {
                    fc.push(format!("[{k}:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo[a{k}]"));
                    labels.push_str(&format!("[a{k}]"));
                }
            }
            fc.push(format!(
                "{labels}concat=n={n}:v=1:a={}[v]{}",
                audio as u8,
                if audio { "[a]" } else { "" }
            ));
            let (vc, ac) = container_defaults(&fmt);
            let enc = video_encoder(
                &ctx,
                vc,
                &EncodeOpts {
                    hw: "none".into(),
                    ..EncodeOpts::from_params(&p)
                },
                &mut rep.warnings,
            )
            .await?;
            ff = ff.args(["-filter_complex", &fc.join(";"), "-map", "[v]"]);
            if audio {
                ff = ff.args(["-map", "[a]"]).args(ac.args(128));
            }
            let out = out_path(&ctx, f, "preview", &fmt);
            ff.args(&enc.args)
                .args(mux_args(&fmt, Some(vc)))
                .out(&out)
                .duration(Some(n as f64 * len))
                .span(
                    span(i, files.len()),
                    format!("Building preview of {}", f.name),
                )
                .run(&ctx)
                .await?;
            rep.file(&ctx, f, &out, vec![json!(n)], json!({"segments": starts}))?;
        }
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// video.hw_encoders

fn hw_encoders() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("video.hw_encoders", "Hardware Encoders", Category::Video)
        .requires(&["ffmpeg"])
        .describe("List GPU/hardware video encoders and decoders FFmpeg supports here (NVENC, Quick Sync, VA-API, VideoToolbox, AMF) and test which actually work")
        .keywords(&["gpu", "nvenc", "qsv", "vaapi", "videotoolbox", "amf", "hardware acceleration", "hwaccel", "cuda"])
        .param(ParamSpec::boolean("test").default_bool(true).describe("Run a tiny test encode for each compiled-in encoder"))
        .output(OutputKind::Data)
        .timeout(300);
    async_tool(spec, |ctx, p| async move {
        let caps = caps(&ctx).await?;
        let test = p.bool("test");
        let mut rows = vec![];
        let mut usable = vec![];
        for api in HwApi::ALL {
            for fam in ["h264", "hevc", "av1", "vp9", "mjpeg"] {
                let enc = api.encoder(fam);
                if !caps.enc(&enc) {
                    continue;
                }
                let works = if test {
                    Some(hw_encoder_works(&ctx, &enc).await)
                } else {
                    None
                };
                if works == Some(true) {
                    usable.push(enc.clone());
                }
                rows.push(json!([
                    enc,
                    fam,
                    api.label(),
                    "yes",
                    works
                        .map(|w| if w { "yes" } else { "no" })
                        .unwrap_or("not tested")
                ]));
            }
        }
        let text = if usable.is_empty() {
            if test {
                "No working hardware encoder was found; software encoders (libx264, libx265, libvpx, SVT-AV1…) will be used.".to_string()
            } else {
                format!(
                    "{} hardware encoders are compiled into FFmpeg (not tested).",
                    rows.len()
                )
            }
        } else {
            format!("Working hardware encoders: {}", usable.join(", "))
        };
        Ok(ToolOutput::text(text).with_data(json!({
            "usable": usable,
            "hwaccels": caps.hwaccels,
            "vaapi_device": vaapi_device().map(|d| d.to_string_lossy().to_string()),
            "table": {"columns": ["Encoder", "Codec", "API", "Compiled in", "Works"], "rows": rows},
        })))
    })
}

// ---------------------------------------------------------------------------
// video.watermark

fn watermark() -> Arc<dyn Tool> {
    let spec = video_spec("video.watermark", "Watermark Video")
        .describe("Overlay a logo image or a text watermark on videos with position, margin, size and opacity control")
        .keywords(&["logo", "overlay", "branding", "drawtext", "text overlay", "copyright"])
        .param(video_input(true))
        .param(ParamSpec::choice("kind", &[("image", "Image / logo"), ("text", "Text")]).default_str("text"))
        .param(ParamSpec::file("image").accept(&["image/*"]).show_if("kind", &["image"]))
        .param(ParamSpec::text("text").max_len(500).placeholder("© My Name").show_if("kind", &["text"]))
        .param(ParamSpec::choice("position", &[("bottom-right", "Bottom right"), ("bottom-left", "Bottom left"), ("top-right", "Top right"), ("top-left", "Top left"), ("center", "Center")]).default_str("bottom-right"))
        .param(ParamSpec::integer("margin").range(0.0, 2000.0).default_int(16))
        .param(ParamSpec::number("opacity").range(0.0, 1.0).step(0.05).default_num(0.8))
        .param(ParamSpec::number("size").range(1.0, 100.0).default_num(15.0).describe("Image width / text height as a percentage of the video width / height"))
        .param(ParamSpec::color("color").default_str("#ffffff").show_if("kind", &["text"]))
        .param(ParamSpec::boolean("box").default_bool(true).describe("Draw a translucent box behind the text").show_if("kind", &["text"]))
        .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        let kind = p.str("kind")?.to_string();
        if kind == "image" && p.opt_file("image").is_none() {
            return Err(ToolError::invalid("image", "choose a watermark image"));
        }
        if kind == "text" && p.text("text").trim().is_empty() {
            return Err(ToolError::invalid("text", "enter the watermark text"));
        }
        let files = p.files("input").to_vec();
        let mut rep = Report::files(&[]);
        let pos = p.str("position")?.to_string();
        let m = p.int_or("margin", 16);
        let op = p.num_or("opacity", 0.8);
        let size = p.num_or("size", 15.0);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let (vw, vh) = pr.dims().unwrap_or((1280, 720));
            let dir = ctx.temp_dir(&format!("wm-{i}"))?;
            let (xe, ye) = wm_pos(&pos, m, kind == "image");
            let re = reencode(&ctx, f, &pr, &p, true, &mut rep.warnings).await?;
            let out = out_path(&ctx, f, "watermarked", re.ext);
            let mut ff = Ff::new().input(&f.path);
            if kind == "image" {
                let img = p.file("image")?;
                let local = dir.join(format!(
                    "logo.{}",
                    img.extension()
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric())
                        .collect::<String>()
                ));
                std::fs::copy(&img.path, &local)?;
                let ww = even(vw as f64 * size / 100.0);
                ff = ff.input(&local).args(["-filter_complex", &format!("[1:v]scale={ww}:-1,format=rgba,colorchannelmixer=aa={op:.3}[wm];[0:v][wm]overlay={xe}:{ye}:format=auto,format=yuv420p[v]"), "-map", "[v]"]);
            } else {
                crate::ffmpeg::need_filter(
                    &ctx,
                    "drawtext",
                    "text watermarks (FFmpeg built with libfreetype)",
                )
                .await?;
                std::fs::write(dir.join("wm.txt"), p.text("text").trim())?;
                let fs = ((vh as f64 * size / 100.0) / 2.0).round().max(8.0) as i64;
                let mut dt = format!("drawtext=textfile=wm.txt:expansion=none:fontsize={fs}:fontcolor={}@{op:.3}:x={xe}:y={ye}", ff_color(p.str_or("color", "#ffffff")));
                if p.bool("box") {
                    dt.push_str(&format!(
                        ":box=1:boxcolor=black@{:.3}:boxborderw={}",
                        op * 0.5,
                        (fs / 3).max(2)
                    ));
                }
                ff =
                    ff.cwd(&dir)
                        .args(["-filter_complex", &format!("[0:v]{dt}[v]"), "-map", "[v]"]);
            }
            if pr.audio().is_some() {
                ff = ff.args(["-map", "0:a?"]);
            }
            ff.args(re.tail())
                .out(&out)
                .duration(pr.duration())
                .span(span(i, files.len()), format!("Watermarking {}", f.name))
                .run(&ctx)
                .await?;
            rep.file(&ctx, f, &out, vec![], json!({"kind": kind}))?;
        }
        Ok(rep.finish())
    })
}

fn wm_pos(pos: &str, m: i64, image: bool) -> (String, String) {
    let (w, h, ow, oh) = if image {
        ("W", "H", "w", "h")
    } else {
        ("w", "h", "tw", "th")
    };
    match pos {
        "top-left" => (format!("{m}"), format!("{m}")),
        "top-right" => (format!("{w}-{ow}-{m}"), format!("{m}")),
        "bottom-left" => (format!("{m}"), format!("{h}-{oh}-{m}")),
        "center" => (format!("({w}-{ow})/2"), format!("({h}-{oh})/2")),
        _ => (format!("{w}-{ow}-{m}"), format!("{h}-{oh}-{m}")),
    }
}

// ---------------------------------------------------------------------------
// video.contact_sheet

fn contact_sheet() -> Arc<dyn Tool> {
    let spec = video_spec("video.contact_sheet", "Video Contact Sheet")
        .describe(
            "Create a screenshot grid (contact sheet) of frames sampled evenly across each video",
        )
        .keywords(&[
            "screenshot grid",
            "thumbnails",
            "storyboard",
            "mosaic",
            "tile",
            "overview",
            "screen caps",
        ])
        .param(video_input(true))
        .param(
            ParamSpec::integer("columns")
                .range(1.0, 12.0)
                .default_int(4),
        )
        .param(ParamSpec::integer("rows").range(1.0, 20.0).default_int(4))
        .param(
            ParamSpec::integer("tile_width")
                .range(32.0, 1920.0)
                .default_int(320),
        )
        .param(
            ParamSpec::integer("padding")
                .range(0.0, 64.0)
                .default_int(4)
                .advanced(),
        )
        .param(
            ParamSpec::color("background")
                .default_str("#000000")
                .advanced(),
        )
        .param(
            ParamSpec::choice(
                "format",
                &[("jpg", "JPEG"), ("png", "PNG"), ("webp", "WebP")],
            )
            .default_str("jpg"),
        );
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let (c, r) = (p.int_or("columns", 4), p.int_or("rows", 4));
        let n = (c * r) as f64;
        let fmt = p.str("format")?.to_string();
        let pad = p.int_or("padding", 4);
        let mut rep = Report::new(&["Video", "Image", "Grid"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let dur = require_dur(&pr, f)?;
            let interval = dur / n;
            let vf = format!(
                "fps=1/{:.6},scale={}:-2:flags=lanczos,tile={c}x{r}:padding={pad}:margin={pad}:color={}",
                interval.max(0.001),
                p.int_or("tile_width", 320),
                ff_color(p.str_or("background", "#000000"))
            );
            let out = out_path(&ctx, f, "sheet", &fmt);
            Ff::new()
                .args(["-ss", &secs(interval / 2.0)])
                .input(&f.path)
                .args(["-an", "-sn", "-vf", &vf, "-frames:v", "1", "-update", "1"])
                .args(image_args(&fmt))
                .out(&out)
                .duration(Some(dur))
                .span(span(i, files.len()), format!("Sampling {}", f.name))
                .run(&ctx)
                .await?;
            let a = ctx.artifact(&out)?;
            rep.rows.push(vec![
                json!(f.name),
                json!(a.name),
                json!(format!("{c}x{r}")),
            ]);
            rep.items
                .push(json!({"input": f.name, "output": a.name, "interval": interval}));
            rep.artifacts.push(a);
        }
        Ok(rep.finish())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atempo() {
        assert_eq!(atempo_chain(2.0), "atempo=2.000000");
        assert_eq!(atempo_chain(4.0), "atempo=2.0,atempo=2.000000");
        assert_eq!(atempo_chain(0.25), "atempo=0.5,atempo=0.500000");
    }

    #[test]
    fn crop_parse() {
        assert_eq!(parse_crop("[Parsed_cropdetect_1 @ 0x] x1:0 x2:319 y1:30 y2:209 w:320 h:180 x:0 y:30 pts:1 t:0.04 limit:0.09 crop=320:180:0:30"), Some((320, 180, 0, 30)));
        assert_eq!(parse_crop("nothing"), None);
    }
}
