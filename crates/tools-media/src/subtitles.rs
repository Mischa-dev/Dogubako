//! Subtitle / transcript model, SRT / WebVTT / JSON parsing and formatting
//! (pure Rust), plus subtitle tools for videos (mux, remove, burn, extract).

use crate::ffmpeg::*;
use dogubako_core::prelude::*;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![
        transcript_convert(),
        subtitle_shift(),
        subtitles_add(),
        subtitles_remove(),
        subtitles_burn(),
        subtitles_extract(),
    ]
}

/// One timed text segment (subtitle cue / transcript segment).
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    /// tinydiarize: a new speaker starts after this segment.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub speaker_turn: bool,
}

// ---------------------------------------------------------------------------
// Timestamps

/// Parse `HH:MM:SS,mmm`, `HH:MM:SS.mmm` or `MM:SS.mmm`.
pub fn parse_cue_time(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return None;
    }
    let mut total = 0.0;
    for (i, p) in parts.iter().enumerate() {
        let last = i == parts.len() - 1;
        if p.is_empty() || (!last && !p.chars().all(|c| c.is_ascii_digit())) {
            return None;
        }
        let v: f64 = p.parse().ok()?;
        if !v.is_finite() || v < 0.0 {
            return None;
        }
        total = total * 60.0 + v;
    }
    Some(total)
}

fn fmt_time(t: f64, sep: char) -> String {
    let ms = (t.max(0.0) * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3_600_000,
        (ms / 60_000) % 60,
        (ms / 1000) % 60,
        ms % 1000
    )
}

fn parse_timing(line: &str) -> Option<(f64, f64)> {
    let (a, b) = line.split_once("-->")?;
    let start = parse_cue_time(a)?;
    // WebVTT cue settings may follow the end time.
    let end = parse_cue_time(b.split_whitespace().next()?)?;
    Some((start, end.max(start)))
}

fn normalize(s: &str) -> String {
    s.trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n")
}

