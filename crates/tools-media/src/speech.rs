//! Offline speech tools: transcription / language detection / subtitles
//! with whisper.cpp (`whisper-cli`) and text-to-speech with Piper.

use crate::ffmpeg::*;
use crate::subtitles::{
    burn_style, burn_style_params, burn_subtitles, format_segments, mux_subtitles, parse_json,
    segments_table, Segment,
};
use dogubako_core::models::{ModelSpec, KNOWN_MODELS};
use dogubako_core::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![transcribe(), detect_language(), subtitles(), tts()]
}

/// Languages supported by Whisper (code, name), from whisper.cpp.
pub const LANGUAGES: &[(&str, &str)] = &[
    ("en", "english"),
    ("zh", "chinese"),
    ("de", "german"),
    ("es", "spanish"),
    ("ru", "russian"),
    ("ko", "korean"),
    ("fr", "french"),
    ("ja", "japanese"),
    ("pt", "portuguese"),
    ("tr", "turkish"),
    ("pl", "polish"),
    ("ca", "catalan"),
    ("nl", "dutch"),
    ("ar", "arabic"),
    ("sv", "swedish"),
    ("it", "italian"),
    ("id", "indonesian"),
    ("hi", "hindi"),
    ("fi", "finnish"),
    ("vi", "vietnamese"),
    ("he", "hebrew"),
    ("uk", "ukrainian"),
    ("el", "greek"),
    ("ms", "malay"),
    ("cs", "czech"),
    ("ro", "romanian"),
    ("da", "danish"),
    ("hu", "hungarian"),
    ("ta", "tamil"),
    ("no", "norwegian"),
    ("th", "thai"),
    ("ur", "urdu"),
    ("hr", "croatian"),
    ("bg", "bulgarian"),
    ("lt", "lithuanian"),
    ("la", "latin"),
    ("mi", "maori"),
    ("ml", "malayalam"),
    ("cy", "welsh"),
    ("sk", "slovak"),
    ("te", "telugu"),
    ("fa", "persian"),
    ("lv", "latvian"),
    ("bn", "bengali"),
    ("sr", "serbian"),
    ("az", "azerbaijani"),
    ("sl", "slovenian"),
    ("kn", "kannada"),
    ("et", "estonian"),
    ("mk", "macedonian"),
    ("br", "breton"),
    ("eu", "basque"),
    ("is", "icelandic"),
    ("hy", "armenian"),
    ("ne", "nepali"),
    ("mn", "mongolian"),
    ("bs", "bosnian"),
    ("kk", "kazakh"),
    ("sq", "albanian"),
    ("sw", "swahili"),
    ("gl", "galician"),
    ("mr", "marathi"),
    ("pa", "punjabi"),
    ("si", "sinhala"),
    ("km", "khmer"),
    ("sn", "shona"),
    ("yo", "yoruba"),
    ("so", "somali"),
    ("af", "afrikaans"),
    ("oc", "occitan"),
    ("ka", "georgian"),
    ("be", "belarusian"),
    ("tg", "tajik"),
    ("sd", "sindhi"),
    ("gu", "gujarati"),
    ("am", "amharic"),
    ("yi", "yiddish"),
    ("lo", "lao"),
    ("uz", "uzbek"),
    ("fo", "faroese"),
    ("ht", "haitian creole"),
    ("ps", "pashto"),
    ("tk", "turkmen"),
    ("nn", "nynorsk"),
    ("mt", "maltese"),
    ("sa", "sanskrit"),
    ("lb", "luxembourgish"),
    ("my", "myanmar"),
    ("bo", "tibetan"),
    ("tl", "tagalog"),
    ("mg", "malagasy"),
    ("as", "assamese"),
    ("tt", "tatar"),
    ("haw", "hawaiian"),
    ("ln", "lingala"),
    ("ha", "hausa"),
    ("ba", "bashkir"),
    ("jw", "javanese"),
    ("su", "sundanese"),
    ("yue", "cantonese"),
];

pub fn language_name(code: &str) -> Option<&'static str> {
    LANGUAGES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

/// Preference when choosing a model automatically (best first).
const AUTO_ORDER: &[&str] = &[
    "whisper-large-v3-turbo-q5_0",
    "whisper-large-v3-turbo",
    "whisper-small",
    "whisper-small.en",
    "whisper-base",
    "whisper-base.en",
    "whisper-tiny",
    "whisper-tiny.en",
    "whisper-small.en-tdrz",
];
const TDRZ: &str = "whisper-small.en-tdrz";

