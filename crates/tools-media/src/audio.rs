//! Audio tools built on FFmpeg.

use crate::ffmpeg::*;
use crate::video::{extract_audio_tool, media_summary, speed_chain};
use dogubako_core::prelude::*;
use std::path::Path;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        convert(),
        compress(),
        trim(),
        split(),
        merge(),
        normalize(),
        volume(),
        sample_rate(),
        channels(),
        remove_silence(),
        detect_silence(),
        waveform(),
        spectrogram(),
        info(),
        tags(),
        denoise(),
        speed(),
        equalizer(),
        fade(),
        reverse(),
        extract_audio_tool("audio.extract_from_video", Category::Audio),
    ]
}

// ---------------------------------------------------------------------------
// Encoding helpers

const FORMATS: &[(&str, &str)] = &[
    ("mp3", "MP3"),
    ("m4a", "AAC (M4A)"),
    ("ogg", "Ogg Vorbis"),
    ("opus", "Opus"),
    ("flac", "FLAC (lossless)"),
    ("wav", "WAV (uncompressed)"),
    ("aiff", "AIFF"),
    ("alac", "Apple Lossless (M4A)"),
    ("wma", "WMA"),
    ("ac3", "AC-3 (Dolby Digital)"),
];

/// (extension, encoder, codec args) for a target format.
fn format_args(
    fmt: &str,
    kbps: u32,
    vbr: Option<i64>,
    depth: &str,
) -> (&'static str, &'static str, Vec<String>) {
    let br = |enc: &str| {
        vec![
            "-c:a".to_string(),
            enc.to_string(),
            "-b:a".to_string(),
            format!("{kbps}k"),
        ]
    };
    let pcm = |be: bool| {
        let e = if be { "be" } else { "le" };
        match depth {
            "24" => format!("pcm_s24{e}"),
            "32" => format!("pcm_s32{e}"),
            "32f" => format!("pcm_f32{e}"),
            _ => format!("pcm_s16{e}"),
        }
    };
    match fmt {
        "mp3" => match vbr {
            Some(q) => (
                "mp3",
                "libmp3lame",
                vec![
                    "-c:a".into(),
                    "libmp3lame".into(),
                    "-q:a".into(),
                    q.clamp(0, 9).to_string(),
                ],
            ),
            None => ("mp3", "libmp3lame", br("libmp3lame")),
        },
        "m4a" => ("m4a", "aac", br("aac")),
        "ogg" => match vbr {
            Some(q) => (
                "ogg",
                "libvorbis",
                vec![
                    "-c:a".into(),
                    "libvorbis".into(),
                    "-q:a".into(),
                    (10 - q.clamp(0, 9)).to_string(),
                ],
            ),
            None => ("ogg", "libvorbis", br("libvorbis")),
        },
        "opus" => (
            "opus",
            "libopus",
            vec![
                "-c:a".into(),
                "libopus".into(),
                "-b:a".into(),
                format!("{}k", kbps.min(256)),
                "-vbr".into(),
                "on".into(),
            ],
        ),
        "flac" => {
            let mut v = vec![
                "-c:a".to_string(),
                "flac".to_string(),
                "-compression_level".to_string(),
                "8".to_string(),
            ];
            match depth {
                "16" => v.extend(["-sample_fmt".into(), "s16".into()]),
                "24" => v.extend([
                    "-sample_fmt".into(),
                    "s32".into(),
                    "-bits_per_raw_sample".into(),
                    "24".into(),
                ]),
                "32" | "32f" => v.extend(["-sample_fmt".into(), "s32".into()]),
                _ => {}
            }
            ("flac", "flac", v)
        }
        "wav" => ("wav", "pcm_s16le", vec!["-c:a".into(), pcm(false)]),
        "aiff" => ("aiff", "pcm_s16be", vec!["-c:a".into(), pcm(true)]),
        "alac" => ("m4a", "alac", vec!["-c:a".into(), "alac".into()]),
        "wma" => ("wma", "wmav2", br("wmav2")),
        _ => ("ac3", "ac3", br("ac3")),
    }
}

fn depth_choice() -> ParamSpec {
    ParamSpec::choice(
        "bit_depth",
        &[
            ("keep", "Keep / default"),
            ("16", "16-bit"),
            ("24", "24-bit"),
            ("32", "32-bit integer"),
            ("32f", "32-bit float"),
        ],
    )
    .default_str("keep")
}

fn rate_choice(name: &str) -> ParamSpec {
    ParamSpec::choice(
        name,
        &[
            ("keep", "Keep"),
            ("8000", "8 kHz"),
            ("11025", "11.025 kHz"),
            ("16000", "16 kHz"),
            ("22050", "22.05 kHz"),
            ("24000", "24 kHz"),
            ("32000", "32 kHz"),
            ("44100", "44.1 kHz"),
            ("48000", "48 kHz"),
            ("88200", "88.2 kHz"),
            ("96000", "96 kHz"),
            ("192000", "192 kHz"),
        ],
    )
    .default_str("keep")
}

/// Map the first audio stream (and cover art for formats that support it).
fn audio_maps(pr: &Probe, ext: &str) -> Vec<String> {
    let mut v = vec!["-map".to_string(), "0:a:0".to_string()];
    if pr.has_cover() && pr.video().is_none() && matches!(ext, "mp3" | "m4a" | "flac") {
        v.extend([
            "-map".into(),
            "0:v:0".into(),
            "-c:v".into(),
            "copy".into(),
            "-disposition:v:0".into(),
            "attached_pic".into(),
        ]);
    } else {
        v.push("-vn".into());
    }
    v.extend([
        "-sn".into(),
        "-dn".into(),
        "-map_metadata".into(),
        "0".into(),
    ]);
    if ext == "mp3" {
        v.extend(["-id3v2_version".into(), "3".into()]);
    }
    if ext == "m4a" {
        v.extend(["-movflags".into(), "+faststart".into()]);
    }
    v
}

async fn need_encoder(ctx: &ToolContext, enc: &str) -> ToolResult<()> {
    if caps(ctx).await?.enc(enc) {
        Ok(())
    } else {
        Err(ToolError::unsupported(format!(
            "this FFmpeg build has no '{enc}' encoder"
        )))
    }
}

// ---------------------------------------------------------------------------
// audio.convert / audio.compress

fn convert() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.convert", "Convert Audio")
        .describe("Convert audio (or the soundtrack of videos) to MP3, AAC/M4A, Ogg Vorbis, Opus, FLAC, WAV, AIFF, ALAC, WMA or AC-3 with bitrate, VBR, sample rate, channel and bit-depth control")
        .keywords(&["mp3", "wav", "flac", "aac", "m4a", "ogg", "opus", "aiff", "alac", "wma", "transcode", "format"])
        .param(audio_input(true))
        .param(ParamSpec::choice("format", FORMATS).default_str("mp3"))
        .param(ParamSpec::integer("bitrate").label("Bitrate (kbit/s)").range(32.0, 640.0).default_int(192).show_if("format", &["mp3", "m4a", "ogg", "opus", "wma", "ac3"]))
        .param(ParamSpec::integer("vbr_quality").label("VBR quality (0 = best, 9 = smallest)").range(0.0, 9.0).describe("Use variable bitrate instead of a fixed bitrate (MP3 / Vorbis)").show_if("format", &["mp3", "ogg"]).advanced())
        .param(rate_choice("sample_rate").advanced())
        .param(ParamSpec::choice("channels", &[("keep", "Keep"), ("1", "Mono"), ("2", "Stereo")]).default_str("keep").advanced())
        .param(depth_choice().show_if("format", &["wav", "flac", "aiff"]).advanced());
    async_tool(spec, |ctx, p| async move {
        let fmt = p.str("format")?.to_string();
        let kbps = p.int_or("bitrate", 192) as u32;
        let (ext, enc, args) = format_args(
            &fmt,
            kbps,
            p.opt_int("vbr_quality"),
            p.str_or("bit_depth", "keep"),
        );
        need_encoder(&ctx, enc).await?;
        let files = p.files("input").to_vec();
        let mut rep = Report::files(&["Codec"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_audio(&ctx, f).await?;
            let out = out_path(&ctx, f, "converted", ext);
            let mut ff = Ff::new()
                .input(&f.path)
                .args(audio_maps(&pr, ext))
                .args(&args);
            if let Ok(sr) = p.str_or("sample_rate", "keep").parse::<u32>() {
                ff = ff.args(["-ar", &sr.to_string()]);
            } else if fmt == "opus" {
                // libopus only accepts 48/24/16/12/8 kHz; FFmpeg resamples automatically.
            }
            if let Ok(ch) = p.str_or("channels", "keep").parse::<u32>() {
                ff = ff.args(["-ac", &ch.to_string()]);
            } else if (fmt == "ac3" || fmt == "wma") && pr.channels().unwrap_or(2) > 6 {
                ff = ff.args(["-ac", "6"]);
            }
            ff.out(&out)
                .duration(pr.duration())
                .span(span(i, files.len()), format!("Converting {}", f.name))
                .run(&ctx)
                .await?;
            rep.file(&ctx, f, &out, vec![json!(enc)], json!({"encoder": enc}))?;
        }
        Ok(rep.finish())
    })
}

