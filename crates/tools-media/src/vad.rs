//! Voice activity detection with the WebRTC VAD (libfvad, via the
//! `webrtc-vad` crate) on 16 kHz mono PCM decoded by FFmpeg.

use crate::audio::decode_pcm;
use crate::ffmpeg::*;
use crate::subtitles::{to_srt, Segment};
use dogubako_core::prelude::*;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![vad_tool(), extract_segments(), split_by_speech()]
}

pub const RATE: u32 = 16_000;
/// Decode long inputs in windows of this many seconds.
const WINDOW: f64 = 600.0;

#[derive(Debug, Clone, Copy)]
pub struct VadParams {
    pub aggressiveness: u8,
    pub frame_ms: u32,
    pub min_speech_ms: u32,
    pub min_silence_ms: u32,
    pub padding_ms: u32,
}

impl Default for VadParams {
    fn default() -> Self {
        VadParams {
            aggressiveness: 2,
            frame_ms: 30,
            min_speech_ms: 250,
            min_silence_ms: 300,
            padding_ms: 200,
        }
    }
}

impl VadParams {
    pub fn from_params(p: &Params) -> Self {
        VadParams {
            aggressiveness: p.int_or("aggressiveness", 2).clamp(0, 3) as u8,
            frame_ms: p.str_or("frame_ms", "30").parse().unwrap_or(30),
            min_speech_ms: p.int_or("min_speech_ms", 250).max(0) as u32,
            min_silence_ms: p.int_or("min_silence_ms", 300).max(0) as u32,
            padding_ms: p.int_or("padding_ms", 200).max(0) as u32,
        }
    }
}

/// Classify consecutive frames of 16 kHz PCM as voiced / unvoiced.
pub fn classify(pcm: &[i16], aggressiveness: u8, frame_ms: u32) -> Vec<bool> {
    use webrtc_vad::{SampleRate, Vad, VadMode};
    let mode = match aggressiveness {
        0 => VadMode::Quality,
        1 => VadMode::LowBitrate,
        2 => VadMode::Aggressive,
        _ => VadMode::VeryAggressive,
    };
    let mut vad = Vad::new_with_rate_and_mode(SampleRate::Rate16kHz, mode);
    let n = (RATE * frame_ms / 1000) as usize;
    pcm.chunks_exact(n)
        .map(|f| vad.is_voice_segment(f).unwrap_or(false))
        .collect()
}

/// Turn frame flags into speech segments (seconds): merge short gaps, drop
/// short bursts, then pad and merge overlaps.
pub fn frames_to_segments(
    flags: &[bool],
    frame_ms: u32,
    p: &VadParams,
    total: f64,
) -> Vec<(f64, f64)> {
    let fs = frame_ms as f64 / 1000.0;
    let mut segs: Vec<(f64, f64)> = vec![];
    let mut start: Option<usize> = None;
    for (i, &v) in flags.iter().chain(std::iter::once(&false)).enumerate() {
        match (v, start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                segs.push((s as f64 * fs, i as f64 * fs));
                start = None;
            }
            _ => {}
        }
    }
    let min_sil = p.min_silence_ms as f64 / 1000.0;
    let mut merged: Vec<(f64, f64)> = vec![];
    for s in segs {
        match merged.last_mut() {
            Some(last) if s.0 - last.1 < min_sil => last.1 = s.1,
            _ => merged.push(s),
        }
    }
    let min_sp = p.min_speech_ms as f64 / 1000.0;
    merged.retain(|(a, b)| b - a >= min_sp);
    let pad = p.padding_ms as f64 / 1000.0;
    let mut out: Vec<(f64, f64)> = vec![];
    for (a, b) in merged {
        let (a, b) = ((a - pad).max(0.0), (b + pad).min(total.max(b)));
        match out.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out.into_iter()
        .map(|(a, b)| (round_ms(a), round_ms(b)))
        .collect()
}