fn blocks(s: &str) -> Vec<Vec<&str>> {
    let mut out = vec![];
    let mut cur = vec![];
    for l in s.lines() {
        if l.trim().is_empty() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(l);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

// ---------------------------------------------------------------------------
// Parsers

pub fn parse_srt(input: &str) -> Result<Vec<Segment>, String> {
    let s = normalize(input);
    let mut segs = vec![];
    for b in blocks(&s) {
        let Some(ti) = b.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let Some((start, end)) = parse_timing(b[ti]) else {
            return Err(format!("invalid timing line: '{}'", b[ti].trim()));
        };
        let text = b[ti + 1..].join("\n").trim().to_string();
        segs.push(Segment {
            start,
            end,
            text,
            ..Default::default()
        });
    }
    if segs.is_empty() {
        return Err("no subtitle cues found".into());
    }
    Ok(segs)
}

fn strip_vtt_tags(text: &str) -> (String, Option<String>) {
    let mut speaker = None;
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('<') {
        out.push_str(&rest[..i]);
        let Some(j) = rest[i..].find('>') else {
            out.push_str(&rest[i..]);
            rest = "";
            break;
        };
        let tag = &rest[i + 1..i + j];
        if let Some(v) = tag
            .strip_prefix('v')
            .filter(|v| v.starts_with(' ') || v.starts_with('.'))
        {
            let name = v.split_once(' ').map(|x| x.1).unwrap_or("").trim();
            if !name.is_empty() {
                speaker = Some(name.to_string());
            }
        } else if matches!(tag, "i" | "/i" | "b" | "/b" | "u" | "/u") {
            out.push('<');
            out.push_str(tag);
            out.push('>');
        }
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    (decode_char_refs(&out), speaker)
}

/// Decode WebVTT character references in one pass, so `&amp;lt;` becomes the
/// literal `&lt;` (decoded text is never decoded again).
fn decode_char_refs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        // Bounded search: a reference name is at most 10 bytes, so looking
        // further would make text full of bare '&' quadratic.
        let semi = after.bytes().take(11).position(|b| b == b';');
        let decoded = semi.and_then(|e| {
            let name = &after[..e];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "nbsp" => Some('\u{a0}'),
                "lrm" => Some('\u{200e}'),
                "rlm" => Some('\u{200f}'),
                _ => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            }?;
            Some((c, e))
        });
        match decoded {
            Some((c, e)) => {
                // Keep the previous behaviour of plain spaces for &nbsp;.
                out.push(if c == '\u{a0}' { ' ' } else { c });
                rest = &after[e + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

pub fn parse_vtt(input: &str) -> Result<Vec<Segment>, String> {
    let s = normalize(input);
    if !s.trim_start().starts_with("WEBVTT") {
        return Err("not a WebVTT file (missing WEBVTT header)".into());
    }
    let mut segs = vec![];
    for (k, b) in blocks(&s).into_iter().enumerate() {
        if k == 0 && b[0].starts_with("WEBVTT") && !b.iter().any(|l| l.contains("-->")) {
            continue;
        }
        if b[0].starts_with("NOTE") || b[0].starts_with("STYLE") || b[0].starts_with("REGION") {
            continue;
        }
        let Some(ti) = b.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        let Some((start, end)) = parse_timing(b[ti]) else {
            return Err(format!("invalid timing line: '{}'", b[ti].trim()));
        };
        let (text, speaker) = strip_vtt_tags(b[ti + 1..].join("\n").trim());
        segs.push(Segment {
            start,
            end,
            text,
            speaker,
            ..Default::default()
        });
    }
    if segs.is_empty() {
        return Err("no cues found".into());
    }
    Ok(segs)
}

fn json_time(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => parse_cue_time(s).or_else(|| s.parse().ok()),
        _ => None,
    }
}

pub fn parse_json(input: &str) -> Result<Vec<Segment>, String> {
    let v: Value = serde_json::from_str(input.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("invalid JSON: {e}"))?;
    // whisper.cpp output
    if let Some(arr) = v["transcription"].as_array() {
        return Ok(arr
            .iter()
            .map(|s| Segment {
                start: s["offsets"]["from"].as_f64().unwrap_or(0.0) / 1000.0,
                end: s["offsets"]["to"].as_f64().unwrap_or(0.0) / 1000.0,
                text: s["text"].as_str().unwrap_or("").trim().to_string(),
                speaker: s["speaker"].as_str().map(String::from),
                speaker_turn: s["speaker_turn_next"].as_bool().unwrap_or(false),
            })
            .collect());
    }
    let arr = v["segments"]
        .as_array()
        .or(v.as_array())
        .ok_or("expected a list of segments or {\"segments\": [...]}")?;
    let mut segs = vec![];
    for (i, s) in arr.iter().enumerate() {
        let start =
            json_time(&s["start"]).ok_or(format!("segment {} has no valid start", i + 1))?;
        let end = json_time(&s["end"]).ok_or(format!("segment {} has no valid end", i + 1))?;
        segs.push(Segment {
            start,
            end: end.max(start),
            text: s["text"].as_str().unwrap_or("").trim().to_string(),
            speaker: s["speaker"].as_str().map(String::from),
            speaker_turn: s["speaker_turn"].as_bool().unwrap_or(false),
        });
    }
    Ok(segs)
}

/// Detect the format of transcript text.
pub fn detect_format(s: &str) -> &'static str {
    let t = normalize(s);
    let t = t.trim_start();
    if t.starts_with("WEBVTT") {
        "vtt"
    } else if t.starts_with('{') || t.starts_with('[') {
        "json"
    } else if t.lines().take(6).any(|l| l.contains("-->")) {
        "srt"
    } else {
        "txt"
    }
}

pub fn parse_any(s: &str, fmt: &str) -> Result<Vec<Segment>, String> {
    match fmt {
        "srt" => parse_srt(s),
        "vtt" => parse_vtt(s),
        "json" => parse_json(s),
        _ => Ok(s
            .split("\n\n")
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(|p| Segment {
                text: p.replace('\n', " "),
                ..Default::default()
            })
            .collect()),
    }
}

// ---------------------------------------------------------------------------
// Formatters

fn turn_marks(segs: &[Segment]) -> Vec<bool> {
    // true if a new speaker starts at segment i
    let mut v = vec![false; segs.len()];
    for i in 1..segs.len() {
        v[i] = segs[i - 1].speaker_turn
            || (segs[i].speaker.is_some() && segs[i].speaker != segs[i - 1].speaker);
    }
    v
}

pub fn to_srt(segs: &[Segment]) -> String {
    let marks = turn_marks(segs);
    let mut out = String::new();
    for (i, s) in segs.iter().enumerate() {
        let mut text = s.text.trim().to_string();
        if let Some(sp) = &s.speaker {
            text = format!("{sp}: {text}");
        } else if marks[i] {
            text = format!("- {text}");
        }
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            fmt_time(s.start, ','),
            fmt_time(s.end, ','),
            text
        ));
    }
    out
}

pub fn to_vtt(segs: &[Segment]) -> String {
    let marks = turn_marks(segs);
    let mut out = String::from("WEBVTT\n\n");
    for (i, s) in segs.iter().enumerate() {
        let esc = s
            .text
            .trim()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        // keep simple formatting tags
        let esc = ["i", "b", "u"].iter().fold(esc, |t, tag| {
            t.replace(&format!("&lt;{tag}&gt;"), &format!("<{tag}>"))
                .replace(&format!("&lt;/{tag}&gt;"), &format!("</{tag}>"))
        });
        let text = match &s.speaker {
            Some(sp) => format!("<v {}>{esc}", sp.replace(['<', '>'], "")),
            None if marks[i] => format!("- {esc}"),
            None => esc,
        };
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            fmt_time(s.start, '.'),
            fmt_time(s.end, '.'),
            text
        ));
    }
    out
}

fn paragraphs(segs: &[Segment]) -> Vec<(f64, Option<String>, String)> {
    let marks = turn_marks(segs);
    let mut out: Vec<(f64, Option<String>, String)> = vec![];
    for (i, s) in segs.iter().enumerate() {
        let t = s.text.trim();
        if t.is_empty() {
            continue;
        }
        let gap = i > 0 && s.start - segs[i - 1].end >= 3.0;
        let long = out
            .last()
            .map(|p| p.2.len() > 1200 && p.2.ends_with(['.', '?', '!']))
            .unwrap_or(false);
        let new_para = out.is_empty() || marks[i] || gap || long;
        if new_para {
            out.push((s.start, s.speaker.clone(), t.to_string()));
        } else if let Some(p) = out.last_mut() {
            p.2.push(' ');
            p.2.push_str(t);
        }
    }
    out
}

pub fn to_txt(segs: &[Segment]) -> String {
    let mut out = String::new();
    for (_, sp, text) in paragraphs(segs) {
        if let Some(sp) = sp {
            out.push_str(&format!("{sp}: "));
        }
        out.push_str(&text);
        out.push_str("\n\n");
    }
    out.trim_end().to_string() + "\n"
}