fn model_param() -> ParamSpec {
    let mut opts: Vec<(&str, &str)> = vec![("auto", "Best installed")];
    opts.extend(
        KNOWN_MODELS
            .iter()
            .filter(|m| m.group == "whisper")
            .map(|m| (m.id, m.name)),
    );
    ParamSpec::choice("model", &opts)
        .default_str("auto")
        .describe("Whisper model (download models in Settings → Models)")
}

fn language_param() -> ParamSpec {
    ParamSpec::text("language")
        .default_str("auto")
        .max_len(8)
        .placeholder("auto, en, de, fr…")
        .describe("Spoken language code, or 'auto' to detect")
}

fn parse_language(p: &Params) -> ToolResult<String> {
    let l = p.str_or("language", "auto").trim().to_lowercase();
    if l.is_empty() || l == "auto" {
        return Ok("auto".into());
    }
    if language_name(&l).is_none() {
        return Err(ToolError::invalid(
            "language",
            format!("'{l}' is not a Whisper language code (e.g. en, de, fr, ja)"),
        ));
    }
    Ok(l)
}

/// Pick the whisper model to use.
fn select_model(
    ctx: &ToolContext,
    choice: &str,
    language: &str,
    need_tdrz: bool,
) -> ToolResult<(&'static ModelSpec, PathBuf)> {
    let installed = ctx.models().installed_in_group("whisper");
    if need_tdrz {
        let m = installed
            .iter()
            .find(|m| m.id == TDRZ)
            .ok_or_else(|| ToolError::MissingModel {
                message: format!(
                    "speaker turn detection needs the '{TDRZ}' model (Whisper small + tinydiarize)"
                ),
            })?;
        return Ok((m, ctx.models().path(m.id).unwrap()));
    }
    if choice != "auto" {
        let m =
            installed
                .iter()
                .find(|m| m.id == choice)
                .ok_or_else(|| ToolError::MissingModel {
                    message: format!("model '{choice}' is not installed"),
                })?;
        return Ok((m, ctx.models().path(m.id).unwrap()));
    }
    let english_ok = language == "en";
    let pick = AUTO_ORDER
        .iter()
        .filter(|id| english_ok || !id.contains(".en"))
        .chain(AUTO_ORDER.iter())
        .find_map(|id| installed.iter().find(|m| m.id == *id))
        .ok_or_else(|| ToolError::MissingModel {
            message: "no Whisper model is installed".into(),
        })?;
    Ok((pick, ctx.models().path(pick.id).unwrap()))
}

fn default_threads() -> i64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i64)
        .unwrap_or(4)
        .clamp(1, 8)
}

/// Convert any media to 16 kHz WAV for whisper.cpp.
async fn to_wav16(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    stereo: bool,
    start: Option<f64>,
    len: Option<f64>,
    sp: (f32, f32),
) -> ToolResult<PathBuf> {
    let wav = ctx.temp_path(&format!(
        "whisper-in-{}.wav",
        dogubako_core::util::random_id(4)
    ));
    let mut ff = Ff::new();
    if let Some(s) = start {
        ff = ff.args(["-ss", &secs(s)]);
    }
    ff = ff.input(&f.path);
    if let Some(l) = len {
        ff = ff.args(["-t", &secs(l)]);
    }
    ff.args([
        "-map",
        "0:a:0",
        "-vn",
        "-sn",
        "-ar",
        "16000",
        "-ac",
        if stereo { "2" } else { "1" },
        "-c:a",
        "pcm_s16le",
    ])
    .out(&wav)
    .duration(len.or(pr.duration()))
    .span(sp, format!("Preparing audio of {}", f.name))
    .run(ctx)
    .await?;
    Ok(wav)
}

pub struct WhisperOpts {
    pub model: String,
    pub language: String,
    pub translate: bool,
    pub diarize: String,
    pub words: bool,
    pub threads: i64,
    pub vad: bool,
    pub prompt: Option<String>,
    pub max_len: Option<i64>,
}