fn compress() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.compress", "Compress Audio")
        .describe("Make audio files smaller by re-encoding to an efficient codec at a lower bitrate, optionally in mono (ideal for voice)")
        .keywords(&["reduce size", "smaller", "shrink", "bitrate", "voice memo", "podcast"])
        .param(audio_input(true))
        .param(ParamSpec::choice("level", &[("light", "Light (160 kbit/s)"), ("balanced", "Balanced (128 kbit/s)"), ("strong", "Strong (96 kbit/s)"), ("voice", "Voice (48 kbit/s mono)")]).default_str("balanced"))
        .param(ParamSpec::choice("format", &[("mp3", "MP3"), ("m4a", "AAC (M4A)"), ("opus", "Opus (best quality per size)")]).default_str("mp3"))
        .param(ParamSpec::boolean("mono").describe("Downmix to mono (halves the size of stereo audio)"));
    async_tool(spec, |ctx, p| async move {
        let level = p.str("level")?;
        let kbps = match level {
            "light" => 160,
            "strong" => 96,
            "voice" => 48,
            _ => 128,
        };
        let mono = p.bool("mono") || level == "voice";
        let fmt = p.str("format")?.to_string();
        let kbps = if mono { kbps * 2 / 3 } else { kbps }.max(32);
        let (ext, enc, args) = format_args(&fmt, kbps, None, "keep");
        need_encoder(&ctx, enc).await?;
        let files = p.files("input").to_vec();
        let mut rep = Report::files(&["Bitrate", "Reduction"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_audio(&ctx, f).await?;
            let out = out_path(&ctx, f, "compressed", ext);
            let mut ff = Ff::new()
                .input(&f.path)
                .args(audio_maps(&pr, ext))
                .args(&args);
            if mono {
                ff = ff.args(["-ac", "1"]);
            }
            if level == "voice" && fmt != "opus" {
                ff = ff.args(["-ar", "32000"]);
            }
            ff.out(&out)
                .duration(pr.duration())
                .span(span(i, files.len()), format!("Compressing {}", f.name))
                .run(&ctx)
                .await?;
            let after = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0) as f64;
            let red = 100.0 - after * 100.0 / f.size().max(1) as f64;
            if red <= 0.0 {
                rep.warnings.push(format!(
                    "{}: the result is not smaller than the original",
                    f.name
                ));
            }
            rep.file(
                &ctx,
                f,
                &out,
                vec![
                    json!(format!("{kbps} kbit/s{}", if mono { " mono" } else { "" })),
                    json!(format!("{red:.1}%")),
                ],
                json!({"reduction_percent": (red * 10.0).round() / 10.0}),
            )?;
        }
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// audio.trim / audio.split

async fn write_audio_segment(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    start: f64,
    len: Option<f64>,
    af: Option<&str>,
    ext: &str,
    out: &Path,
    sp: (f32, f32),
) -> ToolResult<()> {
    let mut ff = Ff::new();
    if start > 0.0 {
        ff = ff.args(["-ss", &secs(start)]);
    }
    ff = ff.input(&f.path);
    if let Some(l) = len {
        ff = ff.args(["-t", &secs(l)]);
    }
    ff = ff.args(["-map", "0:a:0", "-vn", "-sn", "-map_metadata", "0"]);
    if let Some(af) = af {
        ff = ff.args(["-af", af]);
    }
    ff.args(audio_args_for_ext(ext, Some(pr)))
        .out(out)
        .duration(len.or(pr.duration().map(|d| d - start)))
        .span(sp, format!("Cutting {}", f.name))
        .run(ctx)
        .await?;
    Ok(())
}

/// Output extension for audio-only results.
fn audio_only_ext(f: &InputFile, pr: &Probe) -> String {
    let e = audio_out_ext(f, pr);
    match e.as_str() {
        "mp4" | "mov" | "mkv" | "webm" | "avi" => match pr.audio_codec() {
            Some("mp3") => "mp3".into(),
            Some("opus") => "opus".into(),
            Some("vorbis") => "ogg".into(),
            Some("flac") => "flac".into(),
            Some(c) if c.starts_with("pcm") => "wav".into(),
            _ => "m4a".into(),
        },
        _ => e,
    }
}

fn trim() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.trim", "Trim Audio")
        .describe("Cut audio to a start/end time or duration, with optional fade in and fade out")
        .keywords(&[
            "cut", "clip", "shorten", "ringtone", "excerpt", "start", "end",
        ])
        .param(audio_input(true))
        .param(ParamSpec::text("start").placeholder("0:10"))
        .param(ParamSpec::text("end").placeholder("1:30"))
        .param(ParamSpec::text("duration").placeholder("30"))
        .param(
            ParamSpec::number("fade_in")
                .range(0.0, 600.0)
                .default_num(0.0)
                .advanced(),
        )
        .param(
            ParamSpec::number("fade_out")
                .range(0.0, 600.0)
                .default_num(0.0)
                .advanced(),
        );
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let mut rep = Report::files(&["Range"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_audio(&ctx, f).await?;
            let dur = pr.duration();
            let start = opt_time(&p, "start")?.unwrap_or(0.0);
            if dur.map(|d| start >= d).unwrap_or(false) {
                return Err(ToolError::invalid(
                    "start",
                    "start is beyond the end of the audio",
                ));
            }
            let len = match (opt_time(&p, "end")?, opt_time(&p, "duration")?) {
                (Some(e), _) if e <= start => {
                    return Err(ToolError::invalid("end", "end must be after start"))
                }
                (Some(e), _) => Some(e - start),
                (None, Some(l)) if l > 0.0 => Some(l),
                (None, Some(_)) => {
                    return Err(ToolError::invalid("duration", "duration must be positive"))
                }
                (None, None) if start == 0.0 => {
                    return Err(ToolError::input("give a start, end or duration"))
                }
                (None, None) => None,
            };
            let total = len
                .or(dur.map(|d| d - start))
                .map(|l| dur.map(|d| l.min(d - start)).unwrap_or(l));
            let mut af = vec![];
            let fi = p.num_or("fade_in", 0.0);
            let fo = p.num_or("fade_out", 0.0);
            if fi > 0.0 {
                af.push(format!("afade=t=in:st=0:d={}", secs(fi)));
            }
            if fo > 0.0 {
                let t =
                    total.ok_or_else(|| ToolError::input("cannot fade out: unknown duration"))?;
                af.push(format!(
                    "afade=t=out:st={}:d={}",
                    secs((t - fo).max(0.0)),
                    secs(fo)
                ));
            }
            let ext = audio_only_ext(f, &pr);
            let out = out_path(&ctx, f, "trimmed", &ext);
            let afs = af.join(",");
            write_audio_segment(
                &ctx,
                f,
                &pr,
                start,
                len,
                if afs.is_empty() { None } else { Some(&afs) },
                &ext,
                &out,
                span(i, files.len()),
            )
            .await?;
            rep.file(
                &ctx,
                f,
                &out,
                vec![json!(format!(
                    "{} – {}",
                    hms(start),
                    hms(start + total.unwrap_or(0.0))
                ))],
                json!({"start": start, "duration": total}),
            )?;
        }
        Ok(rep.finish())
    })
}

/// Silence intervals from FFmpeg's silencedetect.
pub(crate) async fn detect_silences(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    noise_db: f64,
    min_dur: f64,
    sp: (f32, f32),
) -> ToolResult<Vec<(f64, f64)>> {
    let out = Ff::new()
        .input(&f.path)
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-sn",
            "-af",
            &format!("silencedetect=noise={noise_db}dB:d={min_dur}"),
        ])
        .null_out()
        .verbose()
        .duration(pr.duration())
        .span(sp, format!("Detecting silence in {}", f.name))
        .run(ctx)
        .await?;
    Ok(parse_silences(&out.log, pr.duration()))
}

pub(crate) fn parse_silences(log: &[String], dur: Option<f64>) -> Vec<(f64, f64)> {
    let mut v = vec![];
    let mut cur: Option<f64> = None;
    for l in log {
        if let Some(i) = l.find("silence_start:") {
            cur = l[i + 14..]
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok());
        } else if let Some(i) = l.find("silence_end:") {
            let e: Option<f64> = l[i + 12..]
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok());
            if let (Some(s), Some(e)) = (cur.take(), e) {
                v.push((s.max(0.0), e));
            }
        }
    }
    if let (Some(s), Some(d)) = (cur, dur) {
        if d > s {
            v.push((s.max(0.0), d));
        }
    }
    v
}