pub fn to_md(segs: &[Segment], title: &str) -> String {
    let mut out = format!("# {title}\n\n");
    let timed = segs.iter().any(|s| s.end > 0.0);
    for (t, sp, text) in paragraphs(segs) {
        if timed {
            out.push_str(&format!(
                "**[{}]** ",
                fmt_time(t, '.').split('.').next().unwrap_or("")
            ));
        }
        if let Some(sp) = sp {
            out.push_str(&format!("**{sp}:** "));
        }
        out.push_str(&text);
        out.push_str("\n\n");
    }
    out
}

pub fn to_json(segs: &[Segment], extra: Value) -> String {
    let text = segs
        .iter()
        .map(|s| s.text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut v = json!({"text": text, "segments": segs});
    if let (Value::Object(m), Value::Object(e)) = (&mut v, extra) {
        m.extend(e);
    }
    serde_json::to_string_pretty(&v).unwrap_or_default()
}

pub fn format_segments(
    segs: &[Segment],
    fmt: &str,
    title: &str,
) -> (String, &'static str, &'static str) {
    match fmt {
        "srt" => (to_srt(segs), "srt", "text/plain"),
        "vtt" => (to_vtt(segs), "vtt", "text/plain"),
        "json" => (to_json(segs, json!({})), "json", "application/json"),
        "md" => (to_md(segs, title), "md", "text/markdown"),
        _ => (to_txt(segs), "txt", "text/plain"),
    }
}

/// Shift and scale cue timing: `t' = t * factor + offset` (only for cues
/// starting at or after `from`). Cues ending before 0 are dropped.
pub fn shift(segs: &[Segment], offset: f64, factor: f64, from: f64) -> Vec<Segment> {
    segs.iter()
        .filter_map(|s| {
            if s.start + 1e-9 < from {
                return Some(s.clone());
            }
            let mut n = s.clone();
            n.start = s.start * factor + offset;
            n.end = s.end * factor + offset;
            if n.end <= 0.0 {
                return None;
            }
            n.start = n.start.max(0.0);
            Some(n)
        })
        .collect()
}

pub fn segments_table(segs: &[Segment]) -> Value {
    let rows: Vec<Value> = segs
        .iter()
        .enumerate()
        .map(|(i, s)| json!([i + 1, fmt_time(s.start, '.'), fmt_time(s.end, '.'), s.text]))
        .collect();
    json!({"columns": ["#", "Start", "End", "Text"], "rows": rows})
}

// ---------------------------------------------------------------------------
// Tools: transcript_convert / subtitle_shift

fn read_input(p: &Params) -> ToolResult<(String, String)> {
    if let Some(f) = p.opt_file("file") {
        if f.size() > 50 * 1024 * 1024 {
            return Err(ToolError::invalid(
                "file",
                "subtitle file is larger than 50 MB",
            ));
        }
        let bytes = std::fs::read(&f.path)?;
        let text = String::from_utf8(bytes)
            .unwrap_or_else(|e| e.into_bytes().iter().map(|b| *b as char).collect());
        return Ok((text, f.stem()));
    }
    let t = p.text("text");
    if t.trim().is_empty() {
        return Err(ToolError::invalid(
            "text",
            "paste a transcript or choose a file",
        ));
    }
    Ok((t.to_string(), "transcript".into()))
}

fn input_params(spec: ToolSpec) -> ToolSpec {
    spec.param(
        ParamSpec::textarea("text")
            .label("Transcript / subtitles")
            .primary()
            .describe("Paste SRT, WebVTT, JSON segments or plain text"),
    )
    .param(
        ParamSpec::file("file")
            .accept(&[".srt", ".vtt", ".json", ".txt"])
            .describe("…or choose a file"),
    )
    .param(
        ParamSpec::choice(
            "input_format",
            &[
                ("auto", "Auto-detect"),
                ("srt", "SRT"),
                ("vtt", "WebVTT"),
                ("json", "JSON segments / whisper.cpp JSON"),
                ("txt", "Plain text"),
            ],
        )
        .default_str("auto")
        .advanced(),
    )
}

fn parse_input(p: &Params) -> ToolResult<(Vec<Segment>, &'static str, String)> {
    let (text, stem) = read_input(p)?;
    let fmt: &'static str = match p.str_or("input_format", "auto") {
        "srt" => "srt",
        "vtt" => "vtt",
        "json" => "json",
        "txt" => "txt",
        _ => detect_format(&text),
    };
    let segs = parse_any(&text, fmt).map_err(|e| {
        ToolError::invalid(
            if p.opt_file("file").is_some() {
                "file"
            } else {
                "text"
            },
            e,
        )
    })?;
    Ok((segs, fmt, stem))
}