impl WhisperOpts {
    fn from_params(p: &Params) -> ToolResult<Self> {
        Ok(WhisperOpts {
            model: p.str_or("model", "auto").to_string(),
            language: parse_language(p)?,
            translate: p.bool("translate"),
            diarize: p.str_or("diarize", "none").to_string(),
            words: p.bool("word_timestamps"),
            threads: p.int_or("threads", default_threads()),
            vad: p.bool("vad"),
            prompt: p.opt_str("prompt").map(str::to_string),
            max_len: p.opt_int("max_chars").filter(|v| *v > 0),
        })
    }
}

#[derive(Default)]
pub struct WhisperResult {
    pub segments: Vec<Segment>,
    pub words: Vec<Value>,
    pub language: Option<String>,
    pub model: String,
    pub warnings: Vec<String>,
}

/// Parse whisper.cpp `-oj` / `-ojf` output.
pub fn parse_whisper_json(
    raw: &[u8],
    want_words: bool,
) -> ToolResult<(Vec<Segment>, Vec<Value>, Option<String>)> {
    let text = String::from_utf8_lossy(raw);
    let v: Value = serde_json::from_str(&text).ctx("parsing whisper JSON")?;
    let segs = parse_json(&text).map_err(ToolError::failed)?;
    let lang = v["result"]["language"].as_str().map(String::from);
    let mut words = vec![];
    if want_words {
        for s in v["transcription"].as_array().into_iter().flatten() {
            let mut cur: Option<(String, i64, i64)> = None;
            for t in s["tokens"].as_array().into_iter().flatten() {
                let tx = t["text"].as_str().unwrap_or("");
                if tx.starts_with("[_") || tx.starts_with("<|") || tx.is_empty() {
                    continue;
                }
                let (from, to) = (
                    t["offsets"]["from"].as_i64().unwrap_or(0),
                    t["offsets"]["to"].as_i64().unwrap_or(0),
                );
                let starts_word = tx.starts_with(' ') || cur.is_none();
                match (&mut cur, starts_word) {
                    (Some(c), false) => {
                        c.0.push_str(tx);
                        c.2 = to;
                    }
                    _ => {
                        if let Some((w, a, b)) = cur.take() {
                            words.push(json!({"text": w.trim(), "start": a as f64 / 1000.0, "end": b as f64 / 1000.0}));
                        }
                        cur = Some((tx.to_string(), from, to));
                    }
                }
            }
            if let Some((w, a, b)) = cur.take() {
                words.push(
                    json!({"text": w.trim(), "start": a as f64 / 1000.0, "end": b as f64 / 1000.0}),
                );
            }
        }
    }
    Ok((segs, words, lang))
}

pub async fn run_whisper(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    o: &WhisperOpts,
    sp: (f32, f32),
) -> ToolResult<WhisperResult> {
    let tdrz = o.diarize == "tinydiarize";
    let stereo = o.diarize == "stereo";
    let mut res = WhisperResult::default();
    if stereo && pr.channels().unwrap_or(1) < 2 {
        return Err(ToolError::invalid(
            "diarize",
            "stereo speaker separation needs a stereo recording (one speaker per channel)",
        ));
    }
    let (model, model_path) = select_model(ctx, &o.model, &o.language, tdrz)?;
    res.model = model.id.to_string();
    if model.languages == "en" && o.language != "en" && o.language != "auto" {
        res.warnings.push(format!(
            "{} is English-only; the language setting is ignored",
            model.name
        ));
    }
    let wav = to_wav16(ctx, f, pr, stereo, None, None, sub(sp, 0.0, 0.08)).await?;
    let out_base = ctx.temp_path(&format!(
        "whisper-out-{}",
        dogubako_core::util::random_id(4)
    ));
    let mut cmd = ctx
        .command("whisper")?
        .arg("-m")
        .arg(&model_path)
        .arg("-f")
        .arg(arg_path(&wav))
        .arg("-of")
        .arg(arg_path(&out_base))
        .arg(if o.words { "-ojf" } else { "-oj" })
        .args([
            "-l",
            &o.language,
            "-t",
            &o.threads.clamp(1, 64).to_string(),
            "-pp",
        ])
        .readable(ctx.models_dir());
    if o.translate {
        cmd = cmd.arg("-tr");
    }
    if tdrz {
        cmd = cmd.arg("-tdrz");
    }
    if stereo {
        cmd = cmd.arg("-di");
    }
    if let Some(n) = o.max_len {
        cmd = cmd.args(["-ml", &n.to_string(), "-sow"]);
    }
    if let Some(pr) = &o.prompt {
        cmd = cmd.arg("--prompt").arg(pr);
    }
    if o.vad {
        match ctx.models().path("silero-vad") {
            Some(vm) => cmd = cmd.arg("--vad").arg("-vm").arg(vm),
            None => res.warnings.push("VAD requested but the 'silero-vad' model is not installed; transcribed without VAD".into()),
        }
    }
    let c = ctx.clone();
    let (lo, hi) = sub(sp, 0.08, 0.97);
    let name = f.name.clone();
    let detected = Arc::new(Mutex::new(None::<String>));
    let d2 = detected.clone();
    cmd = cmd.on_stderr_line(move |line| {
        if let Some(i) = line.find("progress =") {
            if let Ok(v) = line[i + 10..]
                .trim()
                .trim_end_matches('%')
                .trim()
                .parse::<f32>()
            {
                let f = (v / 100.0).clamp(0.0, 1.0);
                c.progress(
                    lo + (hi - lo) * f,
                    format!("Transcribing {name} – {:.0}%", f * 100.0),
                );
            }
        } else if let Some((lang, _)) = parse_detected(line) {
            *d2.lock().unwrap() = Some(lang);
        }
    });
    cmd.run().await?;
    let json_path = PathBuf::from(format!("{}.json", out_base.to_string_lossy()));
    let raw = std::fs::read(&json_path)
        .map_err(|e| ToolError::failed(format!("whisper produced no JSON output: {e}")))?;
    let (segs, words, lang) = parse_whisper_json(&raw, o.words)?;
    res.segments = segs
        .into_iter()
        .filter(|s| !s.text.trim().is_empty())
        .collect();
    res.words = words;
    res.language = detected.lock().unwrap().clone().or(lang);
    let _ = std::fs::remove_file(&wav);
    Ok(res)
}