fn split() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.split", "Split Audio")
        .describe("Split audio into parts: fixed-length chunks, at given timestamps, or automatically at silences")
        .keywords(&["chunks", "segments", "cut into pieces", "tracks", "silence split", "chapters"])
        .param(audio_input(false))
        .param(ParamSpec::choice("mode", &[("duration", "Every N seconds"), ("timestamps", "At timestamps"), ("silence", "At silences")]).default_str("duration"))
        .param(ParamSpec::number("chunk_seconds").range(1.0, 86400.0).default_num(600.0).show_if("mode", &["duration"]))
        .param(ParamSpec::text("points").placeholder("3:00, 7:30").show_if("mode", &["timestamps"]))
        .param(ParamSpec::number("threshold_db").range(-100.0, 0.0).default_num(-40.0).show_if("mode", &["silence"]))
        .param(ParamSpec::number("min_silence").label("Minimum silence (s)").range(0.05, 60.0).default_num(1.0).show_if("mode", &["silence"]))
        .param(ParamSpec::number("min_part").label("Minimum part length (s)").range(0.0, 3600.0).default_num(5.0).show_if("mode", &["silence"]).advanced());
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let pr = probe_audio(&ctx, &f).await?;
        let dur = pr
            .duration()
            .ok_or_else(|| ToolError::input("could not determine the duration"))?;
        let mut points: Vec<f64> = match p.str("mode")? {
            "duration" => {
                let n = p.num("chunk_seconds")?;
                if dur / n > 2000.0 {
                    return Err(ToolError::invalid(
                        "chunk_seconds",
                        "this would create more than 2000 files",
                    ));
                }
                (1..)
                    .map(|k| k as f64 * n)
                    .take_while(|t| *t < dur - 0.05)
                    .collect()
            }
            "timestamps" => {
                let v = time_list(&p, "points")?;
                if v.is_empty() {
                    return Err(ToolError::invalid("points", "give at least one timestamp"));
                }
                v
            }
            _ => {
                let sil = detect_silences(
                    &ctx,
                    &f,
                    &pr,
                    p.num_or("threshold_db", -40.0),
                    p.num_or("min_silence", 1.0),
                    (0.0, 0.3),
                )
                .await?;
                let min_part = p.num_or("min_part", 5.0);
                let mut pts = vec![];
                let mut last = 0.0;
                for (s, e) in sil {
                    if s <= 0.01 || e >= dur - 0.01 {
                        continue;
                    }
                    let mid = (s + e) / 2.0;
                    if mid - last >= min_part && dur - mid >= min_part.min(dur / 2.0) {
                        pts.push(mid);
                        last = mid;
                    }
                }
                pts
            }
        };
        points.retain(|t| *t > 0.05 && *t < dur - 0.05);
        points.sort_by(|a, b| a.partial_cmp(b).unwrap());
        points.dedup_by(|a, b| (*a - *b).abs() < 0.05);
        let mut bounds = vec![0.0];
        bounds.extend(points);
        bounds.push(dur);
        let n = bounds.len() - 1;
        let ext = audio_only_ext(&f, &pr);
        let mut rep = Report::new(&["Part", "File", "Start", "End", "Length"]);
        if n == 1 {
            rep.warnings
                .push("no split points found — the output is a single part".into());
        }
        for k in 0..n {
            let (a, b) = (bounds[k], bounds[k + 1]);
            let out = out_path(&ctx, &f, &format!("part{:03}", k + 1), &ext);
            let sp = span(k, n);
            write_audio_segment(
                &ctx,
                &f,
                &pr,
                a,
                if k + 1 == n { None } else { Some(b - a) },
                None,
                &ext,
                &out,
                (0.3 + sp.0 * 0.7, 0.3 + sp.1 * 0.7),
            )
            .await?;
            let art = ctx.artifact(&out)?;
            rep.rows.push(vec![
                json!(k + 1),
                json!(art.name),
                json!(hms(a)),
                json!(hms(b)),
                json!(format!("{:.1} s", b - a)),
            ]);
            rep.items
                .push(json!({"part": k + 1, "output": art.name, "start": a, "end": b}));
            rep.artifacts.push(art);
        }
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// audio.merge

fn merge() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.merge", "Merge Audio")
        .describe(
            "Join audio files end to end, with optional crossfades or gaps of silence between them",
        )
        .keywords(&[
            "join",
            "concatenate",
            "combine",
            "crossfade",
            "mix tape",
            "append",
        ])
        .param(audio_input(true).describe("Files in playback order (at least two)"))
        .param(
            ParamSpec::number("crossfade")
                .label("Crossfade (s)")
                .range(0.0, 30.0)
                .default_num(0.0),
        )
        .param(
            ParamSpec::number("gap")
                .label("Silence between files (s)")
                .range(0.0, 60.0)
                .default_num(0.0)
                .advanced(),
        )
        .param(
            ParamSpec::choice(
                "format",
                &[
                    ("same", "Same as first file"),
                    ("mp3", "MP3"),
                    ("m4a", "AAC (M4A)"),
                    ("wav", "WAV"),
                    ("flac", "FLAC"),
                    ("opus", "Opus"),
                    ("ogg", "Ogg Vorbis"),
                ],
            )
            .default_str("same"),
        );
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        if files.len() < 2 {
            return Err(ToolError::invalid(
                "input",
                "give at least two files to merge",
            ));
        }
        let mut probes = vec![];
        for f in &files {
            probes.push(probe_audio(&ctx, f).await?);
        }
        let cf = p.num_or("crossfade", 0.0);
        let gap = p.num_or("gap", 0.0);
        if cf > 0.0 {
            for (f, pr) in files.iter().zip(&probes) {
                if pr.duration().map(|d| d <= cf).unwrap_or(false) {
                    return Err(ToolError::invalid(
                        "crossfade",
                        format!("'{}' is shorter than the crossfade", f.name),
                    ));
                }
            }
        }
        let sr = probes
            .iter()
            .filter_map(|p| p.sample_rate())
            .max()
            .unwrap_or(44100);
        let stereo = probes.iter().any(|p| p.channels().unwrap_or(2) > 1);
        let layout = if stereo { "stereo" } else { "mono" };
        let mut ff = Ff::new();
        let mut fc = vec![];
        for (i, f) in files.iter().enumerate() {
            ff = ff.input(&f.path);
            let pad = if gap > 0.0 && i + 1 < files.len() {
                format!(",apad=pad_dur={}", secs(gap))
            } else {
                String::new()
            };
            fc.push(format!("[{i}:a:0]aresample={sr},aformat=sample_fmts=fltp:sample_rates={sr}:channel_layouts={layout}{pad}[a{i}]"));
        }
        if cf > 0.0 {
            let mut prev = "a0".to_string();
            for i in 1..files.len() {
                let lab = if i + 1 == files.len() {
                    "out".to_string()
                } else {
                    format!("x{i}")
                };
                fc.push(format!(
                    "[{prev}][a{i}]acrossfade=d={}:c1=tri:c2=tri[{lab}]",
                    secs(cf)
                ));
                prev = lab;
            }
        } else {
            let labels: String = (0..files.len()).map(|i| format!("[a{i}]")).collect();
            fc.push(format!("{labels}concat=n={}:v=0:a=1[out]", files.len()));
        }
        let fmt = p.str("format")?;
        let ext = if fmt == "same" {
            audio_only_ext(&files[0], &probes[0])
        } else {
            fmt.to_string()
        };
        let total: f64 = probes.iter().filter_map(|p| p.duration()).sum::<f64>()
            + gap * (files.len() - 1) as f64
            - cf * (files.len() - 1) as f64;
        let out =
            ctx.output_path(&suffixed(&files[0].name, "merged", Some(&ext)).replace('%', "_"));
        ff.args(["-filter_complex", &fc.join(";"), "-map", "[out]"])
            .args(audio_args_for_ext(&ext, Some(&probes[0])))
            .out(&out)
            .duration(Some(total))
            .span((0.0, 1.0), "Merging")
            .run(&ctx)
            .await?;
        let rows: Vec<Value> = files
            .iter()
            .zip(&probes)
            .enumerate()
            .map(|(i, (f, pr))| json!([i + 1, f.name, pr.duration().map(hms).unwrap_or_default()]))
            .collect();
        Ok(ToolOutput::files(vec![ctx.artifact(&out)?]).with_data(json!({"duration": total, "table": {"columns": ["#", "File", "Duration"], "rows": rows}})))
    })
}

// ---------------------------------------------------------------------------
// audio.normalize

/// Extract the last JSON object printed by loudnorm.
pub(crate) fn parse_loudnorm(log: &[String]) -> Option<Value> {
    let end = log.iter().rposition(|l| l.trim() == "}")?;
    let start = log[..end].iter().rposition(|l| l.trim() == "{")?;
    serde_json::from_str(&log[start..=end].join("\n")).ok()
}

fn jnum(v: &Value, k: &str) -> Option<f64> {
    v[k].as_str()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|f| f.is_finite())
}