fn transcript_convert() -> Arc<dyn Tool> {
    let spec = input_params(ToolSpec::new("speech.transcript_convert", "Convert Transcript / Subtitles", Category::Speech))
        .describe("Convert transcripts and subtitles between SRT, WebVTT, JSON segments, plain text and Markdown")
        .keywords(&["srt to vtt", "vtt to srt", "subtitles", "captions", "transcript", "json", "convert subtitles"])
        .param(ParamSpec::choice("output_format", &[("vtt", "WebVTT"), ("srt", "SRT"), ("json", "JSON segments"), ("txt", "Plain text"), ("md", "Markdown")]).default_str("vtt"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |ctx, p| {
        let (segs, from, stem) = parse_input(p)?;
        let to = p.str("output_format")?;
        if from == "txt" && matches!(to, "srt" | "vtt" | "json") {
            return Err(ToolError::invalid(
                "output_format",
                "plain text has no timing; it can only become text or Markdown",
            ));
        }
        let (out, ext, mime) = format_segments(&segs, to, "Transcript");
        let art = ctx.write_output(&format!("{stem}.{ext}"), out.as_bytes())?;
        Ok(ToolOutput::text_as(out, mime)
            .with_artifact(art)
            .with_data(json!({"input_format": from, "count": segs.len(), "segments": segs})))
    })
}

fn subtitle_shift() -> Arc<dyn Tool> {
    let spec = input_params(ToolSpec::new("speech.subtitle_shift", "Shift Subtitle Timing", Category::Speech))
        .describe("Fix out-of-sync subtitles: shift SRT/WebVTT cues by an offset, stretch them by a factor or frame-rate ratio, optionally from a given time on")
        .keywords(&["sync subtitles", "delay", "offset", "subtitle timing", "resync", "framerate", "srt"])
        .param(ParamSpec::number("offset").label("Offset (seconds, negative = earlier)").range(-86400.0, 86400.0).default_num(0.0))
        .param(ParamSpec::number("factor").label("Speed factor").range(0.1, 10.0).default_num(1.0).describe("Multiply all times (e.g. 1.001 or 25/23.976)").advanced())
        .param(ParamSpec::number("fps_from").label("Convert from frame rate").range(1.0, 240.0).describe("Sets the factor to fps_from / fps_to").advanced())
        .param(ParamSpec::number("fps_to").label("…to frame rate").range(1.0, 240.0).advanced())
        .param(ParamSpec::text("from_time").placeholder("00:10:00").describe("Only shift cues starting at or after this time").advanced())
        .param(ParamSpec::choice("output_format", &[("same", "Same as input"), ("srt", "SRT"), ("vtt", "WebVTT")]).default_str("same"))
        .text_output("text/plain")
        .ui("live");
    sync_tool(spec, |ctx, p| {
        let (segs, from_fmt, stem) = parse_input(p)?;
        if from_fmt == "txt" {
            return Err(ToolError::input("the input has no timing to shift"));
        }
        let mut factor = p.num_or("factor", 1.0);
        if let (Some(a), Some(b)) = (p.opt_num("fps_from"), p.opt_num("fps_to")) {
            factor = a / b;
        }
        let from = opt_time(p, "from_time")?.unwrap_or(0.0);
        let offset = p.num_or("offset", 0.0);
        let shifted = shift(&segs, offset, factor, from);
        let dropped = segs.len() - shifted.len();
        let to = match p.str("output_format")? {
            "same" if from_fmt == "vtt" => "vtt",
            "same" if from_fmt == "json" => "json",
            "same" => "srt",
            o => o,
        };
        let (out, ext, mime) = format_segments(&shifted, to, "Transcript");
        let art = ctx.write_output(&format!("{stem}-shifted.{ext}"), out.as_bytes())?;
        let mut o = ToolOutput::text_as(out, mime).with_artifact(art).with_data(
            json!({"count": shifted.len(), "dropped": dropped, "offset": offset, "factor": factor}),
        );
        if dropped > 0 {
            o = o.warn(format!(
                "{dropped} cue(s) moved before 0:00 and were dropped"
            ));
        }
        Ok(o)
    })
}

// ---------------------------------------------------------------------------
// Video subtitle tools

const BITMAP_SUBS: &[&str] = &["hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle", "xsub"];

fn lang_ok(l: &str) -> bool {
    (2..=3).contains(&l.len()) && l.chars().all(|c| c.is_ascii_alphabetic())
}

/// Codec used for text subtitles in a container.
fn sub_codec_for(fmt: &str, src_ext: &str) -> &'static str {
    match fmt {
        "mp4" | "mov" => "mov_text",
        "webm" => "webvtt",
        _ => match src_ext {
            "ass" | "ssa" => "ass",
            "vtt" => "webvtt",
            _ => "srt",
        },
    }
}

/// Mux subtitle files into a video (stream copy for audio/video).
pub(crate) async fn mux_subtitles(
    ctx: &ToolContext,
    video: &Path,
    pr: &Probe,
    subs: &[(PathBuf, Option<String>)],
    fmt: &str,
    default_first: bool,
    out: &Path,
    sp: (f32, f32),
) -> ToolResult<()> {
    let existing = pr.subtitle_streams();
    if fmt != "mkv"
        && existing
            .iter()
            .any(|s| BITMAP_SUBS.contains(&s["codec_name"].as_str().unwrap_or("")))
    {
        return Err(ToolError::input(
            "the video has image-based subtitles that only MKV can hold; choose MKV output",
        ));
    }
    let mut ff = Ff::new().input(video);
    for (s, _) in subs {
        ff = ff.input(s);
    }
    ff = ff.args(["-map", "0", "-dn"]);
    for k in 0..subs.len() {
        ff = ff.args(["-map", &format!("{}:0", k + 1)]);
    }
    ff = ff.args(["-c", "copy"]);
    let e = existing.len();
    if fmt != "mkv" {
        ff = ff.args(["-c:s", sub_codec_for(fmt, "")]);
    }
    for (k, (s, lang)) in subs.iter().enumerate() {
        let idx = e + k;
        if fmt == "mkv" {
            ff = ff.args([
                format!("-c:s:{idx}"),
                sub_codec_for(fmt, &ext_of(s)).to_string(),
            ]);
        }
        if let Some(l) = lang {
            ff = ff.args([format!("-metadata:s:s:{idx}"), format!("language={l}")]);
        }
        if default_first && k == 0 {
            ff = ff.args([format!("-disposition:s:{idx}"), "default".to_string()]);
        }
    }
    ff = ff.args(mux_args(fmt, None)).args(["-map_metadata", "0"]);
    ff.out(out)
        .duration(pr.duration())
        .span(sp, "Adding subtitles")
        .run(ctx)
        .await?;
    Ok(())
}