/// `auto-detected language: en (p = 0.97)` → ("en", 0.97)
pub fn parse_detected(line: &str) -> Option<(String, f64)> {
    let i = line.find("auto-detected language:")?;
    let rest = line[i + 23..].trim();
    let code = rest.split_whitespace().next()?.to_string();
    let p = rest
        .split("p =")
        .nth(1)
        .and_then(|s| s.trim().trim_end_matches(')').trim().parse().ok())
        .unwrap_or(0.0);
    Some((code, p))
}

fn whisper_params(spec: ToolSpec) -> ToolSpec {
    spec.param(model_param())
        .param(language_param())
        .param(ParamSpec::boolean("translate").describe("Translate the speech to English"))
        .param(
            ParamSpec::boolean("vad")
                .describe("Skip non-speech with Silero VAD (needs the 'silero-vad' model)")
                .advanced(),
        )
        .param(
            ParamSpec::textarea("prompt")
                .max_len(2000)
                .describe("Initial prompt: names, spelling or style hints")
                .advanced(),
        )
        .param(
            ParamSpec::integer("threads")
                .range(1.0, 64.0)
                .default_int(default_threads())
                .advanced(),
        )
}

fn whisper_spec(id: &str, name: &str) -> ToolSpec {
    ToolSpec::new(id, name, Category::Speech)
        .requires(&["whisper", "ffmpeg", "ffprobe"])
        .requires_model("whisper")
        .progress()
        .timeout(6 * 3600)
}

fn plain_text(segs: &[Segment]) -> String {
    crate::subtitles::to_txt(segs)
}

// ---------------------------------------------------------------------------

