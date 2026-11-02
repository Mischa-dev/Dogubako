mod common;
use common::*;
use serde_json::json;
use std::process::Command;

fn has_flite() -> bool {
    Command::new("ffmpeg")
        .args(["-hide_banner", "-filters"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(" flite "))
        .unwrap_or(false)
}

/// silence 1.5 s, speech, silence 2 s, speech, silence 1.5 s (synthetic voice via libflite).
fn speech_fixture(h: &dogubako_core::testing::TestHarness) -> std::path::PathBuf {
    gen(
        h,
        "speech.wav",
        &[
            "-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono:d=1.5",
            "-f", "lavfi", "-i", "flite=text='Hello there. This is the first sentence of the test.'",
            "-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono:d=2",
            "-f", "lavfi", "-i", "flite=text='And here comes a second sentence after a pause.'",
            "-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono:d=1.5",
            "-filter_complex",
            "[1:a]aresample=16000[s1];[3:a]aresample=16000[s2];[0:a][s1][2:a][s2][4:a]concat=n=5:v=0:a=1",
            "-ar", "16000",
        ],
    )
}

#[tokio::test]
async fn transcript_convert_and_shift() {
    let h = harness();
    let srt =
        "1\n00:00:01,000 --> 00:00:02,000\nHello\n\n2\n00:00:03,000 --> 00:00:04,500\nWorld\n";
    let out = h
        .run(
            "speech.transcript_convert",
            json!({"text": srt, "output_format": "vtt"}),
        )
        .await
        .unwrap();
    let vtt = out.text.clone().unwrap();
    assert!(vtt.starts_with("WEBVTT"));
    assert!(vtt.contains("00:00:03.000 --> 00:00:04.500"));
    assert_eq!(out.data["input_format"], "srt");
    assert_eq!(out.artifacts.len(), 1);
    let out = h
        .run(
            "speech.transcript_convert",
            json!({"text": vtt, "output_format": "json"}),
        )
        .await
        .unwrap();
    let v: serde_json::Value = serde_json::from_str(out.text.as_ref().unwrap()).unwrap();
    assert_eq!(v["segments"][1]["end"], 4.5);
    assert_eq!(v["text"], "Hello World");
    let out = h
        .run(
            "speech.transcript_convert",
            json!({"text": out.text.unwrap(), "output_format": "md"}),
        )
        .await
        .unwrap();
    assert!(out.text.unwrap().contains("**[00:00:01]** Hello World"));
    let f = h.write("subs.srt", srt.as_bytes());
    let out = h
        .run(
            "speech.transcript_convert",
            json!({"file": s(&f), "output_format": "txt"}),
        )
        .await
        .unwrap();
    assert_eq!(out.text.as_deref(), Some("Hello World\n"));
    let e = h
        .run(
            "speech.transcript_convert",
            json!({"text": "plain words", "output_format": "srt"}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let e = h
        .run(
            "speech.transcript_convert",
            json!({"text": "1\n00:00:xx,000 --> 00:00:02,000\nBad\n", "input_format": "srt"}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let e = h
        .run("speech.transcript_convert", json!({"text": "  "}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");

    let out = h
        .run(
            "speech.subtitle_shift",
            json!({"text": srt, "offset": -1.5}),
        )
        .await
        .unwrap();
    let t = out.text.unwrap();
    assert!(t.contains("00:00:00,000 --> 00:00:00,500"));
    assert!(t.contains("00:00:01,500 --> 00:00:03,000"));
    let out = h
        .run(
            "speech.subtitle_shift",
            json!({"text": srt, "offset": -2.5}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["dropped"], 1);
    assert!(!out.warnings.is_empty());
    let out = h
        .run(
            "speech.subtitle_shift",
            json!({"text": srt, "fps_from": 25, "fps_to": 50, "output_format": "vtt"}),
        )
        .await
        .unwrap();
    assert!(out.text.unwrap().contains("00:00:01.500 --> 00:00:02.250"));
}

#[tokio::test]
async fn video_subtitle_tools() {
    let h = harness();
    if !has_ffmpeg() || !h.available("video.subtitles_add") {
        return;
    }
    let v = video(&h, "clip.mp4");
    let srt = h.write("en.srt", b"1\n00:00:00,500 --> 00:00:02,000\nHello subtitles\n\n2\n00:00:02,500 --> 00:00:03,500\nSecond cue\n");
    let vtt = h.write("de.vtt", b"WEBVTT\n\n00:00.500 --> 00:02.000\nHallo\n");
    let out = h
        .run(
            "video.subtitles_add",
            json!({"input": s(&v), "subtitles": [s(&srt), s(&vtt)], "languages": "eng, deu"}),
        )
        .await
        .unwrap();
    let tracks = out.data["subtitle_tracks"].as_array().unwrap().clone();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0]["codec"], "mov_text");
    assert_eq!(tracks[1]["language"], "deu");
    let with_subs = out.artifacts[0].path.clone();
    let out = h
        .run(
            "video.subtitles_add",
            json!({"input": s(&v), "subtitles": [s(&srt)], "format": "mkv"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["subtitle_tracks"][0]["codec"], "subrip");
    let e = h
        .run(
            "video.subtitles_add",
            json!({"input": s(&v), "subtitles": [s(&srt)], "languages": "english!"}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let bad = h.write("bad.srt", b"not subtitles at all");
    let e = h
        .run(
            "video.subtitles_add",
            json!({"input": s(&v), "subtitles": [s(&bad)]}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");

    let out = h
        .run(
            "video.subtitles_extract",
            json!({"input": [s(&with_subs)], "format": "vtt"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);
    let text = std::fs::read_to_string(&out.artifacts[0].path).unwrap();
    assert!(text.starts_with("WEBVTT") && text.contains("Hello subtitles"));
    let out = h
        .run(
            "video.subtitles_extract",
            json!({"input": [s(&with_subs)], "list_only": true}),
        )
        .await
        .unwrap();
    assert!(out.artifacts.is_empty());
    assert_eq!(out.data["tracks"].as_array().unwrap().len(), 2);

    let out = h
        .run(
            "video.subtitles_remove",
            json!({"input": [s(&with_subs)], "languages": "deu"}),
        )
        .await
        .unwrap();
    let j = probe_json(&out.artifacts[0].path);
    assert_eq!(
        j["streams"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["codec_type"] == "subtitle")
            .count(),
        1
    );
    let out = h
        .run("video.subtitles_remove", json!({"input": [s(&with_subs)]}))
        .await
        .unwrap();
    assert!(codec(&out.artifacts[0].path, "subtitle").is_none());

    if has_filter("subtitles") {
        let out = h.run("video.subtitles_burn", json!({"input": [s(&v)], "subtitles": s(&srt), "font_size": 18, "color": "#ffff00", "position": "top"})).await.unwrap();
        assert!(codec(&out.artifacts[0].path, "subtitle").is_none());
        assert_eq!(dims(&out.artifacts[0].path), (320, 240));
        let out = h
            .run(
                "video.subtitles_burn",
                json!({"input": [s(&with_subs)], "stream": 0}),
            )
            .await
            .unwrap();
        assert!(out.artifacts[0].size > 0);
    } else {
        // Minimal FFmpeg builds (e.g. Homebrew) lack libass: clear error, not a crash.
        let e = h
            .run(
                "video.subtitles_burn",
                json!({"input": [s(&v)], "subtitles": s(&srt)}),
            )
            .await
            .unwrap_err();
        assert_eq!(e.code(), "unsupported", "{e}");
    }
    let e = h
        .run("video.subtitles_burn", json!({"input": [s(&v)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
}

#[tokio::test]
async fn vad_tools() {
    let h = harness();
    if !has_ffmpeg() || !has_flite() || !h.available("speech.vad") {
        eprintln!("skipping: needs ffmpeg with libflite");
        return;
    }
    let a = speech_fixture(&h);
    let total = duration(&a);
    let out = h
        .run(
            "speech.vad",
            json!({"input": s(&a), "aggressiveness": 3, "padding_ms": 100, "export": "audacity"}),
        )
        .await
        .unwrap();
    let segs = out.data["segments"].as_array().unwrap().clone();
    assert_eq!(segs.len(), 2, "{segs:?}");
    let s0 = segs[0]["start"].as_f64().unwrap();
    let e0 = segs[0]["end"].as_f64().unwrap();
    let s1 = segs[1]["start"].as_f64().unwrap();
    let e1 = segs[1]["end"].as_f64().unwrap();
    // Speech starts after the 1.5 s of leading silence and the 2 s pause separates the sentences.
    assert!((1.2..1.8).contains(&s0), "s0 {s0}");
    assert!(s1 - e0 > 1.5, "gap {}", s1 - e0);
    assert!(e1 < total - 1.0, "e1 {e1} total {total}");
    assert_eq!(out.artifacts.len(), 1);
    assert!(std::fs::read_to_string(&out.artifacts[0].path)
        .unwrap()
        .contains("speech 2"));

    // pure silence → no speech
    let sil = gen(
        &h,
        "silence.wav",
        &["-f", "lavfi", "-i", "anullsrc=r=16000:cl=mono:d=2"],
    );
    let out = h
        .run("speech.vad", json!({"input": s(&sil)}))
        .await
        .unwrap();
    assert_eq!(out.data["count"], 0);
    let e = h
        .run("speech.extract_segments", json!({"input": s(&sil)}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");

    let out = h
        .run(
            "speech.extract_segments",
            json!({"input": s(&a), "mode": "concat", "format": "flac", "padding_ms": 50}),
        )
        .await
        .unwrap();
    let d = duration(&out.artifacts[0].path);
    let speech = out.data["speech_seconds"].as_f64().unwrap();
    assert!((d - speech).abs() < 0.15, "{d} vs {speech}");
    assert!(d < total - 3.0);
    let out = h
        .run(
            "speech.extract_segments",
            json!({"input": s(&a), "mode": "files", "format": "wav"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);

    let out = h
        .run(
            "speech.split_by_speech",
            json!({"input": s(&a), "max_seconds": total * 0.7, "min_seconds": 1}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);
    let cut = out.data["files"][0]["end"].as_f64().unwrap();
    assert!(
        cut > e0 && cut < s1,
        "cut {cut} should be in the pause {e0}..{s1}"
    );
}

/// End-to-end whisper.cpp / Piper plumbing; runs only when the programs and
/// models are installed (e.g. DOGUBAKO_TEST_MODELS + whisper-cli on PATH).
#[tokio::test]
async fn whisper_and_piper_if_installed() {
    let h = harness();
    if !has_ffmpeg() || !has_flite() {
        return;
    }
    if h.available("speech.transcribe") {
        let a = speech_fixture(&h);
        let out = h.run("speech.transcribe", json!({"input": [s(&a)], "formats": ["txt", "srt", "vtt", "json", "md"], "language": "en", "word_timestamps": true, "threads": 2})).await.unwrap();
        assert_eq!(out.artifacts.len(), 5);
        assert!(out.data["segments"].is_array());
        let out = h
            .run(
                "speech.transcribe",
                json!({"input": [s(&a)], "vad": true, "formats": ["json"]}),
            )
            .await
            .unwrap();
        assert!(out.data["language"].is_string());
        let e = h
            .run(
                "speech.transcribe",
                json!({"input": [s(&a)], "language": "klingon"}),
            )
            .await
            .unwrap_err();
        assert_eq!(e.code(), "invalid_input");
        let out = h
            .run("speech.detect_language", json!({"input": s(&a)}))
            .await
            .unwrap();
        assert!(out.data["language"].is_string());
        let v = video(&h, "clip.mp4");
        let out = h
            .run(
                "speech.subtitles",
                json!({"input": [s(&v)], "embed": "soft", "format": "both"}),
            )
            .await
            .unwrap();
        assert!(out.artifacts.len() >= 2);
    }
    if h.available("speech.tts") {
        let out = h
            .run(
                "speech.tts",
                json!({"text": "Hello from Dogubako.", "format": "mp3"}),
            )
            .await
            .unwrap();
        assert_eq!(
            codec(&out.artifacts[0].path, "audio").as_deref(),
            Some("mp3")
        );
    }
}