fn subtitles_add() -> Arc<dyn Tool> {
    let spec = video_spec("video.subtitles_add", "Add Subtitles")
        .describe("Add one or more soft subtitle tracks (SRT, WebVTT, ASS) to a video with language tags — no re-encoding")
        .keywords(&["soft subtitles", "captions", "mux", "srt", "subtitle track", "embed subtitles", "closed captions"])
        .param(video_input(false))
        .param(ParamSpec::files("subtitles").accept(SUB_ACCEPT).required())
        .param(ParamSpec::text("languages").placeholder("eng, deu").describe("ISO 639 language codes, one per subtitle file"))
        .param(ParamSpec::boolean("default_track").default_bool(true).describe("Mark the first added track as default"))
        .param(ParamSpec::choice("format", &[("same", "Same as input (AVI → MKV)"), ("mkv", "MKV"), ("mp4", "MP4"), ("mov", "MOV"), ("webm", "WebM")]).default_str("same"));
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let pr = probe_video(&ctx, &f).await?;
        let subs = p.files("subtitles").to_vec();
        let langs: Vec<String> = p
            .opt_str("languages")
            .unwrap_or("")
            .split([',', ' ', ';'])
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        for l in &langs {
            if !lang_ok(l) {
                return Err(ToolError::invalid(
                    "languages",
                    format!("'{l}' is not an ISO 639 code like 'en' or 'eng'"),
                ));
            }
        }
        let fmt = match p.str("format")? {
            "same" => match same_container(&f) {
                "avi" => "mkv",
                c => c,
            },
            o => o,
        }
        .to_string();
        // validate subtitle files by parsing text formats
        let mut list = vec![];
        for (k, s) in subs.iter().enumerate() {
            let ext = s.extension();
            if matches!(ext.as_str(), "srt" | "vtt") {
                let text = String::from_utf8_lossy(&std::fs::read(&s.path)?).to_string();
                parse_any(&text, &ext)
                    .map_err(|e| ToolError::invalid("subtitles", format!("{}: {e}", s.name)))?;
            }
            list.push((s.path.clone(), langs.get(k).cloned()));
        }
        let out = out_path(&ctx, &f, "subtitled", &fmt);
        mux_subtitles(
            &ctx,
            &f.path,
            &pr,
            &list,
            &fmt,
            p.bool("default_track"),
            &out,
            (0.0, 1.0),
        )
        .await?;
        let npr = probe(&ctx, &out).await?;
        let tracks: Vec<Value> = npr.subtitle_streams().iter().map(|s| json!({"index": s["index"], "codec": s["codec_name"], "language": s["tags"]["language"]})).collect();
        Ok(ToolOutput::files(vec![ctx.artifact(&out)?])
            .with_data(json!({"subtitle_tracks": tracks})))
    })
}

fn subtitles_remove() -> Arc<dyn Tool> {
    let spec = video_spec("video.subtitles_remove", "Remove Subtitles")
        .describe("Remove embedded subtitle tracks from videos without re-encoding (all, or only selected languages)")
        .keywords(&["strip subtitles", "delete captions", "remove subtitle track"])
        .param(video_input(true))
        .param(ParamSpec::text("languages").placeholder("eng, fre").describe("Only remove these languages (empty = remove all)"));
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let langs: Vec<String> = p
            .opt_str("languages")
            .unwrap_or("")
            .split([',', ' ', ';'])
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        let mut rep = Report::files(&["Removed"]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let subs = pr.subtitle_streams();
            let ext = if VIDEO_CONTAINERS.contains(&f.extension().as_str()) {
                f.extension()
            } else {
                "mkv".into()
            };
            let out = out_path(&ctx, f, "nosubs", &ext);
            let mut ff = Ff::new().input(&f.path).args(["-map", "0", "-dn"]);
            let mut removed = 0;
            if langs.is_empty() {
                ff = ff.args(["-map", "-0:s"]);
                removed = subs.len();
            } else {
                for (k, s) in subs.iter().enumerate() {
                    if s["tags"]["language"]
                        .as_str()
                        .map(|l| langs.contains(&l.to_lowercase()))
                        .unwrap_or(false)
                    {
                        ff = ff.args(["-map", &format!("-0:s:{k}")]);
                        removed += 1;
                    }
                }
            }
            if removed == 0 {
                rep.warnings
                    .push(format!("{}: no matching subtitle tracks", f.name));
            }
            ff.args(["-c", "copy", "-map_metadata", "0"])
                .out(&out)
                .duration(pr.duration())
                .span(
                    span(i, files.len()),
                    format!("Removing subtitles from {}", f.name),
                )
                .run(&ctx)
                .await?;
            rep.file(
                &ctx,
                f,
                &out,
                vec![json!(removed)],
                json!({"removed": removed}),
            )?;
        }
        Ok(rep.finish())
    })
}