fn transcribe() -> Arc<dyn Tool> {
    let spec = whisper_params(
        whisper_spec("speech.transcribe", "Transcribe Audio / Video")
            .describe("Offline speech-to-text with whisper.cpp: transcribe or translate audio and video to text, SRT, WebVTT, JSON or Markdown, with optional word timestamps, speaker turns and VAD")
            .keywords(&["speech to text", "whisper", "transcription", "dictation", "captions", "subtitles", "stt", "asr", "meeting notes", "interview"])
            .param(ParamSpec::files("input").label("Audio / video").accept(MEDIA_ACCEPT).required().primary())
            .param(ParamSpec::multi_choice("formats", &[("txt", "Text"), ("srt", "SRT"), ("vtt", "WebVTT"), ("json", "JSON"), ("md", "Markdown")]).default_value(json!(["txt", "srt"]))),
    )
    .param(ParamSpec::boolean("word_timestamps").describe("Include per-word timings in the JSON / data").advanced())
    .param(ParamSpec::choice("diarize", &[("none", "Off"), ("tinydiarize", "Speaker turns (tinydiarize model)"), ("stereo", "Stereo: one speaker per channel")]).default_str("none").advanced())
    .text_output("text/plain");
    async_tool(spec, |ctx, p| async move {
        let o = WhisperOpts::from_params(&p)?;
        let mut formats = p.list("formats");
        if formats.is_empty() {
            formats.push("txt".into());
        }
        let files = p.files("input").to_vec();
        let mut artifacts = vec![];
        let mut warnings = vec![];
        let mut all_text = String::new();
        let mut items = vec![];
        let mut last: Option<WhisperResult> = None;
        for (i, f) in files.iter().enumerate() {
            let pr = probe_audio(&ctx, f).await?;
            let r = run_whisper(&ctx, f, &pr, &o, span(i, files.len())).await?;
            if r.segments.is_empty() {
                warnings.push(format!("{}: no speech was recognized", f.name));
            }
            let stem = f.stem().replace('%', "_");
            for fmt in &formats {
                let (body, ext, _) = if fmt == "json" {
                    let extra = json!({"language": r.language, "model": r.model, "words": if o.words { Value::Array(r.words.clone()) } else { Value::Null }});
                    (crate::subtitles::to_json(&r.segments, extra), "json", "")
                } else {
                    format_segments(&r.segments, fmt, &format!("Transcript: {}", f.name))
                };
                artifacts.push(ctx.write_output(&format!("{stem}.{ext}"), body.as_bytes())?);
            }
            let txt = plain_text(&r.segments);
            if files.len() > 1 {
                all_text.push_str(&format!("## {}\n\n", f.name));
            }
            all_text.push_str(&txt);
            all_text.push('\n');
            warnings.extend(r.warnings.iter().cloned());
            items.push(json!({"file": f.name, "language": r.language, "model": r.model, "segments": r.segments, "duration": pr.duration()}));
            last = Some(r);
        }
        let mut data = json!({"files": items});
        if let (1, Some(r)) = (files.len(), last) {
            data = json!({
                "language": r.language,
                "language_name": r.language.as_deref().and_then(language_name),
                "model": r.model,
                "segments": r.segments,
                "table": segments_table(&r.segments),
            });
            if o.words {
                data["words"] = Value::Array(r.words);
            }
        }
        let mut out = ToolOutput::text(all_text.trim_end().to_string())
            .with_artifacts(artifacts)
            .with_data(data);
        out.warnings = warnings;
        Ok(out)
    })
}

fn detect_language() -> Arc<dyn Tool> {
    let spec = whisper_spec("speech.detect_language", "Detect Spoken Language")
        .describe("Identify the language spoken in audio or video with Whisper (analyses 30 seconds from the chosen offset)")
        .keywords(&["language identification", "what language", "whisper", "lid", "spoken language"])
        .param(audio_input(false).label("Audio / video"))
        .param(model_param())
        .param(ParamSpec::text("offset").placeholder("0:00").describe("Where to start listening"))
        .param(ParamSpec::integer("threads").range(1.0, 64.0).default_int(default_threads()).advanced())
        .output(OutputKind::Data);
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let pr = probe_audio(&ctx, &f).await?;
        let (model, path) = select_model(&ctx, p.str_or("model", "auto"), "auto", false)?;
        if model.languages == "en" {
            return Err(ToolError::invalid(
                "model",
                format!(
                    "{} is English-only and cannot detect languages; choose a multilingual model",
                    model.name
                ),
            ));
        }
        let offset = opt_time(&p, "offset")?.unwrap_or(0.0);
        if pr.duration().map(|d| offset >= d).unwrap_or(false) {
            return Err(ToolError::invalid("offset", "offset is beyond the end"));
        }
        let wav = to_wav16(&ctx, &f, &pr, false, Some(offset), Some(30.0), (0.0, 0.3)).await?;
        ctx.progress(0.35, "Detecting language");
        let out = ctx
            .command("whisper")?
            .arg("-m")
            .arg(&path)
            .arg("-f")
            .arg(arg_path(&wav))
            .args([
                "-dl",
                "-l",
                "auto",
                "-t",
                &p.int_or("threads", default_threads()).to_string(),
            ])
            .readable(ctx.models_dir())
            .timeout(Duration::from_secs(1800))
            .run()
            .await?;
        let (code, prob) = out
            .stderr
            .lines()
            .find_map(parse_detected)
            .ok_or_else(|| ToolError::failed("whisper did not report a language"))?;
        let name = language_name(&code).unwrap_or("unknown");
        Ok(ToolOutput::text(format!("{name} ({code}), p = {prob:.2}")).with_data(json!({"language": code, "language_name": name, "probability": prob, "model": model.id})))
    })
}