fn parse_db(log: &[String], key: &str) -> Option<f64> {
    log.iter().rev().find_map(|l| {
        let i = l.find(key)?;
        l[i + key.len()..]
            .trim()
            .trim_end_matches("dB")
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

fn normalize() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.normalize", "Normalize Audio")
        .describe("Normalize loudness with two-pass EBU R128 (loudnorm: measure, then apply to a target LUFS / true peak / LRA) or peak-normalize to a dBFS level")
        .keywords(&["loudness", "lufs", "ebu r128", "loudnorm", "volume leveling", "peak", "podcast", "spotify", "youtube"])
        .param(audio_input(true))
        .param(ParamSpec::choice("mode", &[("loudness", "Loudness (EBU R128)"), ("peak", "Peak")]).default_str("loudness"))
        .param(ParamSpec::choice("preset", &[("streaming", "Streaming (−14 LUFS)"), ("podcast", "Podcast (−16 LUFS)"), ("broadcast", "Broadcast EBU R128 (−23 LUFS)"), ("custom", "Custom")]).default_str("streaming").show_if("mode", &["loudness"]))
        .param(ParamSpec::number("target_lufs").range(-70.0, -5.0).default_num(-16.0).show_if("preset", &["custom"]))
        .param(ParamSpec::number("true_peak").label("True peak (dBTP)").range(-9.0, 0.0).default_num(-1.5).show_if("mode", &["loudness"]).advanced())
        .param(ParamSpec::number("lra").label("Loudness range (LU)").range(1.0, 50.0).default_num(11.0).show_if("mode", &["loudness"]).advanced())
        .param(ParamSpec::number("peak_db").label("Peak level (dBFS)").range(-30.0, 0.0).default_num(-1.0).show_if("mode", &["peak"]));
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let mode = p.str("mode")?.to_string();
        let mut rep = Report::files(&["Before", "After"]);
        for (i, f) in files.iter().enumerate() {
            let sp = span(i, files.len());
            let pr = probe_audio(&ctx, f).await?;
            let ext = audio_out_ext(f, &pr);
            let out = out_path(&ctx, f, "normalized", &ext);
            let sr = pr.sample_rate().unwrap_or(48000);
            if mode == "peak" {
                let log = Ff::new()
                    .input(&f.path)
                    .args(["-map", "0:a:0", "-vn", "-af", "volumedetect"])
                    .null_out()
                    .verbose()
                    .duration(pr.duration())
                    .span(sub(sp, 0.0, 0.4), format!("Measuring {}", f.name))
                    .run(&ctx)
                    .await?
                    .log;
                let max = parse_db(&log, "max_volume:")
                    .ok_or_else(|| ToolError::failed("could not measure the peak level"))?;
                if max <= -90.0 {
                    return Err(ToolError::input(format!("'{}' is silent", f.name)));
                }
                let target = p.num_or("peak_db", -1.0);
                let gain = target - max;
                filter_audio_file(
                    &ctx,
                    f,
                    &pr,
                    &format!("volume={gain:.2}dB"),
                    &ext,
                    &out,
                    pr.duration(),
                    sub(sp, 0.4, 1.0),
                )
                .await?;
                rep.file(
                    &ctx,
                    f,
                    &out,
                    vec![
                        json!(format!("peak {max:.1} dBFS")),
                        json!(format!("peak {target:.1} dBFS ({gain:+.1} dB)")),
                    ],
                    json!({"peak_before": max, "gain_db": gain, "peak_after": target}),
                )?;
                continue;
            }
            let i_target = match p.str_or("preset", "streaming") {
                "podcast" => -16.0,
                "broadcast" => -23.0,
                "custom" => p.num_or("target_lufs", -16.0),
                _ => -14.0,
            };
            let tp = p.num_or("true_peak", -1.5);
            let lra = p.num_or("lra", 11.0);
            let log = Ff::new()
                .input(&f.path)
                .args([
                    "-map",
                    "0:a:0",
                    "-vn",
                    "-af",
                    &format!("loudnorm=I={i_target}:TP={tp}:LRA={lra}:print_format=json"),
                ])
                .null_out()
                .verbose()
                .duration(pr.duration())
                .span(
                    sub(sp, 0.0, 0.45),
                    format!("Measuring loudness of {}", f.name),
                )
                .run(&ctx)
                .await?
                .log;
            let m = parse_loudnorm(&log)
                .ok_or_else(|| ToolError::failed("could not parse loudnorm measurement"))?;
            let (Some(mi), Some(mtp), Some(mlra), Some(mth), Some(off)) = (
                jnum(&m, "input_i"),
                jnum(&m, "input_tp"),
                jnum(&m, "input_lra"),
                jnum(&m, "input_thresh"),
                jnum(&m, "target_offset"),
            ) else {
                return Err(ToolError::input(format!(
                    "'{}' is silent or too short to measure loudness",
                    f.name
                )));
            };
            let af = format!("loudnorm=I={i_target}:TP={tp}:LRA={lra}:measured_I={mi}:measured_TP={mtp}:measured_LRA={mlra}:measured_thresh={mth}:offset={off}:linear=true:print_format=json,aresample={sr}");
            let same = ext == f.extension();
            let mut ff = Ff::new().input(&f.path);
            if same
                && pr.streams().iter().any(|s| s["codec_type"] == "video")
                && ext != "ogg"
                && ext != "opus"
            {
                ff = ff.args(["-map", "0:v?", "-c:v", "copy"]);
            } else {
                ff = ff.a("-vn");
            }
            let log2 = ff
                .args(["-map", "0:a:0", "-map_metadata", "0", "-af", &af])
                .args(audio_args_for_ext(&ext, Some(&pr)))
                .out(&out)
                .verbose()
                .duration(pr.duration())
                .span(sub(sp, 0.45, 1.0), format!("Normalizing {}", f.name))
                .run(&ctx)
                .await?
                .log;
            let r = parse_loudnorm(&log2).unwrap_or(Value::Null);
            let oi = jnum(&r, "output_i");
            let ntype = r["normalization_type"].as_str().unwrap_or("").to_string();
            rep.file(
                &ctx,
                f,
                &out,
                vec![json!(format!("{mi:.1} LUFS, {mtp:.1} dBTP, LRA {mlra:.1}")), json!(format!("{} LUFS{}", oi.map(|v| format!("{v:.1}")).unwrap_or_else(|| "?".into()), if ntype.is_empty() { String::new() } else { format!(" ({ntype})") }))],
                json!({"measured": {"integrated": mi, "true_peak": mtp, "lra": mlra, "threshold": mth}, "target": {"integrated": i_target, "true_peak": tp, "lra": lra}, "output": {"integrated": oi, "true_peak": jnum(&r, "output_tp"), "lra": jnum(&r, "output_lra")}, "normalization_type": ntype}),
            )?;
        }
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// Simple filter tools

/// Batch runner for "apply this audio filter" tools.
async fn af_batch<F>(
    ctx: &ToolContext,
    p: &Params,
    suffix: &str,
    label: &str,
    build: F,
) -> ToolResult<ToolOutput>
where
    F: Fn(&InputFile, &Probe) -> ToolResult<(String, Option<f64>, Value)>,
{
    let files = p.files("input").to_vec();
    let mut rep = Report::files(&[]);
    for (i, f) in files.iter().enumerate() {
        let pr = probe_audio(ctx, f).await?;
        let (af, dur, item) = build(f, &pr)?;
        let ext = audio_out_ext(f, &pr);
        let out = out_path(ctx, f, suffix, &ext);
        filter_audio_file(
            ctx,
            f,
            &pr,
            &af,
            &ext,
            &out,
            dur.or(pr.duration()),
            span(i, files.len()),
        )
        .await?;
        let _ = label;
        rep.file(ctx, f, &out, vec![], item)?;
    }
    Ok(rep.finish())
}

fn volume() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.volume", "Change Volume")
        .describe("Make audio louder or quieter by decibels or a factor, with an optional limiter to prevent clipping")
        .keywords(&["louder", "quieter", "gain", "amplify", "boost", "db", "attenuate"])
        .param(audio_input(true))
        .param(ParamSpec::choice("unit", &[("db", "Decibels"), ("factor", "Factor")]).default_str("db"))
        .param(ParamSpec::number("amount").range(-60.0, 60.0).default_num(6.0).describe("dB (e.g. +6, −3) or factor (e.g. 1.5, 0.5)"))
        .param(ParamSpec::boolean("limiter").default_bool(true).describe("Prevent clipping with a limiter at −1 dBFS"));
    async_tool(spec, |ctx, p| async move {
        let unit = p.str("unit")?.to_string();
        let amt = p.num("amount")?;
        if unit == "factor" && amt <= 0.0 {
            return Err(ToolError::invalid(
                "amount",
                "factor must be greater than 0",
            ));
        }
        let mut af = if unit == "db" {
            format!("volume={amt}dB")
        } else {
            format!("volume={amt}")
        };
        let boosting = if unit == "db" { amt > 0.0 } else { amt > 1.0 };
        if p.bool("limiter") && boosting {
            af.push_str(",alimiter=limit=0.891:level=0");
        }
        af_batch(&ctx, &p, "volume", "Adjusting", |_, _| {
            Ok((af.clone(), None, json!({"filter": af})))
        })
        .await
    })
}

fn sample_rate() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.sample_rate", "Change Sample Rate")
        .describe("Resample audio to another sample rate (high-quality SoX resampler when available) and change the bit depth of WAV/FLAC/AIFF")
        .keywords(&["resample", "44.1khz", "48khz", "96khz", "bit depth", "16-bit", "24-bit", "hz"])
        .param(audio_input(true))
        .param(rate_choice("rate").default_str("48000"))
        .param(depth_choice());
    async_tool(spec, |ctx, p| async move {
        let soxr = ffmpeg_has_soxr(&ctx).await;
        let rate = p.str("rate")?.parse::<u32>().ok();
        let depth = p.str_or("bit_depth", "keep").to_string();
        if rate.is_none() && depth == "keep" {
            return Err(ToolError::input("choose a sample rate or bit depth"));
        }
        let files = p.files("input").to_vec();
        let mut rep = Report::files(&["Format"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_audio(&ctx, f).await?;
            let ext = audio_only_ext(f, &pr);
            let out = out_path(
                &ctx,
                f,
                &rate
                    .map(|r| format!("{r}hz"))
                    .unwrap_or_else(|| format!("{depth}bit")),
                &ext,
            );
            let mut args = audio_args_for_ext(&ext, Some(&pr));
            if depth != "keep" {
                match ext.as_str() {
                    "wav" | "aiff" | "aif" | "flac" => {
                        let fmt = if ext == "flac" {
                            "flac"
                        } else if ext == "wav" {
                            "wav"
                        } else {
                            "aiff"
                        };
                        args = format_args(fmt, 0, None, &depth).2;
                    }
                    _ => rep.warnings.push(format!(
                        "{}: bit depth only applies to WAV, FLAC and AIFF",
                        f.name
                    )),
                }
            }
            let mut af = format!("aresample={}", rate.or(pr.sample_rate()).unwrap_or(48000));
            if soxr {
                af.push_str(":resampler=soxr:precision=28");
            }
            if depth == "16" {
                af.push_str(":dither_method=triangular");
            }
            let mut ff = Ff::new()
                .input(&f.path)
                .args(["-map", "0:a:0", "-vn", "-map_metadata", "0", "-af", &af])
                .args(args);
            if let Some(r) = rate {
                ff = ff.args(["-ar", &r.to_string()]);
            }
            ff.out(&out)
                .duration(pr.duration())
                .span(span(i, files.len()), format!("Resampling {}", f.name))
                .run(&ctx)
                .await?;
            let opr = probe(&ctx, &out).await?;
            let desc = format!(
                "{} Hz {}",
                opr.sample_rate().unwrap_or(0),
                opr.audio()
                    .and_then(|a| a["sample_fmt"].as_str())
                    .unwrap_or("")
            );
            rep.file(
                &ctx,
                f,
                &out,
                vec![json!(desc)],
                json!({"sample_rate": opr.sample_rate(), "soxr": soxr}),
            )?;
        }
        Ok(rep.finish())
    })
}

async fn ffmpeg_has_soxr(ctx: &ToolContext) -> bool {
    match ctx.command("ffmpeg") {
        Ok(c) => c
            .args(["-hide_banner", "-version"])
            .run()
            .await
            .map(|o| o.stdout_text().contains("--enable-libsoxr"))
            .unwrap_or(false),
        Err(_) => false,
    }
}

fn channels() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.channels", "Audio Channels")
        .describe("Convert to mono or stereo, downmix surround to stereo, extract the left or right channel, or swap channels")
        .keywords(&["mono", "stereo", "downmix", "left channel", "right channel", "swap", "5.1", "surround"])
        .param(audio_input(true))
        .param(
            ParamSpec::choice("operation", &[("mono", "Mix to mono"), ("stereo", "Make stereo"), ("downmix", "Downmix surround to stereo"), ("left", "Left channel only (mono)"), ("right", "Right channel only (mono)"), ("swap", "Swap left/right")])
                .default_str("mono"),
        );
    async_tool(spec, |ctx, p| async move {
        let op = p.str("operation")?.to_string();
        af_batch(&ctx, &p, &op.clone(), "Remixing", |f, pr| {
            let ch = pr.channels().unwrap_or(2);
            if matches!(op.as_str(), "left" | "right" | "swap") && ch < 2 {
                return Err(ToolError::input(format!("'{}' is mono", f.name)));
            }
            let af = match op.as_str() {
                "mono" => "pan=mono|c0=0.5*FL+0.5*FR".to_string(),
                "stereo" => "aformat=channel_layouts=stereo".to_string(),
                "downmix" => {
                    "aresample=matrix_encoding=none,aformat=channel_layouts=stereo".to_string()
                }
                "left" => "pan=mono|c0=FL".to_string(),
                "right" => "pan=mono|c0=FR".to_string(),
                _ => "pan=stereo|c0=c1|c1=c0".to_string(),
            };
            let af = if op == "mono" && ch != 2 {
                "aformat=channel_layouts=mono".to_string()
            } else {
                af
            };
            Ok((af, None, json!({"operation": op, "channels_before": ch})))
        })
        .await
    })
}

fn remove_silence() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.remove_silence", "Remove Silence")
        .describe("Trim silence from the start and end, or shorten every pause longer than a minimum duration")
        .keywords(&["silence", "pauses", "dead air", "trim silence", "tighten", "silenceremove"])
        .param(audio_input(true))
        .param(ParamSpec::choice("mode", &[("edges", "Start and end only"), ("all", "All pauses")]).default_str("edges"))
        .param(ParamSpec::number("threshold_db").range(-100.0, 0.0).default_num(-50.0).describe("Audio below this level counts as silence"))
        .param(ParamSpec::number("min_silence").label("Minimum pause (s)").range(0.05, 60.0).default_num(0.5).show_if("mode", &["all"]))
        .param(ParamSpec::number("keep_silence").label("Pause to keep (s)").range(0.0, 10.0).default_num(0.25).show_if("mode", &["all"]));
    async_tool(spec, |ctx, p| async move {
        let t = p.num_or("threshold_db", -50.0);
        let af = if p.str("mode")? == "edges" {
            format!("silenceremove=start_periods=1:start_threshold={t}dB:start_silence=0.05,areverse,silenceremove=start_periods=1:start_threshold={t}dB:start_silence=0.05,areverse")
        } else {
            let min = p.num_or("min_silence", 0.5);
            let keep = p.num_or("keep_silence", 0.25).min(min);
            format!("silenceremove=start_periods=1:start_threshold={t}dB:start_silence={keep}:stop_periods=-1:stop_duration={min}:stop_threshold={t}dB:stop_silence={keep}")
        };
        let mut out = af_batch(&ctx, &p, "nosilence", "Removing silence", |_, _| {
            Ok((af.clone(), None, json!({})))
        })
        .await?;
        // Report new durations.
        let mut durs = vec![];
        for (k, a) in out.artifacts.iter().enumerate() {
            let before = match p.files("input").get(k) {
                Some(f) => probe(&ctx, &f.path).await.ok().and_then(|p| p.duration()),
                None => None,
            };
            let after = probe(&ctx, &a.path).await.ok().and_then(|p| p.duration());
            durs.push((before, after));
        }
        for (k, (before, after)) in durs.into_iter().enumerate() {
            if let Some(Value::Array(r)) = out.data["table"]["rows"].get_mut(k) {
                r.push(json!(before.map(hms)));
                r.push(json!(after.map(hms)));
            }
            out.data["files"][k]["duration_before"] = json!(before);
            out.data["files"][k]["duration_after"] = json!(after);
        }
        if let Some(c) = out.data["table"]["columns"].as_array_mut() {
            c.push(json!("Duration before"));
            c.push(json!("Duration after"));
        }
        Ok(out)
    })
}