/// `#rrggbb` → ASS `&H00BBGGRR`.
fn ass_color(hex: &str) -> String {
    let h = hex.trim_start_matches('#');
    if h.len() < 6 {
        return "&H00FFFFFF".into();
    }
    format!("&H00{}{}{}", &h[4..6], &h[2..4], &h[0..2]).to_uppercase()
}

pub(crate) struct BurnStyle {
    pub font_size: i64,
    pub color: String,
    pub outline: String,
    pub position: String,
    pub margin: i64,
    pub force: bool,
}

/// Burn a subtitle file into a video (re-encode).
pub(crate) async fn burn_subtitles(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    p: &Params,
    sub_file: Option<&Path>,
    stream: Option<usize>,
    style: &BurnStyle,
    out_ext_hint: Option<&str>,
    sp: (f32, f32),
    warnings: &mut Vec<String>,
) -> ToolResult<PathBuf> {
    let dir = ctx.temp_dir(&format!("burn-{}", dogubako_core::util::random_id(4)))?;
    let re = reencode(ctx, f, pr, p, true, warnings).await?;
    let ext = out_ext_hint.unwrap_or(re.ext);
    let out = out_path(ctx, f, "burned", ext);
    let mut ff = Ff::new().input(&f.path).cwd(&dir);
    let bitmap_stream = stream.and_then(|k| {
        let s = pr
            .subtitle_streams()
            .get(k)
            .map(|s| s["codec_name"].as_str().unwrap_or("").to_string())?;
        BITMAP_SUBS.contains(&s.as_str()).then_some(k)
    });
    let fc = if let Some(k) = bitmap_stream {
        format!("[0:v:0][0:s:{k}]overlay=(W-w)/2:H-h,format=yuv420p[v]")
    } else {
        let name = match (sub_file, stream) {
            (Some(path), _) => {
                let e = ext_of(path);
                let e = if matches!(e.as_str(), "srt" | "vtt" | "ass" | "ssa") {
                    e
                } else {
                    "srt".into()
                };
                let n = format!("subs.{e}");
                std::fs::copy(path, dir.join(&n))?;
                n
            }
            (None, Some(k)) => {
                if pr.subtitle_streams().get(k).is_none() {
                    return Err(ToolError::invalid(
                        "stream",
                        format!(
                            "the video has {} subtitle track(s)",
                            pr.subtitle_streams().len()
                        ),
                    ));
                }
                Ff::new()
                    .input(&f.path)
                    .args(["-map", &format!("0:s:{k}"), "-c:s", "ass"])
                    .out(&dir.join("subs.ass"))
                    .no_progress()
                    .run(ctx)
                    .await?;
                "subs.ass".to_string()
            }
            (None, None) => {
                return Err(ToolError::invalid(
                    "subtitles",
                    "choose a subtitle file or an embedded track",
                ))
            }
        };
        let is_ass = name.ends_with(".ass") || name.ends_with(".ssa");
        crate::ffmpeg::need_filter(
            ctx,
            "subtitles",
            "burning in subtitles (FFmpeg built with libass)",
        )
        .await?;
        let mut filter = format!("subtitles=filename={name}");
        if !is_ass || style.force {
            let align = match style.position.as_str() {
                "top" => 8,
                "middle" => 5,
                _ => 2,
            };
            let mut st = vec![
                format!("PrimaryColour={}", ass_color(&style.color)),
                format!("OutlineColour={}", ass_color(&style.outline)),
                "BorderStyle=1".into(),
                "Outline=2".into(),
                format!("Alignment={align}"),
                format!("MarginV={}", style.margin),
            ];
            if style.font_size > 0 {
                st.push(format!("FontSize={}", style.font_size));
            }
            filter.push_str(&format!(":force_style='{}'", st.join(",")));
        }
        format!("[0:v:0]{filter}[v]")
    };
    ff = ff.args(["-filter_complex", &fc, "-map", "[v]"]);
    if pr.audio().is_some() {
        ff = ff.args(["-map", "0:a?"]);
    }
    ff.args(re.tail())
        .a("-sn")
        .out(&out)
        .duration(pr.duration())
        .span(sp, format!("Burning subtitles into {}", f.name))
        .run(ctx)
        .await?;
    Ok(out)
}

pub(crate) fn burn_style_params(spec: ToolSpec, show: Option<(&str, &[&str])>) -> ToolSpec {
    let w = |p: ParamSpec| match show {
        Some((k, v)) => p.show_if(k, v),
        None => p,
    };
    spec.param(w(ParamSpec::integer("font_size")
        .range(0.0, 200.0)
        .default_int(0)
        .describe("0 = automatic")))
        .param(w(ParamSpec::color("color").default_str("#ffffff")))
        .param(w(ParamSpec::color("outline_color")
            .default_str("#000000")
            .advanced()))
        .param(w(ParamSpec::choice(
            "position",
            &[("bottom", "Bottom"), ("top", "Top"), ("middle", "Middle")],
        )
        .default_str("bottom")))
        .param(w(ParamSpec::integer("margin")
            .range(0.0, 500.0)
            .default_int(20)
            .advanced()))
}