fn subtitles() -> Arc<dyn Tool> {
    let spec = burn_style_params(
        whisper_params(
            whisper_spec("speech.subtitles", "Generate Subtitles")
                .describe("Automatically create SRT/WebVTT subtitles from speech with whisper.cpp, optionally adding them to the video as a soft track or burning them in")
                .keywords(&["auto subtitles", "captions", "closed captions", "srt", "vtt", "whisper", "accessibility", "auto caption"])
                .param(ParamSpec::files("input").label("Video / audio").accept(MEDIA_ACCEPT).required().primary())
                .param(ParamSpec::choice("format", &[("srt", "SRT"), ("vtt", "WebVTT"), ("both", "SRT + WebVTT")]).default_str("srt"))
                .param(ParamSpec::integer("max_chars").label("Max characters per subtitle").range(0.0, 200.0).default_int(42).describe("0 = whisper's natural segments"))
                .param(ParamSpec::choice("embed", &[("none", "Subtitle file only"), ("soft", "Add to video as a track"), ("burn", "Burn into the video")]).default_str("none")),
        ),
        Some(("embed", &["burn"])),
    )
    .param(quality_param())
    .output(OutputKind::Files);
    async_tool(spec, |ctx, p| async move {
        let mut o = WhisperOpts::from_params(&p)?;
        o.max_len = p.opt_int("max_chars").filter(|v| *v > 0);
        let files = p.files("input").to_vec();
        let embed = p.str("embed")?.to_string();
        let mut artifacts = vec![];
        let mut warnings = vec![];
        let mut items = vec![];
        for (i, f) in files.iter().enumerate() {
            let sp = span(i, files.len());
            let pr = probe_audio(&ctx, f).await?;
            let r = run_whisper(
                &ctx,
                f,
                &pr,
                &o,
                if embed == "none" {
                    sp
                } else {
                    sub(sp, 0.0, 0.7)
                },
            )
            .await?;
            warnings.extend(r.warnings.iter().cloned());
            if r.segments.is_empty() {
                warnings.push(format!("{}: no speech was recognized", f.name));
            }
            let stem = f.stem().replace('%', "_");
            let fmts: &[&str] = match p.str("format")? {
                "vtt" => &["vtt"],
                "both" => &["srt", "vtt"],
                _ => &["srt"],
            };
            let lang = r.language.clone().filter(|l| l != "auto").map(|l| {
                if o.translate {
                    "en".to_string()
                } else {
                    l
                }
            });
            let suffix = lang.as_deref().map(|l| format!(".{l}")).unwrap_or_default();
            let mut first_sub = None;
            for fmt in fmts {
                let (body, ext, _) = format_segments(&r.segments, fmt, "");
                let a = ctx.write_output(&format!("{stem}{suffix}.{ext}"), body.as_bytes())?;
                if first_sub.is_none() {
                    first_sub = Some(a.path.clone());
                }
                artifacts.push(a);
            }
            if embed != "none" {
                if pr.video().is_none() {
                    warnings.push(format!(
                        "{}: not a video; subtitles were saved as a file only",
                        f.name
                    ));
                } else if r.segments.is_empty() {
                    warnings.push(format!("{}: nothing to embed", f.name));
                } else if let Some(sub_path) = &first_sub {
                    let out = if embed == "soft" {
                        let fmt = match same_container(f) {
                            "avi" => "mkv",
                            c => c,
                        };
                        let out = out_path(&ctx, f, "subtitled", fmt);
                        let lang3 = lang.as_deref().map(iso639_2).map(String::from);
                        mux_subtitles(
                            &ctx,
                            &f.path,
                            &pr,
                            &[(sub_path.clone(), lang3)],
                            fmt,
                            true,
                            &out,
                            sub(sp, 0.7, 1.0),
                        )
                        .await?;
                        out
                    } else {
                        burn_subtitles(
                            &ctx,
                            f,
                            &pr,
                            &p,
                            Some(sub_path),
                            None,
                            &burn_style(&p),
                            None,
                            sub(sp, 0.7, 1.0),
                            &mut warnings,
                        )
                        .await?
                    };
                    artifacts.push(ctx.artifact(&out)?);
                }
            }
            items.push(json!({"file": f.name, "language": r.language, "model": r.model, "segments": r.segments.len()}));
        }
        let mut out = ToolOutput::files(artifacts).with_data(json!({"files": items}));
        out.warnings = warnings;
        Ok(out)
    })
}