fn round_ms(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// Run VAD over a whole file, decoding it in windows.
pub async fn detect_speech(
    ctx: &ToolContext,
    f: &InputFile,
    pr: &Probe,
    vp: &VadParams,
    sp: (f32, f32),
) -> ToolResult<Vec<(f64, f64)>> {
    let dur = pr.duration().ok_or_else(|| {
        ToolError::input(format!("could not determine the duration of '{}'", f.name))
    })?;
    let mut flags = vec![];
    let windows = (dur / WINDOW).ceil().max(1.0) as usize;
    for w in 0..windows {
        ctx.check_cancelled()?;
        let start = w as f64 * WINDOW;
        let pcm = decode_pcm(
            ctx,
            &f.path,
            RATE,
            if start > 0.0 { Some(start) } else { None },
            Some(WINDOW),
        )
        .await?;
        let (a, fm) = (vp.aggressiveness, vp.frame_ms);
        let mut part = ctx.blocking(move || Ok(classify(&pcm, a, fm))).await?;
        // Keep frame alignment exact across windows.
        let expected = (WINDOW * 1000.0 / fm as f64) as usize;
        if w + 1 < windows && part.len() < expected {
            part.resize(expected, false);
        }
        flags.extend(part);
        ctx.progress(
            sp.0 + (sp.1 - sp.0) * (w + 1) as f32 / windows as f32,
            format!("Detecting speech in {}", f.name),
        );
    }
    Ok(frames_to_segments(&flags, vp.frame_ms, vp, dur))
}

pub fn vad_params() -> Vec<ParamSpec> {
    vec![
        ParamSpec::integer("aggressiveness")
            .range(0.0, 3.0)
            .default_int(2)
            .describe("0 = keep more audio as speech, 3 = strictest filtering of non-speech"),
        ParamSpec::integer("min_speech_ms")
            .label("Minimum speech (ms)")
            .range(0.0, 10000.0)
            .default_int(250)
            .advanced(),
        ParamSpec::integer("min_silence_ms")
            .label("Minimum silence (ms)")
            .range(0.0, 10000.0)
            .default_int(300)
            .describe("Shorter pauses are merged into the surrounding speech")
            .advanced(),
        ParamSpec::integer("padding_ms")
            .label("Padding (ms)")
            .range(0.0, 5000.0)
            .default_int(200)
            .describe("Extra audio kept around each speech segment")
            .advanced(),
        ParamSpec::choice_of("frame_ms", &["10", "20", "30"])
            .default_str("30")
            .describe("Analysis frame length (ms)")
            .advanced(),
    ]
}

fn seg_table(segs: &[(f64, f64)]) -> Value {
    let rows: Vec<Value> = segs
        .iter()
        .enumerate()
        .map(|(i, (a, b))| json!([i + 1, hms(*a), hms(*b), format!("{:.2}", b - a)]))
        .collect();
    json!({"columns": ["#", "Start", "End", "Duration (s)"], "rows": rows})
}

fn speech_spec(id: &str, name: &str) -> ToolSpec {
    ToolSpec::new(id, name, Category::Speech)
        .requires(FF)
        .progress()
        .timeout(4 * 3600)
}

// ---------------------------------------------------------------------------

fn vad_tool() -> Arc<dyn Tool> {
    let spec = speech_spec("speech.vad", "Voice Activity Detection")
        .describe("Find where people are speaking in audio or video using the WebRTC voice activity detector; returns speech segments and can export JSON, CSV, SRT or Audacity labels")
        .keywords(&["vad", "speech detection", "voice detection", "speech segments", "webrtc", "talking", "non-speech"])
        .output(OutputKind::Data)
        .param(audio_input(false))
        .params(vad_params())
        .param(ParamSpec::choice("export", &[("none", "None"), ("json", "JSON"), ("csv", "CSV"), ("srt", "SRT (one cue per segment)"), ("audacity", "Audacity labels")]).default_str("none"));
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let pr = probe_audio(&ctx, &f).await?;
        let vp = VadParams::from_params(&p);
        let segs = detect_speech(&ctx, &f, &pr, &vp, (0.0, 0.95)).await?;
        let dur = pr.duration().unwrap_or(0.0);
        let speech: f64 = segs.iter().map(|(a, b)| b - a).sum();
        let items: Vec<Value> = segs
            .iter()
            .map(|(a, b)| json!({"start": a, "end": b, "duration": round_ms(b - a)}))
            .collect();
        let stem = f.stem().replace('%', "_");
        let mut out = ToolOutput::text(format!(
            "{} speech segment(s), {:.1} s of speech ({:.1}% of {})",
            segs.len(),
            speech,
            speech * 100.0 / dur.max(0.001),
            hms(dur)
        ));
        match p.str("export")? {
            "json" => {
                out = out.with_artifact(ctx.write_output(
                    &format!("{stem}-speech.json"),
                    serde_json::to_string_pretty(&json!({"segments": items}))?.as_bytes(),
                )?)
            }
            "csv" => {
                let mut s = String::from("start,end,duration\n");
                for (a, b) in &segs {
                    s.push_str(&format!("{a:.3},{b:.3},{:.3}\n", b - a));
                }
                out = out
                    .with_artifact(ctx.write_output(&format!("{stem}-speech.csv"), s.as_bytes())?);
            }
            "srt" => {
                let cues: Vec<Segment> = segs
                    .iter()
                    .enumerate()
                    .map(|(i, (a, b))| Segment {
                        start: *a,
                        end: *b,
                        text: format!("Speech {}", i + 1),
                        ..Default::default()
                    })
                    .collect();
                out = out.with_artifact(
                    ctx.write_output(&format!("{stem}-speech.srt"), to_srt(&cues).as_bytes())?,
                );
            }
            "audacity" => {
                let s: String = segs
                    .iter()
                    .enumerate()
                    .map(|(i, (a, b))| format!("{a:.6}\t{b:.6}\tspeech {}\n", i + 1))
                    .collect();
                out = out
                    .with_artifact(ctx.write_output(&format!("{stem}-labels.txt"), s.as_bytes())?);
            }
            _ => {}
        }
        Ok(out.with_data(json!({
            "segments": items,
            "count": segs.len(),
            "speech_seconds": round_ms(speech),
            "duration": dur,
            "speech_ratio": if dur > 0.0 { (speech / dur * 1000.0).round() / 1000.0 } else { 0.0 },
            "table": seg_table(&segs),
        })))
    })
}

