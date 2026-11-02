mod common;
use common::*;
use serde_json::json;

macro_rules! need {
    ($h:expr, $id:expr) => {
        if !has_ffmpeg() || !$h.available($id) {
            return;
        }
    };
}

fn sr(p: &std::path::Path) -> String {
    probe_json(p)["streams"][0]["sample_rate"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn convert_compress() {
    let h = harness();
    need!(h, "audio.convert");
    let t = tone(&h, "tone.wav");
    for (fmt, codec_name) in [
        ("mp3", "mp3"),
        ("m4a", "aac"),
        ("ogg", "vorbis"),
        ("opus", "opus"),
        ("flac", "flac"),
        ("aiff", "pcm_s16be"),
        ("alac", "alac"),
    ] {
        let out = match h
            .run("audio.convert", json!({"input": [s(&t)], "format": fmt}))
            .await
        {
            Ok(o) => o,
            // FFmpeg builds differ (e.g. Homebrew ships without libvorbis):
            // the tool must say so clearly rather than fail obscurely.
            Err(e) if e.code() == "unsupported" && !common::has_encoder_for(fmt) => {
                assert!(e.to_string().contains("encoder"), "{fmt}: {e}");
                continue;
            }
            Err(e) => panic!("{fmt}: {e}"),
        };
        assert_eq!(
            codec(&out.artifacts[0].path, "audio").as_deref(),
            Some(codec_name),
            "{fmt}"
        );
    }
    let out = h.run("audio.convert", json!({"input": [s(&t)], "format": "wav", "sample_rate": "16000", "channels": "1", "bit_depth": "24"})).await.unwrap();
    let j = probe_json(&out.artifacts[0].path);
    assert_eq!(j["streams"][0]["codec_name"], "pcm_s24le");
    assert_eq!(j["streams"][0]["channels"], 1);
    assert_eq!(sr(&out.artifacts[0].path), "16000");
    let out = h
        .run(
            "audio.compress",
            json!({"input": [s(&t)], "level": "voice", "format": "opus"}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].size < std::fs::metadata(&t).unwrap().len() / 5);
    let e = h
        .run("audio.convert", json!({"input": [s(&t)], "format": "xyz"}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let bad = h.write("bad.mp3", b"nope");
    let e = h
        .run("audio.convert", json!({"input": [s(&bad)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
}

#[tokio::test]
async fn trim_split_merge() {
    let h = harness();
    need!(h, "audio.trim");
    let t = tone(&h, "tone.wav");
    let out = h.run("audio.trim", json!({"input": [s(&t)], "start": "0.5", "duration": "1", "fade_in": 0.2, "fade_out": 0.2})).await.unwrap();
    assert!((duration(&out.artifacts[0].path) - 1.0).abs() < 0.05);
    let e = h
        .run("audio.trim", json!({"input": [s(&t)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");

    let out = h
        .run(
            "audio.split",
            json!({"input": s(&t), "mode": "duration", "chunk_seconds": 1}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 3);
    let out = h
        .run(
            "audio.split",
            json!({"input": s(&t), "mode": "timestamps", "points": "1.2"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);
    // tone / silence / tone → split at the silence
    let gappy = gen(&h, "gappy.wav", &["-f", "lavfi", "-i", "sine=d=2", "-f", "lavfi", "-i", "anullsrc=r=44100:cl=mono:d=1.5", "-f", "lavfi", "-i", "sine=d=2", "-filter_complex", "[0:a]aformat=channel_layouts=mono[a0];[1:a]aformat=channel_layouts=mono[a1];[2:a]aformat=channel_layouts=mono[a2];[a0][a1][a2]concat=n=3:v=0:a=1"]);
    let out = h
        .run(
            "audio.split",
            json!({"input": s(&gappy), "mode": "silence", "min_silence": 0.5, "min_part": 1}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);
    let cut = out.data["files"][0]["end"].as_f64().unwrap();
    assert!((cut - 2.75).abs() < 0.2, "{cut}");

    let out = h
        .run(
            "audio.detect_silence",
            json!({"input": s(&gappy), "min_duration": 0.5}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["count"], 1);
    let st = out.data["silences"][0]["start"].as_f64().unwrap();
    assert!((st - 2.0).abs() < 0.1);

    let t2 = gen(
        &h,
        "mono.mp3",
        &["-f", "lavfi", "-i", "sine=f=880:d=2:sample_rate=22050"],
    );
    let out = h
        .run(
            "audio.merge",
            json!({"input": [s(&t), s(&t2)], "format": "flac"}),
        )
        .await
        .unwrap();
    assert!((duration(&out.artifacts[0].path) - 5.0).abs() < 0.1);
    let out = h
        .run(
            "audio.merge",
            json!({"input": [s(&t), s(&t2)], "crossfade": 1, "format": "mp3"}),
        )
        .await
        .unwrap();
    assert!((duration(&out.artifacts[0].path) - 4.0).abs() < 0.15);
    let e = h
        .run(
            "audio.merge",
            json!({"input": [s(&t), s(&t2)], "crossfade": 2.5}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");

    let out = h
        .run(
            "audio.remove_silence",
            json!({"input": [s(&gappy)], "mode": "all", "min_silence": 0.5, "keep_silence": 0.2}),
        )
        .await
        .unwrap();
    let d = duration(&out.artifacts[0].path);
    assert!(d < 4.8 && d > 3.8, "{d}");
}

#[tokio::test]
async fn loudness_and_filters() {
    let h = harness();
    need!(h, "audio.normalize");
    let quiet = gen(
        &h,
        "quiet.wav",
        &[
            "-f",
            "lavfi",
            "-i",
            "anoisesrc=d=5:a=0.02:c=pink",
            "-ac",
            "2",
        ],
    );
    let out = h
        .run(
            "audio.normalize",
            json!({"input": [s(&quiet)], "preset": "podcast"}),
        )
        .await
        .unwrap();
    let oi = out.data["files"][0]["output"]["integrated"]
        .as_f64()
        .unwrap();
    assert!((oi + 16.0).abs() < 1.5, "integrated {oi}");
    assert!(
        out.data["files"][0]["measured"]["integrated"]
            .as_f64()
            .unwrap()
            < -30.0
    );
    let out = h
        .run(
            "audio.normalize",
            json!({"input": [s(&quiet)], "mode": "peak", "peak_db": -3}),
        )
        .await
        .unwrap();
    assert!(out.data["files"][0]["gain_db"].as_f64().unwrap() > 10.0);
    let info = h
        .run(
            "audio.info",
            json!({"input": [s(&out.artifacts[0].path)], "loudness": true}),
        )
        .await
        .unwrap();
    let tp = info.data["summary"]["loudness"]["true_peak_dbfs"]
        .as_f64()
        .unwrap();
    assert!(tp < -1.5 && tp > -4.5, "true peak {tp}");
    let silent = gen(&h, "silent.wav", &["-f", "lavfi", "-i", "anullsrc=d=2"]);
    let e = h
        .run("audio.normalize", json!({"input": [s(&silent)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");

    let t = tone(&h, "tone.wav");
    for (id, params) in [
        ("audio.volume", json!({"amount": -6})),
        ("audio.volume", json!({"unit": "factor", "amount": 1.5})),
        ("audio.denoise", json!({"method": "afftdn"})),
        (
            "audio.denoise",
            json!({"method": "anlmdn", "strength": "light", "highpass": 0}),
        ),
        ("audio.equalizer", json!({"preset": "voice"})),
        (
            "audio.equalizer",
            json!({"preset": "none", "bands": "100:+3, 2000:-4:2"}),
        ),
        ("audio.fade", json!({"fade_in": 0.5, "fade_out": 1})),
        ("audio.reverse", json!({})),
        ("audio.channels", json!({"operation": "left"})),
        ("audio.channels", json!({"operation": "swap"})),
    ] {
        let mut pr = params.clone();
        pr["input"] = json!([s(&t)]);
        let out = match h.run(id, pr).await {
            Ok(o) => o,
            // FFmpeg 9 builds refuse anlmdn (it crashes there); other
            // FFmpeg versions run it.
            Err(e) if params["method"] == "anlmdn" && e.code() == "unsupported" => {
                assert!(e.to_string().contains("afftdn"), "{e}");
                continue;
            }
            Err(e) => panic!("{id} {params}: {e}"),
        };
        assert!((duration(&out.artifacts[0].path) - 3.0).abs() < 0.1, "{id}");
    }
    let e = h
        .run(
            "audio.equalizer",
            json!({"input": [s(&t)], "preset": "none", "bands": "abc"}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let out = h
        .run(
            "audio.channels",
            json!({"input": [s(&t)], "operation": "mono"}),
        )
        .await
        .unwrap();
    assert_eq!(
        probe_json(&out.artifacts[0].path)["streams"][0]["channels"],
        1
    );
    let mono = out.artifacts[0].path.clone();
    let e = h
        .run(
            "audio.channels",
            json!({"input": [s(&mono)], "operation": "swap"}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");

    let out = h
        .run("audio.speed", json!({"input": [s(&t)], "tempo": 1.5}))
        .await
        .unwrap();
    assert!((duration(&out.artifacts[0].path) - 2.0).abs() < 0.1);
    let out = h
        .run(
            "audio.speed",
            json!({"input": [s(&t)], "tempo": 1.0, "pitch": 3, "engine": "atempo"}),
        )
        .await
        .unwrap();
    assert!((duration(&out.artifacts[0].path) - 3.0).abs() < 0.15);
    let out = h
        .run(
            "audio.sample_rate",
            json!({"input": [s(&t)], "rate": "22050", "bit_depth": "16"}),
        )
        .await
        .unwrap();
    assert_eq!(sr(&out.artifacts[0].path), "22050");

    // audio filters keep the video stream of video inputs
    let v = video(&h, "clip.mp4");
    let out = h
        .run("audio.volume", json!({"input": [s(&v)], "amount": 3}))
        .await
        .unwrap();
    assert_eq!(
        codec(&out.artifacts[0].path, "video").as_deref(),
        Some("h264")
    );
}

#[tokio::test]
async fn visuals_info_tags() {
    let h = harness();
    need!(h, "audio.waveform");
    let t = tone(&h, "tone.wav");
    let out = h
        .run(
            "audio.waveform",
            json!({"input": [s(&t)], "width": 400, "height": 100, "peaks": true, "points": 50}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out.artifacts[0].path), (400, 100));
    assert_eq!(out.data["files"][0]["peaks"].as_array().unwrap().len(), 100);
    let out = h
        .run(
            "audio.waveform",
            json!({"input": [s(&t)], "transparent": true, "split_channels": true}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts[0].mime, "image/png");
    let out = h
        .run(
            "audio.spectrogram",
            json!({"input": [s(&t)], "width": 256, "height": 128, "legend": false}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out.artifacts[0].path), (256, 128));

    let out = h
        .run("audio.info", json!({"input": [s(&t)]}))
        .await
        .unwrap();
    assert_eq!(out.data["summary"]["sample_rate"], 44100);
    assert_eq!(out.data["summary"]["channels"], 2);

    let mp3 = gen(
        &h,
        "song.mp3",
        &["-f", "lavfi", "-i", "sine=d=2", "-metadata", "title=Old"],
    );
    let out = h
        .run("audio.tags", json!({"input": s(&mp3)}))
        .await
        .unwrap();
    assert_eq!(out.data["tags"]["title"], "Old");
    assert!(out.artifacts.is_empty());
    let cover = gen(
        &h,
        "cover.png",
        &[
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=64x64",
            "-frames:v",
            "1",
            "-update",
            "1",
        ],
    );
    let out = h.run("audio.tags", json!({"input": s(&mp3), "title": "New -title", "artist": "Me", "year": "2024", "cover": s(&cover)})).await.unwrap();
    assert_eq!(out.data["tags"]["title"], "New -title");
    assert_eq!(out.data["tags"]["artist"], "Me");
    assert_eq!(out.data["has_cover"], true);
    let tagged = out.artifacts[0].path.clone();
    let out = h
        .run(
            "audio.tags",
            json!({"input": s(&tagged), "remove_cover": true}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["has_cover"], false);
    assert_eq!(out.data["tags"]["artist"], "Me");
    if common::has_encoder("libvorbis") {
        let ogg = gen(
            &h,
            "song.ogg",
            &["-f", "lavfi", "-i", "sine=d=1", "-c:a", "libvorbis"],
        );
        let out = h
            .run(
                "audio.tags",
                json!({"input": s(&ogg), "album": "Ogg Album"}),
            )
            .await
            .unwrap();
        assert_eq!(out.data["tags"]["album"], "Ogg Album");
        let e = h
            .run("audio.tags", json!({"input": s(&ogg), "cover": s(&cover)}))
            .await
            .unwrap_err();
        assert_eq!(e.code(), "invalid_input");
    }
}