/// ISO 639-1 → 639-2 (as used in MKV/MP4 language tags) for common languages.
fn iso639_2(code: &str) -> &str {
    match code {
        "en" => "eng",
        "de" => "deu",
        "fr" => "fra",
        "es" => "spa",
        "it" => "ita",
        "pt" => "por",
        "nl" => "nld",
        "ru" => "rus",
        "zh" => "zho",
        "ja" => "jpn",
        "ko" => "kor",
        "ar" => "ara",
        "pl" => "pol",
        "tr" => "tur",
        "sv" => "swe",
        "da" => "dan",
        "no" => "nor",
        "fi" => "fin",
        "cs" => "ces",
        "el" => "ell",
        "he" => "heb",
        "hi" => "hin",
        "uk" => "ukr",
        "hu" => "hun",
        "ro" => "ron",
        "vi" => "vie",
        "id" => "ind",
        "th" => "tha",
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Piper TTS

fn tts() -> Arc<dyn Tool> {
    let mut voices: Vec<(&str, &str)> = vec![("auto", "First installed voice")];
    voices.extend(
        KNOWN_MODELS
            .iter()
            .filter(|m| m.group == "piper")
            .map(|m| (m.id, m.name)),
    );
    let spec = ToolSpec::new("speech.tts", "Text to Speech", Category::Speech)
        .requires(&["piper"])
        .requires_model("piper")
        .output(OutputKind::Files)
        .timeout(1800)
        .describe("Turn text into natural-sounding speech offline with Piper neural voices; save as WAV, MP3, Ogg, Opus, FLAC or M4A")
        .keywords(&["tts", "read aloud", "voice", "narration", "speech synthesis", "piper", "audiobook"])
        .param(ParamSpec::textarea("text").required().primary().max_len(200_000))
        .param(ParamSpec::choice("voice", &voices).default_str("auto"))
        .param(ParamSpec::number("speed").range(0.25, 4.0).step(0.05).default_num(1.0))
        .param(ParamSpec::number("sentence_pause").label("Pause between sentences (s)").range(0.0, 5.0).default_num(0.2).advanced())
        .param(ParamSpec::integer("speaker").range(0.0, 1000.0).describe("Speaker id for multi-speaker voices").advanced())
        .param(ParamSpec::choice("format", &[("wav", "WAV"), ("mp3", "MP3"), ("ogg", "Ogg Vorbis"), ("opus", "Opus"), ("flac", "FLAC"), ("m4a", "AAC (M4A)")]).default_str("wav"));
    async_tool(spec, |ctx, p| async move {
        let text = p.text("text").trim().to_string();
        if text.is_empty() {
            return Err(ToolError::invalid("text", "enter some text"));
        }
        let installed = ctx.models().installed_in_group("piper");
        let voice = match p.str_or("voice", "auto") {
            "auto" => installed
                .first()
                .copied()
                .ok_or_else(|| ToolError::MissingModel {
                    message: "no Piper voice is installed".into(),
                })?,
            id => installed
                .iter()
                .copied()
                .find(|m| m.id == id)
                .ok_or_else(|| ToolError::MissingModel {
                    message: format!("voice '{id}' is not installed"),
                })?,
        };
        let model = ctx
            .models()
            .path(voice.id)
            .ok_or_else(|| ToolError::MissingModel {
                message: format!("voice '{}' is not installed", voice.id),
            })?;
        let config = voice
            .extra
            .first()
            .map(|(_, f)| ctx.models_dir().join(f))
            .unwrap_or_else(|| PathBuf::from(format!("{}.json", model.to_string_lossy())));
        let wav = ctx.temp_path("tts.wav");
        let speed = p.num_or("speed", 1.0);
        ctx.message(format!("Synthesizing with {}", voice.name));
        let mut cmd = ctx
            .command("piper")?
            .arg("-m")
            .arg(&model)
            .arg("-c")
            .arg(&config)
            .arg("-f")
            .arg(arg_path(&wav))
            .args([
                "--length-scale",
                &format!("{:.4}", 1.0 / speed),
                "--sentence-silence",
                &format!("{:.3}", p.num_or("sentence_pause", 0.2)),
            ])
            .readable(ctx.models_dir())
            .stdin(text.replace("\r\n", "\n").into_bytes());
        if let Some(s) = p.opt_int("speaker") {
            cmd = cmd.args(["-s", &s.to_string()]);
        }
        cmd.run().await?;
        if !wav.is_file() {
            return Err(ToolError::failed("piper produced no audio"));
        }
        let duration = wav_duration(&wav);
        let fmt = p.str("format")?;
        let base = text
            .split_whitespace()
            .take(6)
            .collect::<Vec<_>>()
            .join("-")
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-')
            .take(40)
            .collect::<String>();
        let base = if base.is_empty() {
            "speech".to_string()
        } else {
            base.to_lowercase()
        };
        let out = if fmt == "wav" {
            let dest = ctx.output_path(&format!("{base}.wav"));
            std::fs::rename(&wav, &dest).or_else(|_| std::fs::copy(&wav, &dest).map(|_| ()))?;
            dest
        } else {
            let dest = ctx.output_path(&format!("{base}.{fmt}"));
            Ff::new()
                .input(&wav)
                .args(audio_args_for_ext(fmt, None))
                .out(&dest)
                .duration(duration)
                .span((0.8, 1.0), "Encoding")
                .run(&ctx)
                .await?;
            dest
        };
        Ok(ToolOutput::files(vec![ctx.artifact(&out)?]).with_data(
            json!({"voice": voice.id, "duration": duration, "characters": text.chars().count()}),
        ))
    })
}

/// Duration of a PCM WAV file from its header.
pub fn wav_duration(path: &Path) -> Option<f64> {
    let b = std::fs::read(path).ok()?;
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return None;
    }
    let mut i = 12;
    let mut byte_rate = None;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let size = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        if id == b"fmt " && i + 20 <= b.len() {
            byte_rate = Some(u32::from_le_bytes([
                b[i + 16],
                b[i + 17],
                b[i + 18],
                b[i + 19],
            ]));
        }
        if id == b"data" {
            let len = size.min(b.len() - i - 8);
            return byte_rate.filter(|r| *r > 0).map(|r| len as f64 / r as f64);
        }
        i += 8 + size + (size & 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_line() {
        assert_eq!(
            parse_detected("whisper_full_with_state: auto-detected language: de (p = 0.912345)"),
            Some(("de".into(), 0.912345))
        );
        assert_eq!(parse_detected("nothing"), None);
    }

    #[test]
    fn whisper_json_words() {
        let raw = br#"{"result":{"language":"en"},"transcription":[{"offsets":{"from":0,"to":1500},"text":" Hello world","tokens":[
            {"text":"[_BEG_]","offsets":{"from":0,"to":0}},
            {"text":" Hel","offsets":{"from":0,"to":300}},
            {"text":"lo","offsets":{"from":300,"to":600}},
            {"text":" world","offsets":{"from":700,"to":1500}},
            {"text":"[_TT_75]","offsets":{"from":1500,"to":1500}}]}]}"#;
        let (segs, words, lang) = parse_whisper_json(raw, true).unwrap();
        assert_eq!(lang.as_deref(), Some("en"));
        assert_eq!(segs[0].text, "Hello world");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0]["text"], "Hello");
        assert_eq!(words[0]["end"], 0.6);
        assert_eq!(words[1]["start"], 0.7);
    }

    #[test]
    fn wav_header() {
        let dir = std::env::temp_dir().join(format!("dgb-wav-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.wav");
        let mut b = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        b.extend(16u32.to_le_bytes());
        b.extend([1, 0, 1, 0]);
        b.extend(16000u32.to_le_bytes());
        b.extend(32000u32.to_le_bytes());
        b.extend([2, 0, 16, 0]);
        b.extend(b"data");
        b.extend(16000u32.to_le_bytes());
        b.extend(vec![0u8; 16000]);
        std::fs::write(&p, b).unwrap();
        assert_eq!(wav_duration(&p), Some(0.5));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn languages() {
        assert_eq!(language_name("yue"), Some("cantonese"));
        assert_eq!(LANGUAGES.len(), 100);
    }
}