pub(crate) fn burn_style(p: &Params) -> BurnStyle {
    BurnStyle {
        font_size: p.int_or("font_size", 0),
        color: p.str_or("color", "#ffffff").to_string(),
        outline: p.str_or("outline_color", "#000000").to_string(),
        position: p.str_or("position", "bottom").to_string(),
        margin: p.int_or("margin", 20),
        force: p.bool("override_ass_style"),
    }
}

fn subtitles_burn() -> Arc<dyn Tool> {
    let spec = burn_style_params(
        video_spec("video.subtitles_burn", "Burn In Subtitles")
            .describe("Permanently render subtitles into the picture (hard subs) from an SRT/VTT/ASS file or an embedded track, with font size, colour and position options")
            .keywords(&["hardsub", "hard subtitles", "open captions", "burn subtitles", "render captions"])
            .param(video_input(true))
            .param(ParamSpec::file("subtitles").accept(SUB_ACCEPT))
            .param(ParamSpec::integer("stream").range(0.0, 99.0).describe("…or burn this embedded subtitle track (0 = first)")),
        None,
    )
    .param(ParamSpec::boolean("override_ass_style").describe("Apply the style options to ASS files too").advanced())
    .param(quality_param());
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let sub = p.opt_file("subtitles").cloned();
        let stream = p.opt_int("stream").map(|s| s as usize);
        if sub.is_none() && stream.is_none() {
            return Err(ToolError::invalid(
                "subtitles",
                "choose a subtitle file or an embedded track number",
            ));
        }
        if let Some(s) = &sub {
            let e = s.extension();
            if matches!(e.as_str(), "srt" | "vtt") {
                let text = String::from_utf8_lossy(&std::fs::read(&s.path)?).to_string();
                parse_any(&text, &e).map_err(|err| ToolError::invalid("subtitles", err))?;
            }
        }
        let style = burn_style(&p);
        let mut rep = Report::files(&[]);
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let out = burn_subtitles(
                &ctx,
                f,
                &pr,
                &p,
                sub.as_ref().map(|s| s.path.as_path()),
                stream,
                &style,
                None,
                span(i, files.len()),
                &mut rep.warnings,
            )
            .await?;
            rep.file(&ctx, f, &out, vec![], json!({}))?;
        }
        Ok(rep.finish())
    })
}