fn out_format_param() -> ParamSpec {
    ParamSpec::choice(
        "format",
        &[
            ("wav", "WAV"),
            ("flac", "FLAC"),
            ("mp3", "MP3"),
            ("m4a", "AAC (M4A)"),
            ("opus", "Opus"),
        ],
    )
    .default_str("wav")
}

fn extract_segments() -> Arc<dyn Tool> {
    let spec = speech_spec("speech.extract_segments", "Extract Speech Segments")
        .describe("Detect speech with VAD and save each speech segment as its own file, or join all speech into one file (removes non-speech)")
        .keywords(&["remove non-speech", "speech only", "cut pauses", "vad", "utterances", "dataset", "clips"])
        .output(OutputKind::Files)
        .param(audio_input(false))
        .param(ParamSpec::choice("mode", &[("concat", "One file with speech only"), ("files", "One file per segment")]).default_str("concat"))
        .param(out_format_param())
        .param(ParamSpec::boolean("zip").describe("Return segment files as a ZIP archive").show_if("mode", &["files"]))
        .params(vad_params());
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let pr = probe_audio(&ctx, &f).await?;
        let vp = VadParams::from_params(&p);
        let segs = detect_speech(&ctx, &f, &pr, &vp, (0.0, 0.5)).await?;
        if segs.is_empty() {
            return Err(ToolError::input(
                "no speech was detected (try a lower aggressiveness)",
            ));
        }
        let fmt = p.str("format")?.to_string();
        let speech: f64 = segs.iter().map(|(a, b)| b - a).sum();
        let stem = f.stem().replace('%', "_");
        let data = json!({"segments": segs.iter().map(|(a, b)| json!({"start": a, "end": b})).collect::<Vec<_>>(), "count": segs.len(), "speech_seconds": round_ms(speech), "table": seg_table(&segs)});
        if p.str("mode")? == "concat" {
            let expr: Vec<String> = segs
                .iter()
                .map(|(a, b)| format!("between(t,{a:.3},{b:.3})"))
                .collect();
            // Re-chunk into small frames first: aselect keeps or drops whole
            // frames, and decoder frame sizes differ between FFmpeg builds
            // (large PCM frames would let silence leak into the output).
            let af = format!(
                "asetnsamples=n=256:p=0,aselect='{}',asetpts=N/SR/TB",
                expr.join("+")
            );
            let out = out_path(&ctx, &f, "speech", &fmt);
            Ff::new()
                .input(&f.path)
                .args(["-map", "0:a:0", "-vn", "-sn", "-af", &af])
                .args(audio_args_for_ext(&fmt, Some(&pr)))
                .out(&out)
                .duration(Some(speech))
                .span((0.5, 1.0), "Joining speech")
                .run(&ctx)
                .await?;
            return Ok(ToolOutput::files(vec![ctx.artifact(&out)?]).with_data(data));
        }
        if segs.len() > 5000 {
            return Err(ToolError::limit(
                "more than 5000 segments; increase the minimum silence",
            ));
        }
        let dir = ctx.temp_dir("segments")?;
        let mut paths = vec![];
        for (k, (a, b)) in segs.iter().enumerate() {
            let path = dir.join(format!(
                "{stem}-{:04}-{}.{fmt}",
                k + 1,
                hms(*a).replace(':', "-")
            ));
            Ff::new()
                .args(["-ss", &secs(*a)])
                .input(&f.path)
                .args(["-t", &secs(b - a), "-map", "0:a:0", "-vn", "-sn"])
                .args(audio_args_for_ext(&fmt, Some(&pr)))
                .out(&path)
                .no_progress()
                .run(&ctx)
                .await?;
            ctx.progress(
                0.5 + 0.5 * (k + 1) as f32 / segs.len() as f32,
                format!("Segment {}/{}", k + 1, segs.len()),
            );
            paths.push(path);
        }
        let arts = if p.bool("zip") || paths.len() > 100 {
            vec![zip_files(
                &ctx,
                &paths,
                &format!("{stem}-speech-segments.zip"),
            )?]
        } else {
            publish(&ctx, &paths)?
        };
        Ok(ToolOutput::files(arts).with_data(data))
    })
}