fn detect_silence() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("audio.detect_silence", "Detect Silence", Category::Audio)
        .requires(FF)
        .progress()
        .timeout(3 * 3600)
        .output(OutputKind::Data)
        .describe("Find silent intervals in audio or video (FFmpeg silencedetect) and list their start, end and duration")
        .keywords(&["silence", "pauses", "gaps", "silencedetect", "quiet parts", "dead air"])
        .param(audio_input(false))
        .param(ParamSpec::number("threshold_db").range(-100.0, 0.0).default_num(-50.0))
        .param(ParamSpec::number("min_duration").label("Minimum duration (s)").range(0.01, 600.0).default_num(0.5));
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let pr = probe_audio(&ctx, &f).await?;
        let sil = detect_silences(
            &ctx,
            &f,
            &pr,
            p.num_or("threshold_db", -50.0),
            p.num_or("min_duration", 0.5),
            (0.0, 1.0),
        )
        .await?;
        let total: f64 = sil.iter().map(|(s, e)| e - s).sum();
        let rows: Vec<Value> = sil
            .iter()
            .enumerate()
            .map(|(i, (s, e))| json!([i + 1, hms(*s), hms(*e), format!("{:.3}", e - s)]))
            .collect();
        let items: Vec<Value> = sil
            .iter()
            .map(|(s, e)| json!({"start": s, "end": e, "duration": e - s}))
            .collect();
        let dur = pr.duration();
        let text = format!(
            "{} silent interval(s), {:.1} s of silence{}",
            sil.len(),
            total,
            dur.map(|d| format!(" ({:.1}% of {})", total * 100.0 / d.max(0.001), hms(d)))
                .unwrap_or_default()
        );
        Ok(ToolOutput::text(text).with_data(json!({
            "silences": items,
            "count": sil.len(),
            "total_silence": total,
            "duration": dur,
            "table": {"columns": ["#", "Start", "End", "Duration (s)"], "rows": rows},
        })))
    })
}

// ---------------------------------------------------------------------------
// Visualizations

fn waveform() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.waveform", "Audio Waveform")
        .describe("Render a waveform image (PNG) of audio with custom size, colors and scale, and optionally export min/max peak data as JSON")
        .keywords(&["waveform", "visualize", "peaks", "soundwave", "image", "showwavespic", "podcast art"])
        .param(audio_input(true))
        .param(ParamSpec::integer("width").range(64.0, 8000.0).default_int(1800))
        .param(ParamSpec::integer("height").range(32.0, 4000.0).default_int(280))
        .param(ParamSpec::color("color").default_str("#3b82f6"))
        .param(ParamSpec::color("background").default_str("#ffffff"))
        .param(ParamSpec::boolean("transparent").describe("Transparent background instead of a color"))
        .param(ParamSpec::boolean("split_channels").describe("Draw each channel separately"))
        .param(ParamSpec::choice("scale", &[("lin", "Linear"), ("log", "Logarithmic"), ("sqrt", "Square root"), ("cbrt", "Cube root")]).default_str("lin").advanced())
        .param(ParamSpec::boolean("peaks").describe("Also export peak data (JSON) for web players"))
        .param(ParamSpec::integer("points").range(10.0, 100000.0).default_int(1000).show_if_bool("peaks", true).advanced());
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let (w, h) = (p.int_or("width", 1800), p.int_or("height", 280));
        let mut artifacts = vec![];
        let mut items = vec![];
        for (i, f) in files.iter().enumerate() {
            let sp = span(i, files.len());
            let pr = probe_audio(&ctx, f).await?;
            let wave = format!(
                "[0:a:0]showwavespic=s={w}x{h}:split_channels={}:colors={}:scale={}:draw=full:filter=peak",
                p.bool("split_channels") as u8,
                ff_color(p.str_or("color", "#3b82f6")),
                p.str_or("scale", "lin")
            );
            let fc = if p.bool("transparent") {
                format!("{wave}[out]")
            } else {
                format!("{wave}[w];color=c={}:s={w}x{h}[b];[b][w]overlay=format=auto:shortest=1,format=rgb24[out]", ff_color(p.str_or("background", "#ffffff")))
            };
            let out = out_path(&ctx, f, "waveform", "png");
            Ff::new()
                .input(&f.path)
                .args([
                    "-filter_complex",
                    &fc,
                    "-map",
                    "[out]",
                    "-frames:v",
                    "1",
                    "-update",
                    "1",
                ])
                .out(&out)
                .duration(pr.duration())
                .span(sub(sp, 0.0, 0.6), format!("Drawing {}", f.name))
                .run(&ctx)
                .await?;
            artifacts.push(ctx.artifact(&out)?);
            let mut item = json!({"input": f.name, "image": artifacts.last().unwrap().name});
            if p.bool("peaks") {
                let n = p.int_or("points", 1000) as usize;
                let dur = pr.duration().unwrap_or(0.0);
                // Keep the decoded PCM under ~64 MB.
                let rate = ((32_000_000.0 / dur.max(1.0)) as u32).clamp(100, 8000);
                let pcm = decode_pcm(&ctx, &f.path, rate, None, None).await?;
                let peaks = compute_peaks(&pcm, n);
                let json = json!({"version": 1, "sample_rate": rate, "duration": dur, "points": peaks.len(), "data": peaks.iter().flat_map(|(a, b)| [round3(*a), round3(*b)]).collect::<Vec<_>>()});
                artifacts.push(ctx.write_output(
                    &format!("{}-peaks.json", f.stem().replace('%', "_")),
                    serde_json::to_string(&json)?.as_bytes(),
                )?);
                if peaks.len() <= 2000 {
                    item["peaks"] = json["data"].clone();
                }
                ctx.progress(sp.1, "");
            }
            items.push(item);
        }
        Ok(ToolOutput::files(artifacts).with_data(json!({"files": items})))
    })
}