fn subtitles_extract() -> Arc<dyn Tool> {
    let spec = video_spec("video.subtitles_extract", "Extract Subtitles")
        .describe("List the subtitle tracks of videos and save them as SRT, WebVTT or ASS (or copy image-based tracks as-is)")
        .keywords(&["rip subtitles", "export captions", "srt", "subtitle track", "demux subtitles"])
        .param(video_input(true))
        .param(ParamSpec::choice("format", &[("srt", "SRT"), ("vtt", "WebVTT"), ("ass", "ASS"), ("copy", "Original format")]).default_str("srt"))
        .param(ParamSpec::integer("track").range(0.0, 99.0).describe("Only this subtitle track (0 = first; empty = all)"))
        .param(ParamSpec::boolean("list_only").describe("Only list the tracks"));
    async_tool(spec, |ctx, p| async move {
        let files = p.files("input").to_vec();
        let fmt = p.str("format")?.to_string();
        let mut rows = vec![];
        let mut artifacts = vec![];
        let mut warnings = vec![];
        let mut tracks = vec![];
        for (i, f) in files.iter().enumerate() {
            let pr = probe_video(&ctx, f).await?;
            let subs = pr.subtitle_streams();
            if subs.is_empty() {
                warnings.push(format!("{}: no subtitle tracks", f.name));
            }
            let n = subs.len();
            for (k, s) in subs.iter().enumerate() {
                let codec = s["codec_name"].as_str().unwrap_or("?").to_string();
                let lang = s["tags"]["language"].as_str().unwrap_or("und").to_string();
                let title = s["tags"]["title"].as_str().unwrap_or("").to_string();
                let bitmap = BITMAP_SUBS.contains(&codec.as_str());
                let mut file_name = String::new();
                let selected = p.opt_int("track").map(|t| t as usize == k).unwrap_or(true);
                if selected && !p.bool("list_only") {
                    let (ext, args): (&str, Vec<&str>) = match (fmt.as_str(), bitmap) {
                        ("copy", true) => (
                            if codec == "hdmv_pgs_subtitle" {
                                "sup"
                            } else {
                                "mks"
                            },
                            vec!["-c:s", "copy"],
                        ),
                        (_, true) => {
                            warnings.push(format!("{} track {k}: image-based subtitles ({codec}) cannot be converted to text; use 'Original format'", f.name));
                            ("", vec![])
                        }
                        ("copy", false) => match codec.as_str() {
                            "ass" | "ssa" => ("ass", vec!["-c:s", "copy"]),
                            "webvtt" => ("vtt", vec!["-c:s", "copy"]),
                            "subrip" => ("srt", vec!["-c:s", "copy"]),
                            _ => ("srt", vec!["-c:s", "srt"]),
                        },
                        ("vtt", false) => ("vtt", vec!["-c:s", "webvtt"]),
                        ("ass", false) => ("ass", vec!["-c:s", "ass"]),
                        _ => ("srt", vec!["-c:s", "srt"]),
                    };
                    if !ext.is_empty() {
                        let out = out_path(
                            &ctx,
                            f,
                            &format!(
                                "sub{k}-{}",
                                lang.chars()
                                    .filter(|c| c.is_ascii_alphanumeric())
                                    .collect::<String>()
                            ),
                            ext,
                        );
                        Ff::new()
                            .input(&f.path)
                            .args(["-map", &format!("0:s:{k}")])
                            .args(args)
                            .out(&out)
                            .duration(pr.duration())
                            .span(
                                sub(
                                    span(i, files.len()),
                                    k as f32 / n as f32,
                                    (k + 1) as f32 / n as f32,
                                ),
                                format!("Extracting track {k}"),
                            )
                            .run(&ctx)
                            .await?;
                        let a = ctx.artifact(&out)?;
                        file_name = a.name.clone();
                        artifacts.push(a);
                    }
                }
                rows.push(json!([
                    f.name,
                    k,
                    codec,
                    lang,
                    title,
                    if bitmap { "image" } else { "text" },
                    file_name
                ]));
                tracks.push(json!({"file": f.name, "track": k, "codec": codec, "language": lang, "title": title, "bitmap": bitmap, "output": file_name}));
            }
        }
        let mut o = ToolOutput::files(artifacts).with_data(json!({"tracks": tracks, "table": {"columns": ["Video", "Track", "Codec", "Language", "Title", "Type", "Output"], "rows": rows}}));
        o.warnings = warnings;
        Ok(o)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vtt_char_refs_decode_once() {
        let (t, _) = strip_vtt_tags(
            "Use &amp;lt; and &amp;gt; literally &lt;b&gt; &#65;&#x42; &nbsp;x &bogus; a & b",
        );
        assert_eq!(t, "Use &lt; and &gt; literally <b> AB  x &bogus; a & b");
        // Ten-byte names still decode; bare ampersands stay linear.
        assert_eq!(decode_char_refs("&#x0001F600;"), "\u{1F600}");
        let many = "&".repeat(400_000) + ";";
        let start = std::time::Instant::now();
        assert_eq!(decode_char_refs(&many), many);
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
    }

    const SRT: &str = "\u{feff}1\r\n00:00:01,000 --> 00:00:02,500\r\nHello <i>world</i>\r\n\r\n2\r\n00:00:03,000 --> 00:00:04,000\r\nSecond line\r\nwith break\r\n\r\n";

    #[test]
    fn srt_roundtrip() {
        let s = parse_srt(SRT).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].start, 1.0);
        assert_eq!(s[0].end, 2.5);
        assert_eq!(s[1].text, "Second line\nwith break");
        let out = to_srt(&s);
        assert!(out.starts_with("1\n00:00:01,000 --> 00:00:02,500\nHello <i>world</i>\n"));
        assert_eq!(parse_srt(&out).unwrap(), s);
    }

    #[test]
    fn vtt_parse_and_write() {
        let v = "WEBVTT - title\n\nNOTE a comment\n\nSTYLE\n::cue { color: red }\n\nintro\n00:01.000 --> 00:02.000 align:start position:10%\n<v Alice>Hi &amp; welcome\n\n00:00:03.500 --> 00:00:05.000\n<c.yellow>Bye</c> <00:00:04.000>now\n";
        let s = parse_vtt(v).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].speaker.as_deref(), Some("Alice"));
        assert_eq!(s[0].text, "Hi & welcome");
        assert_eq!(s[1].text, "Bye now");
        assert_eq!(s[1].start, 3.5);
        let out = to_vtt(&s);
        assert!(out
            .starts_with("WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\n<v Alice>Hi &amp; welcome"));
        assert_eq!(parse_vtt(&out).unwrap(), s);
        assert!(parse_vtt("1\n00:00:01,000 --> 00:00:02,000\nx").is_err());
    }

    #[test]
    fn json_and_detect() {
        let w = r#"{"result":{"language":"en"},"transcription":[{"timestamps":{"from":"00:00:00,000","to":"00:00:02,000"},"offsets":{"from":0,"to":2000},"text":" Hello.","speaker_turn_next":true},{"offsets":{"from":2000,"to":3500},"text":" Hi."}]}"#;
        let s = parse_json(w).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[1].end, 3.5);
        assert!(s[0].speaker_turn);
        assert_eq!(detect_format(w), "json");
        assert_eq!(detect_format(SRT), "srt");
        assert_eq!(detect_format("WEBVTT\n"), "vtt");
        assert_eq!(detect_format("just words"), "txt");
        let ours = to_json(&s, json!({}));
        assert_eq!(parse_json(&ours).unwrap(), s);
        assert!(parse_json(r#"[{"start": "x"}]"#).is_err());
        // speaker turn marks the next cue in SRT
        assert!(to_srt(&s).contains("\n- Hi."));
        let txt = to_txt(&s);
        assert_eq!(txt, "Hello.\n\nHi.\n");
    }

    #[test]
    fn shifting() {
        let s = parse_srt(SRT).unwrap();
        let t = shift(&s, -1.5, 1.0, 0.0);
        assert_eq!(t[0].start, 0.0);
        assert_eq!(t[0].end, 1.0);
        let t = shift(&s, -3.0, 1.0, 0.0);
        assert_eq!(t.len(), 1);
        let t = shift(&s, 10.0, 2.0, 2.0);
        assert_eq!(t[0].start, 1.0);
        assert_eq!(t[1].start, 16.0);
        assert!(parse_cue_time("1:2:3").is_some());
        assert_eq!(parse_cue_time("00:01:02,500"), Some(62.5));
        assert_eq!(parse_cue_time("abc"), None);
        assert_eq!(parse_cue_time("-1:00"), None);
    }

    #[test]
    fn colors() {
        assert_eq!(ass_color("#ff8800"), "&H000088FF");
    }
}