/// Choose cut points so that chunks are at most `max` seconds, cutting in
/// the middle of the latest pause between speech segments when possible.
/// Returns (cut points, number of forced cuts inside speech).
pub fn plan_chunks(speech: &[(f64, f64)], dur: f64, max: f64, min: f64) -> (Vec<f64>, usize) {
    let mut gaps: Vec<f64> = vec![];
    for w in speech.windows(2) {
        if w[1].0 > w[0].1 {
            gaps.push((w[0].1 + w[1].0) / 2.0);
        }
    }
    if let Some(first) = speech.first() {
        if first.0 > 0.0 {
            gaps.insert(0, first.0 / 2.0);
        }
    }
    if let Some(last) = speech.last() {
        if last.1 < dur {
            gaps.push((last.1 + dur) / 2.0);
        }
    }
    let mut cuts = vec![];
    let mut forced = 0;
    let mut start = 0.0;
    while dur - start > max {
        let limit = start + max;
        match gaps
            .iter()
            .copied()
            .rfind(|g| *g > start + min && *g <= limit)
        {
            Some(g) => {
                cuts.push(g);
                start = g;
            }
            None => {
                // Prefer cutting inside a speech segment's boundary if one ends before the limit.
                let edge = speech
                    .iter()
                    .map(|s| s.1)
                    .rfind(|e| *e > start + min && *e <= limit);
                let c = edge.unwrap_or(limit);
                forced += 1;
                cuts.push(c);
                start = c;
            }
        }
    }
    (cuts, forced)
}