fn round3(v: f32) -> f64 {
    (v as f64 * 1000.0).round() / 1000.0
}

/// Decode audio to mono signed 16-bit PCM at `rate` Hz.
pub(crate) async fn decode_pcm(
    ctx: &ToolContext,
    path: &Path,
    rate: u32,
    start: Option<f64>,
    len: Option<f64>,
) -> ToolResult<Vec<i16>> {
    let mut cmd = ctx
        .command("ffmpeg")?
        .args(["-hide_banner", "-nostdin", "-loglevel", "error"]);
    if let Some(s) = start {
        cmd = cmd.args(["-ss", &secs(s)]);
    }
    cmd = cmd.arg("-i").arg(arg_path(path));
    if let Some(l) = len {
        cmd = cmd.args(["-t", &secs(l)]);
    }
    let out = cmd
        .args([
            "-map",
            "0:a:0",
            "-vn",
            "-sn",
            "-ac",
            "1",
            "-ar",
            &rate.to_string(),
            "-f",
            "s16le",
            "-c:a",
            "pcm_s16le",
            "pipe:1",
        ])
        .run()
        .await?;
    Ok(out
        .stdout
        .chunks_exact(2)
        .map(|c| i16::from_le_bytes([c[0], c[1]]))
        .collect())
}

/// Min/max peaks (normalized to -1..1) over `n` buckets.
pub fn compute_peaks(pcm: &[i16], n: usize) -> Vec<(f32, f32)> {
    if pcm.is_empty() || n == 0 {
        return vec![];
    }
    let n = n.min(pcm.len());
    (0..n)
        .map(|k| {
            let a = k * pcm.len() / n;
            let b = ((k + 1) * pcm.len() / n).max(a + 1);
            let (mut lo, mut hi) = (i16::MAX, i16::MIN);
            for &s in &pcm[a..b] {
                lo = lo.min(s);
                hi = hi.max(s);
            }
            (lo as f32 / 32768.0, hi as f32 / 32767.0)
        })
        .collect()
}

fn spectrogram() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.spectrogram", "Audio Spectrogram")
        .describe("Render a spectrogram image (frequency over time) of audio with color scheme, scale and legend options")
        .keywords(&["spectrum", "frequency", "fft", "visualize", "showspectrumpic", "analysis"])
        .param(audio_input(true))
        .param(ParamSpec::integer("width").range(64.0, 8000.0).default_int(1024))
        .param(ParamSpec::integer("height").range(64.0, 4000.0).default_int(512))
        .param(ParamSpec::choice_of("color", &["intensity", "rainbow", "magma", "viridis", "plasma", "cividis", "fire", "cool", "green", "channel", "moreland", "nebulae", "terrain"]).default_str("intensity"))
        .param(ParamSpec::choice("mode", &[("combined", "All channels combined"), ("separate", "Channels separately")]).default_str("combined"))
        .param(ParamSpec::choice("scale", &[("log", "Logarithmic"), ("sqrt", "Square root"), ("cbrt", "Cube root"), ("lin", "Linear")]).default_str("log").advanced())
        .param(ParamSpec::choice("frequency_scale", &[("lin", "Linear"), ("log", "Logarithmic")]).default_str("lin").advanced())
        .param(ParamSpec::boolean("legend").default_bool(true));
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let mut rep = Report::new(&["Input", "Image"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_audio(&ctx, f).await?;
            let vf = format!(
                "[0:a:0]showspectrumpic=s={}x{}:mode={}:color={}:scale={}:fscale={}:legend={}[out]",
                p.int_or("width", 1024),
                p.int_or("height", 512),
                p.str_or("mode", "combined"),
                p.str_or("color", "intensity"),
                p.str_or("scale", "log"),
                p.str_or("frequency_scale", "lin"),
                p.bool("legend") as u8
            );
            let out = out_path(&ctx, f, "spectrogram", "png");
            Ff::new()
                .input(&f.path)
                .args([
                    "-filter_complex",
                    &vf,
                    "-map",
                    "[out]",
                    "-frames:v",
                    "1",
                    "-update",
                    "1",
                ])
                .out(&out)
                .duration(pr.duration())
                .span(span(i, files.len()), format!("Analyzing {}", f.name))
                .run(&ctx)
                .await?;
            let a = ctx.artifact(&out)?;
            rep.rows.push(vec![json!(f.name), json!(a.name)]);
            rep.items.push(json!({"input": f.name, "image": a.name}));
            rep.artifacts.push(a);
        }
        Ok(rep.finish())
    })
}

// ---------------------------------------------------------------------------
// audio.info / audio.tags

fn info() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("audio.info", "Audio Info", Category::Audio)
        .requires(FF)
        .progress()
        .timeout(3600)
        .output(OutputKind::Data)
        .describe("Show audio format, codec, bitrate, sample rate, channels and tags; optionally measure loudness (EBU R128 integrated LUFS, LRA, true peak)")
        .keywords(&["metadata", "tags", "bitrate", "sample rate", "lufs", "loudness", "ffprobe", "properties"])
        .param(ParamSpec::files("input").label("Audio files").accept(MEDIA_ACCEPT).required().primary())
        .param(ParamSpec::boolean("loudness").describe("Measure loudness (decodes the whole file)"));
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let mut md = String::new();
        let mut rows = vec![];
        let mut items = vec![];
        for (i, f) in files.iter().enumerate() {
            let pr = probe_audio(&ctx, f).await?;
            let (mut summary, r, m) = media_summary(&pr, &f.name);
            rows.extend(r);
            md.push_str(&m);
            let tags = collect_tags(&pr);
            if !tags.is_empty() {
                md.push_str("\n| Tag | Value |\n|---|---|\n");
                for (k, v) in &tags {
                    md.push_str(&format!(
                        "| {k} | {} |\n",
                        v.as_str().unwrap_or("").replace('|', "\\|")
                    ));
                }
            }
            summary["sample_rate"] = json!(pr.sample_rate());
            summary["channels"] = json!(pr.channels());
            summary["audio_bitrate"] = json!(pr.audio_bitrate());
            if p.bool("loudness") {
                let log = Ff::new()
                    .input(&f.path)
                    .args([
                        "-map",
                        "0:a:0",
                        "-vn",
                        "-af",
                        "ebur128=peak=true:framelog=quiet",
                    ])
                    .null_out()
                    .verbose()
                    .duration(pr.duration())
                    .span(span(i, files.len()), format!("Measuring {}", f.name))
                    .run(&ctx)
                    .await?
                    .log;
                let l = parse_ebur128(&log);
                md.push_str(&format!(
                    "\n**Loudness:** {} LUFS integrated, LRA {} LU, true peak {} dBFS\n",
                    fmt_opt(l.0),
                    fmt_opt(l.1),
                    fmt_opt(l.2)
                ));
                summary["loudness"] =
                    json!({"integrated_lufs": l.0, "lra": l.1, "true_peak_dbfs": l.2});
            }
            md.push('\n');
            items.push(json!({"file": f.name, "summary": summary, "tags": Value::Object(tags), "streams": pr.streams(), "format": pr.format()}));
        }
        let mut data = json!({"table": {"columns": ["File", "#", "Type", "Codec", "Details", "Language", "Bitrate"], "rows": rows}});
        if items.len() == 1 {
            let it = items.remove(0);
            for k in ["summary", "tags", "streams", "format"] {
                data[k] = it[k].clone();
            }
        } else {
            data["files"] = Value::Array(items);
        }
        Ok(ToolOutput::text_as(md, "text/markdown").with_data(data))
    })
}

fn fmt_opt(v: Option<f64>) -> String {
    v.map(|v| format!("{v:.1}")).unwrap_or_else(|| "?".into())
}

/// (integrated LUFS, LRA, true peak) from the ebur128 summary.
pub(crate) fn parse_ebur128(log: &[String]) -> (Option<f64>, Option<f64>, Option<f64>) {
    let start = log
        .iter()
        .rposition(|l| l.contains("Summary:"))
        .unwrap_or(0);
    let mut r = (None, None, None);
    let mut section = "";
    for l in &log[start..] {
        let t = l.trim();
        if t.starts_with("Integrated loudness") {
            section = "i";
        } else if t.starts_with("Loudness range") {
            section = "lra";
        } else if t.starts_with("True peak") {
            section = "tp";
        }
        let val = |pfx: &str| {
            t.strip_prefix(pfx)
                .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
        };
        match section {
            "i" if t.starts_with("I:") => r.0 = val("I:"),
            "lra" if t.starts_with("LRA:") => r.1 = val("LRA:"),
            "tp" if t.starts_with("Peak:") => r.2 = val("Peak:"),
            _ => {}
        }
    }
    r
}