fn split_by_speech() -> Arc<dyn Tool> {
    let spec = speech_spec("speech.split_by_speech", "Split at Pauses")
        .describe("Split long recordings into chunks no longer than a maximum length, cutting in pauses between speech (found by VAD) — ideal before transcription or upload limits")
        .keywords(&["split long audio", "chunk", "pauses", "vad", "podcast", "lecture", "transcription prep", "segment"])
        .output(OutputKind::Files)
        .param(audio_input(false))
        .param(ParamSpec::number("max_seconds").label("Maximum chunk length (s)").range(5.0, 86400.0).default_num(300.0))
        .param(ParamSpec::number("min_seconds").label("Minimum chunk length (s)").range(0.0, 3600.0).default_num(10.0).advanced())
        .param(ParamSpec::choice("format", &[("same", "Same as input"), ("wav", "WAV"), ("flac", "FLAC"), ("mp3", "MP3"), ("m4a", "AAC (M4A)"), ("opus", "Opus")]).default_str("same"))
        .params(vad_params());
    async_tool(spec, |ctx, p| async move {
        let f = p.file("input")?.clone();
        let pr = probe_audio(&ctx, &f).await?;
        let dur = pr
            .duration()
            .ok_or_else(|| ToolError::input("could not determine the duration"))?;
        let max = p.num("max_seconds")?;
        let min = p.num_or("min_seconds", 10.0).min(max / 2.0);
        if dur / max > 2000.0 {
            return Err(ToolError::invalid(
                "max_seconds",
                "this would create more than 2000 chunks",
            ));
        }
        let vp = VadParams::from_params(&p);
        let segs = detect_speech(&ctx, &f, &pr, &vp, (0.0, 0.5)).await?;
        let (cuts, forced) = plan_chunks(&segs, dur, max, min);
        let mut bounds = vec![0.0];
        bounds.extend(cuts);
        bounds.push(dur);
        let n = bounds.len() - 1;
        let fmt = match p.str("format")? {
            "same" => match audio_out_ext(&f, &pr).as_str() {
                "mp4" | "mov" | "mkv" | "webm" | "avi" => "m4a".to_string(),
                e => e.to_string(),
            },
            o => o.to_string(),
        };
        let mut rep = Report::new(&["Part", "File", "Start", "End", "Length"]);
        if forced > 0 {
            rep.warnings.push(format!("{forced} cut(s) had no pause within the maximum length and were made inside speech"));
        }
        for k in 0..n {
            let (a, b) = (bounds[k], bounds[k + 1]);
            let out = out_path(&ctx, &f, &format!("chunk{:03}", k + 1), &fmt);
            let mut ff = Ff::new();
            if a > 0.0 {
                ff = ff.args(["-ss", &secs(a)]);
            }
            ff = ff.input(&f.path);
            if k + 1 < n {
                ff = ff.args(["-t", &secs(b - a)]);
            }
            let sp = span(k, n);
            ff.args(["-map", "0:a:0", "-vn", "-sn"])
                .args(audio_args_for_ext(&fmt, Some(&pr)))
                .out(&out)
                .duration(Some(b - a))
                .span(
                    (0.5 + sp.0 * 0.5, 0.5 + sp.1 * 0.5),
                    format!("Writing chunk {}/{n}", k + 1),
                )
                .run(&ctx)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_from_flags() {
        let p = VadParams {
            aggressiveness: 2,
            frame_ms: 100,
            min_speech_ms: 250,
            min_silence_ms: 300,
            padding_ms: 0,
        };
        // speech 0.2–0.6, gap 0.2 (merged), speech 0.8–1.2, gap 1.0, blip 2.2–2.3 (dropped), speech 3.0–3.5
        let mut flags = vec![false; 40];
        for i in (2..6).chain(8..12).chain(22..23).chain(30..35) {
            flags[i] = true;
        }
        let s = frames_to_segments(&flags, 100, &p, 4.0);
        assert_eq!(s, vec![(0.2, 1.2), (3.0, 3.5)]);
        let p2 = VadParams {
            padding_ms: 500,
            ..p
        };
        let s = frames_to_segments(&flags, 100, &p2, 4.0);
        assert_eq!(s, vec![(0.0, 1.7), (2.5, 4.0)]);
        assert!(frames_to_segments(&[], 30, &p, 1.0).is_empty());
    }

    #[test]
    fn silence_is_not_speech() {
        let pcm = vec![0i16; 16000];
        assert!(classify(&pcm, 3, 30).iter().all(|v| !v));
        assert_eq!(classify(&pcm, 0, 10).len(), 100);
    }

    #[test]
    fn chunk_planning() {
        let speech = vec![(0.0, 50.0), (55.0, 110.0), (112.0, 200.0), (205.0, 290.0)];
        let (cuts, forced) = plan_chunks(&speech, 300.0, 120.0, 10.0);
        assert_eq!(cuts, vec![111.0, 202.5]);
        assert_eq!(forced, 0);
        let (cuts, forced) = plan_chunks(&[(0.0, 300.0)], 300.0, 100.0, 10.0);
        assert_eq!(cuts, vec![100.0, 200.0]);
        assert_eq!(forced, 2);
        assert!(plan_chunks(&speech, 300.0, 400.0, 10.0).0.is_empty());
    }
}