/// Merge format- and stream-level tags (Ogg/Opus keep them on the stream).
fn collect_tags(pr: &Probe) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    if let Some(a) = pr.audio().and_then(|a| a["tags"].as_object()) {
        for (k, v) in a {
            if !matches!(
                k.to_lowercase().as_str(),
                "encoder" | "language" | "handler_name" | "vendor_id"
            ) {
                m.insert(k.to_lowercase(), v.clone());
            }
        }
    }
    if let Some(t) = pr.format()["tags"].as_object() {
        for (k, v) in t {
            m.insert(k.to_lowercase(), v.clone());
        }
    }
    m
}

const TAG_FIELDS: &[(&str, &str)] = &[
    ("title", "title"),
    ("artist", "artist"),
    ("album", "album"),
    ("album_artist", "album_artist"),
    ("year", "date"),
    ("genre", "genre"),
    ("track", "track"),
    ("disc", "disc"),
    ("composer", "composer"),
    ("comment", "comment"),
];

fn tags() -> Arc<dyn Tool> {
    let mut spec = audio_spec("audio.tags", "Audio Tags")
        .describe("Read or edit audio tags (title, artist, album, year, genre, track, comment…) and embed or remove cover art — without re-encoding")
        .keywords(&["id3", "metadata", "mp3 tags", "cover art", "album art", "vorbis comment", "artist", "title", "tag editor"])
        .param(ParamSpec::file("input").label("Audio file").accept(MEDIA_ACCEPT).required().primary());
    for (name, _) in TAG_FIELDS {
        spec = spec.param(ParamSpec::text(name).max_len(2000));
    }
    let spec = spec
        .param(ParamSpec::file("cover").label("Cover image").accept(&[
            "image/jpeg",
            "image/png",
            "image/*",
        ]))
        .param(ParamSpec::boolean("remove_cover"))
        .param(
            ParamSpec::boolean("clear_all")
                .describe("Remove all existing tags before writing")
                .advanced(),
        );
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let pr = probe_audio(&ctx, &f).await?;
        let before = collect_tags(&pr);
        let edits: Vec<(&str, String)> = TAG_FIELDS
            .iter()
            .filter_map(|(n, key)| p.opt_str(n).map(|v| (*key, v.trim().to_string())))
            .filter(|(_, v)| !v.is_empty())
            .collect();
        let cover = p.opt_file("cover").cloned();
        let table = |m: &serde_json::Map<String, Value>| json!({"columns": ["Tag", "Value"], "rows": m.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>()});
        if edits.is_empty() && cover.is_none() && !p.bool("remove_cover") && !p.bool("clear_all") {
            return Ok(ToolOutput::data(
                json!({"tags": before, "has_cover": pr.has_cover(), "table": table(&before)}),
            ));
        }
        let ext = f.extension();
        if !matches!(
            ext.as_str(),
            "mp3"
                | "m4a"
                | "m4b"
                | "mp4"
                | "flac"
                | "ogg"
                | "oga"
                | "opus"
                | "wav"
                | "aiff"
                | "aif"
                | "wma"
                | "mka"
        ) {
            return Err(ToolError::unsupported(format!(
                "editing tags of .{ext} files is not supported"
            )));
        }
        if cover.is_some() && !matches!(ext.as_str(), "mp3" | "m4a" | "m4b" | "mp4" | "flac") {
            return Err(ToolError::invalid(
                "cover",
                format!(
                    "FFmpeg cannot embed cover art in .{ext}; convert to MP3, M4A or FLAC first"
                ),
            ));
        }
        let ogg = matches!(ext.as_str(), "ogg" | "oga" | "opus");
        let mut ff = Ff::new().input(&f.path);
        if let Some(c) = &cover {
            ff = ff.input(&c.path);
        }
        ff = ff.args(["-map", "0:a"]);
        if let Some(c) = &cover {
            let copy = matches!(c.extension().as_str(), "jpg" | "jpeg" | "png");
            ff = ff.args([
                "-map",
                "1:v:0",
                "-c:v",
                if copy { "copy" } else { "mjpeg" },
                "-disposition:v:0",
                "attached_pic",
                "-metadata:s:v:0",
                "title=Album cover",
                "-metadata:s:v:0",
                "comment=Cover (front)",
            ]);
        } else if pr.has_cover() && !p.bool("remove_cover") {
            ff = ff.args(["-map", "0:v", "-c:v", "copy"]);
        }
        ff = ff.args([
            "-c:a",
            "copy",
            "-map_metadata",
            if p.bool("clear_all") { "-1" } else { "0" },
        ]);
        if ogg && !p.bool("clear_all") {
            ff = ff.args(["-map_metadata:s:a:0", "0:s:a:0"]);
        }
        for (k, v) in &edits {
            ff = ff.args(["-metadata", &format!("{k}={v}")]);
            if ogg {
                ff = ff.args(["-metadata:s:a:0", &format!("{k}={v}")]);
            }
        }
        if ext == "mp3" {
            ff = ff.args(["-id3v2_version", "3", "-write_id3v1", "1"]);
        }
        let out = out_path(&ctx, &f, "tagged", &ext);
        ff.out(&out)
            .duration(pr.duration())
            .span((0.0, 1.0), "Writing tags")
            .run(&ctx)
            .await?;
        let npr = probe(&ctx, &out).await?;
        let after = collect_tags(&npr);
        Ok(ToolOutput::files(vec![ctx.artifact(&out)?]).with_data(json!({"tags": after, "previous_tags": before, "has_cover": npr.has_cover(), "table": table(&after)})))
    })
}

// ---------------------------------------------------------------------------
// audio.denoise / speed / equalizer / fade / reverse

fn denoise() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.denoise", "Reduce Noise")
        .describe("Reduce background hiss and hum with FFmpeg's FFT denoiser (afftdn) or non-local-means (anlmdn), plus optional high-/low-pass filters")
        .keywords(&["noise reduction", "hiss", "hum", "background noise", "clean audio", "afftdn", "anlmdn", "rumble"])
        .param(audio_input(true))
        .param(ParamSpec::choice("method", &[("afftdn", "FFT denoise (adaptive)"), ("anlmdn", "Non-local means")]).default_str("afftdn"))
        .param(ParamSpec::choice("strength", &[("light", "Light"), ("medium", "Medium"), ("strong", "Strong")]).default_str("medium"))
        .param(ParamSpec::number("highpass").label("High-pass (Hz, 0 = off)").range(0.0, 2000.0).default_num(80.0).describe("Remove rumble below this frequency"))
        .param(ParamSpec::number("lowpass").label("Low-pass (Hz, 0 = off)").range(0.0, 24000.0).default_num(0.0).advanced())
        .param(ParamSpec::number("noise_floor").label("Noise floor (dB)").range(-80.0, -20.0).default_num(-50.0).show_if("method", &["afftdn"]).advanced());
    async_tool(spec, |ctx, p| async move {
        let mut chain = vec![];
        let hp = p.num_or("highpass", 0.0);
        if hp > 0.0 {
            chain.push(format!("highpass=f={hp}"));
        }
        let lp = p.num_or("lowpass", 0.0);
        if lp > 0.0 {
            chain.push(format!("lowpass=f={lp}"));
        }
        let st = p.str("strength")?;
        chain.push(match p.str("method")? {
            "anlmdn" => format!(
                "anlmdn=s={}",
                match st {
                    "light" => "0.0001",
                    "strong" => "0.01",
                    _ => "0.001",
                }
            ),
            _ => format!(
                "afftdn=nr={}:nf={}:tn=1",
                match st {
                    "light" => 6,
                    "strong" => 24,
                    _ => 12,
                },
                p.num_or("noise_floor", -50.0)
            ),
        });
        let af = chain.join(",");
        af_batch(&ctx, &p, "denoised", "Denoising", |_, _| {
            Ok((af.clone(), None, json!({"filter": af})))
        })
        .await
    })
}

fn speed() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.speed", "Change Audio Speed / Pitch")
        .describe("Change tempo without affecting pitch (0.25×–4×) and/or shift pitch by semitones — uses Rubber Band when available")
        .keywords(&["tempo", "pitch shift", "faster", "slower", "playback speed", "semitones", "nightcore", "chipmunk"])
        .param(audio_input(true))
        .param(ParamSpec::number("tempo").range(0.25, 4.0).step(0.05).default_num(1.25))
        .param(ParamSpec::number("pitch").label("Pitch (semitones)").range(-24.0, 24.0).step(0.5).default_num(0.0))
        .param(ParamSpec::choice("engine", &[("auto", "Auto"), ("rubberband", "Rubber Band"), ("atempo", "FFmpeg atempo/asetrate")]).default_str("auto").advanced());
    async_tool(spec, |ctx, p| async move {
        let tempo = p.num("tempo")?;
        let semis = p.num_or("pitch", 0.0);
        if (tempo - 1.0).abs() < 1e-6 && semis.abs() < 1e-6 {
            return Err(ToolError::input(
                "tempo is 1× and pitch is 0 — nothing to change",
            ));
        }
        let has_rb = caps(&ctx).await?.filter("rubberband");
        let engine = match p.str_or("engine", "auto") {
            "rubberband" if !has_rb => {
                return Err(ToolError::unsupported(
                    "this FFmpeg build has no rubberband filter",
                ))
            }
            "auto" => {
                if has_rb && semis.abs() > 1e-6 {
                    "rubberband"
                } else {
                    "atempo"
                }
            }
            e => e,
        }
        .to_string();
        let ratio = 2f64.powf(semis / 12.0);
        af_batch(&ctx, &p, "speed", "Processing", |_, pr| {
            let sr = pr.sample_rate().unwrap_or(44100);
            let af = if engine == "rubberband" {
                format!("rubberband=tempo={tempo:.6}:pitch={ratio:.6}:pitchq=quality")
            } else if semis.abs() > 1e-6 {
                format!(
                    "asetrate={:.0},aresample={sr},{}",
                    sr as f64 * ratio,
                    speed_chain(tempo / ratio)
                )
            } else {
                speed_chain(tempo)
            };
            Ok((
                af,
                pr.duration().map(|d| d / tempo),
                json!({"engine": engine, "tempo": tempo, "pitch_semitones": semis}),
            ))
        })
        .await
    })
}

fn equalizer() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.equalizer", "Equalizer")
        .describe("Shape the sound with presets (bass boost, treble, voice clarity, podcast, loudness) or custom EQ bands, plus high-/low-pass filters")
        .keywords(&["eq", "bass boost", "treble", "voice", "podcast", "highpass", "lowpass", "tone"])
        .param(audio_input(true))
        .param(
            ParamSpec::choice(
                "preset",
                &[("bass_boost", "Bass boost"), ("bass_cut", "Bass cut"), ("treble_boost", "Treble boost"), ("treble_cut", "Treble cut"), ("voice", "Voice clarity"), ("podcast", "Podcast (voice + compression)"), ("loudness", "Loudness (smile curve)"), ("none", "None (custom only)")],
            )
            .default_str("bass_boost"),
        )
        .param(ParamSpec::text("bands").placeholder("60:+4, 1000:-2, 8000:+3:1.5").describe("Custom bands as frequency:gain_dB[:Q], comma separated"))
        .param(ParamSpec::number("highpass").label("High-pass (Hz, 0 = off)").range(0.0, 5000.0).default_num(0.0).advanced())
        .param(ParamSpec::number("lowpass").label("Low-pass (Hz, 0 = off)").range(0.0, 24000.0).default_num(0.0).advanced());
    async_tool(spec, |ctx, p| async move {
        let mut chain: Vec<String> = vec![];
        let hp = p.num_or("highpass", 0.0);
        if hp > 0.0 {
            chain.push(format!("highpass=f={hp}"));
        }
        let lp = p.num_or("lowpass", 0.0);
        if lp > 0.0 {
            chain.push(format!("lowpass=f={lp}"));
        }
        let mut boost = true;
        let preset: &[&str] = match p.str("preset")? {
            "bass_boost" => &["bass=g=8:f=110:w=0.6"],
            "bass_cut" => &["bass=g=-10:f=110:w=0.6"],
            "treble_boost" => &["treble=g=6:f=4000:w=0.6"],
            "treble_cut" => &["treble=g=-8:f=4000:w=0.6"],
            "voice" => &[
                "highpass=f=90",
                "equalizer=f=250:t=q:w=1.2:g=-3",
                "equalizer=f=3000:t=q:w=1:g=4",
                "equalizer=f=6000:t=q:w=2:g=2",
            ],
            "podcast" => &[
                "highpass=f=80",
                "equalizer=f=200:t=q:w=1:g=-2",
                "equalizer=f=3500:t=q:w=1:g=3",
                "acompressor=threshold=-20dB:ratio=3:attack=10:release=200:makeup=2",
            ],
            "loudness" => &["bass=g=5:f=100", "treble=g=4:f=8000"],
            _ => {
                boost = false;
                &[]
            }
        };
        chain.extend(preset.iter().map(|s| s.to_string()));
        for part in p
            .opt_str("bands")
            .unwrap_or("")
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let v: Vec<&str> = part.split(':').map(str::trim).collect();
            let parse = |s: &str| {
                s.trim_start_matches('+')
                    .parse::<f64>()
                    .ok()
                    .filter(|x| x.is_finite())
            };
            let (Some(fr), Some(g)) = (
                v.first().and_then(|s| parse(s)),
                v.get(1).and_then(|s| parse(s)),
            ) else {
                return Err(ToolError::invalid(
                    "bands",
                    format!("'{part}' is not frequency:gain[:Q]"),
                ));
            };
            let q = v.get(2).and_then(|s| parse(s)).unwrap_or(1.0);
            if !(10.0..=24000.0).contains(&fr)
                || !(-30.0..=30.0).contains(&g)
                || !(0.05..=20.0).contains(&q)
            {
                return Err(ToolError::invalid(
                    "bands",
                    format!("'{part}' is out of range (10–24000 Hz, ±30 dB, Q 0.05–20)"),
                ));
            }
            if g > 0.0 {
                boost = true;
            }
            chain.push(format!("equalizer=f={fr}:t=q:w={q}:g={g}"));
        }
        if chain.is_empty() {
            return Err(ToolError::input("choose a preset or give custom bands"));
        }
        if boost {
            chain.push("alimiter=limit=0.95:level=0".into());
        }
        let af = chain.join(",");
        af_batch(&ctx, &p, "eq", "Equalizing", |_, _| {
            Ok((af.clone(), None, json!({"filter": af})))
        })
        .await
    })
}

fn fade() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.fade", "Fade In / Out")
        .describe("Add a fade-in at the start and/or a fade-out at the end of audio, with selectable fade curves")
        .keywords(&["fade in", "fade out", "afade", "intro", "outro", "smooth"])
        .param(audio_input(true))
        .param(ParamSpec::number("fade_in").label("Fade in (s)").range(0.0, 600.0).default_num(2.0))
        .param(ParamSpec::number("fade_out").label("Fade out (s)").range(0.0, 600.0).default_num(3.0))
        .param(ParamSpec::choice("curve", &[("tri", "Linear"), ("qsin", "Quarter sine"), ("esin", "Exponential sine"), ("log", "Logarithmic"), ("exp", "Exponential"), ("par", "Parabola")]).default_str("tri").advanced());
    async_tool(spec, |ctx, p| async move {
        let (fi, fo) = (p.num_or("fade_in", 0.0), p.num_or("fade_out", 0.0));
        if fi <= 0.0 && fo <= 0.0 {
            return Err(ToolError::input("set a fade-in or fade-out duration"));
        }
        let curve = p.str_or("curve", "tri").to_string();
        af_batch(&ctx, &p, "faded", "Fading", |f, pr| {
            let mut v = vec![];
            if fi > 0.0 {
                v.push(format!("afade=t=in:st=0:d={}:curve={curve}", secs(fi)));
            }
            if fo > 0.0 {
                let d = pr
                    .duration()
                    .ok_or_else(|| ToolError::input(format!("unknown duration of '{}'", f.name)))?;
                v.push(format!(
                    "afade=t=out:st={}:d={}:curve={curve}",
                    secs((d - fo).max(0.0)),
                    secs(fo.min(d))
                ));
            }
            Ok((v.join(","), None, json!({"fade_in": fi, "fade_out": fo})))
        })
        .await
    })
}

fn reverse() -> Arc<dyn Tool> {
    let spec = audio_spec("audio.reverse", "Reverse Audio")
        .describe("Play audio backwards (the whole file is buffered in memory)")
        .keywords(&["backwards", "reverse", "backmasking", "areverse"])
        .param(audio_input(true));
    async_tool(spec, |ctx, p| async move {
        for f in p.files("input") {
            let pr = probe_audio(&ctx, f).await?;
            if pr.duration().map(|d| d > 3.0 * 3600.0).unwrap_or(false) {
                return Err(ToolError::limit(format!(
                    "'{}' is longer than 3 hours; reversing buffers everything in memory",
                    f.name
                )));
            }
        }
        af_batch(&ctx, &p, "reversed", "Reversing", |_, _| {
            Ok(("areverse".into(), None, json!({})))
        })
        .await
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silences() {
        let log: Vec<String> = [
            "[silencedetect @ 0x1] silence_start: 1.5",
            "[silencedetect @ 0x1] silence_end: 2.75 | silence_duration: 1.25",
            "[silencedetect @ 0x1] silence_start: 9",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            parse_silences(&log, Some(10.0)),
            vec![(1.5, 2.75), (9.0, 10.0)]
        );
    }

    #[test]
    fn loudnorm_json() {
        let log: Vec<String> = [
            "[Parsed_loudnorm_0 @ 0x55]",
            "{",
            "\t\"input_i\" : \"-27.61\",",
            "\t\"input_tp\" : \"-4.47\",",
            "\t\"target_offset\" : \"0.58\"",
            "}",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let v = parse_loudnorm(&log).unwrap();
        assert_eq!(jnum(&v, "input_i"), Some(-27.61));
    }

    #[test]
    fn ebur() {
        let log: Vec<String> = [
            "[Parsed_ebur128_0 @ 0x] Summary:",
            "",
            "  Integrated loudness:",
            "    I:         -20.1 LUFS",
            "    Threshold: -30.3 LUFS",
            "",
            "  Loudness range:",
            "    LRA:         0.0 LU",
            "    Threshold:   0.0 LUFS",
            "    LRA low:     0.0 LUFS",
            "",
            "  True peak:",
            "    Peak:       -3.0 dBFS",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(parse_ebur128(&log), (Some(-20.1), Some(0.0), Some(-3.0)));
    }

    #[test]
    fn peaks() {
        let pcm = vec![0i16, 100, -100, 32767, -32768, 0];
        let p = compute_peaks(&pcm, 2);
        assert_eq!(p.len(), 2);
        assert!(p[1].1 > 0.99 && p[1].0 < -0.99);
        assert!(compute_peaks(&[], 10).is_empty());
    }
}
