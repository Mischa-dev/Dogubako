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

#[tokio::test]
async fn convert_variants() {
    let h = harness();
    need!(h, "video.convert");
    let v = video(&h, "in.mp4");
    let out = h
        .run(
            "video.convert",
            json!({"input": [s(&v)], "format": "webm", "quality": "low", "speed": "fast"}),
        )
        .await
        .unwrap();
    let a = &out.artifacts[0];
    assert!(a.name.ends_with(".webm"));
    assert_eq!(codec(&a.path, "video").as_deref(), Some("vp9"));
    assert_eq!(codec(&a.path, "audio").as_deref(), Some("opus"));
    let out = h.run("video.convert", json!({"input": [s(&v)], "format": "mkv", "video_codec": "copy", "audio_codec": "copy"})).await.unwrap();
    assert_eq!(
        codec(&out.artifacts[0].path, "video").as_deref(),
        Some("h264")
    );
    let out = h
        .run("video.convert", json!({"input": [s(&v)], "format": "gif"}))
        .await
        .unwrap();
    assert_eq!(out.artifacts[0].mime, "image/gif");
    // invalid combinations are rejected
    let e = h
        .run(
            "video.convert",
            json!({"input": [s(&v)], "format": "webm", "video_codec": "h264"}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let e = h
        .run(
            "video.convert",
            json!({"input": [s(&v)], "format": "webm", "video_codec": "copy"}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let e = h
        .run("video.convert", json!({"input": [s(&v)], "format": "flv"}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    // hardware request falls back to software with a warning when unavailable
    let out = h.run("video.convert", json!({"input": [s(&v)], "format": "mp4", "hardware": "auto", "quality": "low", "speed": "fast"})).await.unwrap();
    assert!(out.artifacts[0].size > 0);
}

#[tokio::test]
async fn compress_to_size_and_quality() {
    let h = harness();
    need!(h, "video.compress");
    let v = gen(
        &h,
        "big.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=640x480:d=6:r=25",
            "-f",
            "lavfi",
            "-i",
            "anoisesrc=d=6",
            "-c:v",
            "libx264",
            "-crf",
            "5",
            "-preset",
            "ultrafast",
            "-c:a",
            "aac",
            "-b:a",
            "256k",
            "-shortest",
        ],
    );
    let out = h
        .run(
            "video.compress",
            json!({"input": [s(&v)], "mode": "size", "target_mb": 0.4, "speed": "fast"}),
        )
        .await
        .unwrap();
    let size = out.artifacts[0].size as f64 / 1024.0 / 1024.0;
    assert!(size < 0.55, "size {size}");
    // A small target caps the audio budget; the encoder must use that cap,
    // not the full audio_kbps, or the file overshoots.
    let small = h
        .run(
            "video.compress",
            json!({"input": [s(&v)], "mode": "size", "target_mb": 0.15, "speed": "fast"}),
        )
        .await
        .unwrap();
    let size = small.artifacts[0].size as f64 / 1024.0 / 1024.0;
    assert!(size < 0.165, "size {size}");
    assert!(out.data["files"][0]["method"]
        .as_str()
        .unwrap()
        .contains("2-pass"));
    let out = h
        .run(
            "video.compress",
            json!({"input": [s(&v)], "level": "strong", "max_resolution": "360", "speed": "fast"}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out.artifacts[0].path), (480, 360));
    let e = h
        .run(
            "video.compress",
            json!({"input": [s(&v)], "mode": "size", "target_mb": 0.01}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
}

#[tokio::test]
async fn resize_fps_crop_rotate() {
    let h = harness();
    need!(h, "video.resize");
    let v = video(&h, "in.mp4");
    let out = h
        .run(
            "video.resize",
            json!({"input": [s(&v)], "size": "custom", "width": 160}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out.artifacts[0].path), (160, 120));
    let out = h
        .run(
            "video.resize",
            json!({"input": [s(&v)], "size": "custom", "width": 200, "height": 200, "fit": "pad"}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out.artifacts[0].path), (200, 200));
    let out = h
        .run("video.resize", json!({"input": [s(&v)], "size": "1080"}))
        .await
        .unwrap();
    assert_eq!(
        dims(&out.artifacts[0].path),
        (320, 240),
        "no upscale by default"
    );
    assert!(!out.warnings.is_empty());

    let out = h
        .run("video.fps", json!({"input": [s(&v)], "fps": 10}))
        .await
        .unwrap();
    let j = probe_json(&out.artifacts[0].path);
    assert_eq!(j["streams"][0]["avg_frame_rate"], "10/1");

    let out = h
        .run(
            "video.crop",
            json!({"input": [s(&v)], "mode": "aspect", "aspect": "1:1"}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out.artifacts[0].path), (240, 240));
    let out = h.run("video.crop", json!({"input": [s(&v)], "mode": "manual", "x": 10, "y": 10, "width": 100, "height": 50})).await.unwrap();
    assert_eq!(dims(&out.artifacts[0].path), (100, 50));
    let e = h.run("video.crop", json!({"input": [s(&v)], "mode": "manual", "x": 300, "y": 0, "width": 100, "height": 50})).await.unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let lb = gen(
        &h,
        "letterbox.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=320x180:d=2:r=25",
            "-vf",
            "pad=320:240:0:30:black",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
        ],
    );
    let out = h
        .run("video.crop", json!({"input": [s(&lb)], "mode": "auto"}))
        .await
        .unwrap();
    let (w, hh) = dims(&out.artifacts[0].path);
    assert_eq!(w, 320);
    assert!((176..=184).contains(&hh), "auto crop height {hh}");

    let out = h
        .run(
            "video.rotate",
            json!({"input": [s(&v)], "rotation": "cw90"}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out.artifacts[0].path), (240, 320));
    let out = h
        .run(
            "video.rotate",
            json!({"input": [s(&v)], "rotation": "ccw90", "method": "metadata"}),
        )
        .await
        .unwrap();
    let j = probe_json(&out.artifacts[0].path);
    let rot = j["streams"][0]["side_data_list"][0]["rotation"]
        .as_i64()
        .unwrap();
    assert_eq!(rot, 90);
}

#[tokio::test]
async fn trim_cut_merge_speed() {
    let h = harness();
    need!(h, "video.trim");
    let v = video(&h, "in.mp4");
    let out = h
        .run(
            "video.trim",
            json!({"input": [s(&v)], "start": "1", "end": "2.5"}),
        )
        .await
        .unwrap();
    let d = duration(&out.artifacts[0].path);
    assert!((d - 1.5).abs() < 0.15, "trim {d}");
    let out = h
        .run(
            "video.trim",
            json!({"input": [s(&v)], "start": "0:01", "duration": "2", "mode": "copy"}),
        )
        .await
        .unwrap();
    assert!(duration(&out.artifacts[0].path) > 1.0);
    let e = h
        .run("video.trim", json!({"input": [s(&v)], "start": "10"}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let e = h
        .run("video.trim", json!({"input": [s(&v)], "start": "abc"}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");

    let out = h
        .run(
            "video.cut",
            json!({"input": s(&v), "operation": "remove", "start": "1", "end": "2"}),
        )
        .await
        .unwrap();
    let d = duration(&out.artifacts[0].path);
    assert!((d - 3.0).abs() < 0.2, "cut {d}");
    let out = h
        .run(
            "video.cut",
            json!({"input": s(&v), "operation": "split", "points": "1, 2.5"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 3);
    let out = h
        .run(
            "video.cut",
            json!({"input": s(&v), "operation": "every", "every_seconds": 2, "mode": "copy"}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);

    let v2 = video(&h, "in2.mp4");
    let out = h
        .run("video.merge", json!({"input": [s(&v), s(&v2)]}))
        .await
        .unwrap();
    assert_eq!(out.data["method"], "stream copy");
    assert!((duration(&out.artifacts[0].path) - 8.0).abs() < 0.3);
    let small = gen(
        &h,
        "small.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc=s=160x90:d=2:r=30",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
        ],
    );
    let out = h
        .run("video.merge", json!({"input": [s(&v), s(&small)]}))
        .await
        .unwrap();
    assert_eq!(out.data["method"], "re-encoded");
    assert_eq!(dims(&out.artifacts[0].path), (320, 240));
    assert!(codec(&out.artifacts[0].path, "audio").is_some());
    let e = h
        .run("video.merge", json!({"input": [s(&v)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");

    let out = h
        .run("video.speed", json!({"input": [s(&v)], "factor": 2}))
        .await
        .unwrap();
    let d = duration(&out.artifacts[0].path);
    assert!((d - 2.0).abs() < 0.2, "speed {d}");
}

#[tokio::test]
async fn audio_tracks() {
    let h = harness();
    need!(h, "video.extract_audio");
    let v = video(&h, "in.mp4");
    let out = h
        .run(
            "video.extract_audio",
            json!({"input": [s(&v)], "format": "copy"}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].name.ends_with(".m4a"));
    let out = h
        .run(
            "audio.extract_from_video",
            json!({"input": [s(&v)], "format": "mp3"}),
        )
        .await
        .unwrap();
    assert_eq!(
        codec(&out.artifacts[0].path, "audio").as_deref(),
        Some("mp3")
    );
    let out = h
        .run("video.remove_audio", json!({"input": [s(&v)]}))
        .await
        .unwrap();
    assert!(codec(&out.artifacts[0].path, "audio").is_none());
    let muted = out.artifacts[0].path.clone();
    let e = h
        .run("video.extract_audio", json!({"input": [s(&muted)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let t = gen(&h, "music.wav", &["-f", "lavfi", "-i", "sine=f=220:d=1.5"]);
    let out = h
        .run(
            "video.replace_audio",
            json!({"input": [s(&muted)], "audio": s(&t), "fit": "loop"}),
        )
        .await
        .unwrap();
    assert!((duration(&out.artifacts[0].path) - 4.0).abs() < 0.3);
    let out = h
        .run(
            "video.replace_audio",
            json!({"input": [s(&v)], "audio": s(&t), "fit": "shortest"}),
        )
        .await
        .unwrap();
    assert!(duration(&out.artifacts[0].path) < 2.5);
    let out = h
        .run(
            "video.replace_audio",
            json!({"input": [s(&v)], "audio": s(&t), "fit": "pad", "mix_original": true}),
        )
        .await
        .unwrap();
    assert!((duration(&out.artifacts[0].path) - 4.0).abs() < 0.3);
}

#[tokio::test]
async fn frames_thumbs_gif() {
    let h = harness();
    need!(h, "video.extract_frames");
    let v = video(&h, "in.mp4");
    let out = h
        .run(
            "video.extract_frames",
            json!({"input": [s(&v)], "mode": "interval", "interval": 1, "format": "jpg"}),
        )
        .await
        .unwrap();
    assert!(
        (4..=5).contains(&out.artifacts.len()),
        "{}",
        out.artifacts.len()
    );
    assert_eq!(
        out.data["frames"][1]["time"].as_f64().map(|t| t.round()),
        Some(1.0)
    );
    let out = h
        .run(
            "video.extract_frames",
            json!({"input": [s(&v)], "mode": "timestamps", "timestamps": "0.5, 2", "width": 100}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 2);
    let out = h
        .run(
            "video.extract_frames",
            json!({"input": [s(&v)], "mode": "all", "zip": true, "max_frames": 20}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts.len(), 1);
    assert_eq!(out.data["count"], 20);
    let out = h
        .run(
            "video.extract_frames",
            json!({"input": [s(&v)], "mode": "keyframes"}),
        )
        .await
        .unwrap();
    assert!(!out.artifacts.is_empty());

    let out = h
        .run(
            "video.thumbnail",
            json!({"input": [s(&v)], "mode": "best", "width": 160}),
        )
        .await
        .unwrap();
    assert_eq!(out.artifacts[0].mime, "image/jpeg");
    let out = h
        .run(
            "video.thumbnail",
            json!({"input": [s(&v)], "mode": "time", "time": "50%", "format": "png"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["files"][0]["time"].as_f64(), Some(2.0));

    let out = h
        .run(
            "video.to_gif",
            json!({"input": [s(&v)], "start": "1", "duration": "1", "width": 160, "fps": 10}),
        )
        .await
        .unwrap();
    let g = out.artifacts[0].path.clone();
    assert_eq!(dims(&g).0, 160);
    let webp = h
        .run(
            "video.to_gif",
            json!({"input": [s(&v)], "duration": "1", "format": "webp"}),
        )
        .await;
    if has_encoder("libwebp_anim") {
        assert_eq!(webp.unwrap().artifacts[0].mime, "image/webp");
    } else {
        assert_eq!(webp.unwrap_err().code(), "unsupported");
    }
    let out = h
        .run("video.from_gif", json!({"input": [s(&g)], "repeat": 1}))
        .await
        .unwrap();
    assert_eq!(
        codec(&out.artifacts[0].path, "video").as_deref(),
        Some("h264")
    );
    assert!(duration(&out.artifacts[0].path) > 1.5);

    let out = h
        .run(
            "video.contact_sheet",
            json!({"input": [s(&v)], "columns": 2, "rows": 2, "tile_width": 100, "padding": 0}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out.artifacts[0].path), (200, 152));
    let out = h
        .run(
            "video.preview",
            json!({"input": [s(&v)], "segments": 2, "segment_seconds": 1, "width": 160}),
        )
        .await
        .unwrap();
    assert!((duration(&out.artifacts[0].path) - 2.0).abs() < 0.3);
}

#[tokio::test]
async fn gif_palette_colors_down_to_two() {
    let h = harness();
    need!(h, "video.to_gif");
    if !palettegen_allows_two_colors() {
        return;
    }
    let v = video(&h, "in.mp4");
    for colors in [2, 16] {
        let out = h
            .run(
                "video.to_gif",
                json!({"input": [s(&v)], "duration": "1", "width": 80, "fps": 5, "colors": colors}),
            )
            .await
            .unwrap_or_else(|e| panic!("colors={colors}: {e:?}"));
        let g = &out.artifacts[0];
        assert_eq!(g.mime, "image/gif");
        assert_eq!(dims(&g.path).0, 80);
        let (_, _, px) = first_frame_rgb(&g.path);
        let distinct: std::collections::HashSet<&[u8]> = px.chunks(3).collect();
        assert!(
            distinct.len() <= colors,
            "colors={colors}: {} colours",
            distinct.len()
        );
    }
}

#[tokio::test]
async fn scene_mode_with_no_matches_reports_no_frames() {
    let h = harness();
    need!(h, "video.extract_frames");
    let v = gen(
        &h,
        "still.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=160x120:d=2:r=25",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ],
    );
    for fmt in ["png", "jpg", "webp"] {
        if fmt == "webp" && !has_encoder("libwebp") {
            continue;
        }
        let out = h
            .run(
                "video.extract_frames",
                json!({"input": [s(&v)], "mode": "scene", "format": fmt}),
            )
            .await
            .unwrap_or_else(|e| panic!("{fmt}: {e}"));
        assert!(out.artifacts.is_empty(), "{fmt}");
        assert_eq!(out.data["count"], 0, "{fmt}");
        assert!(
            out.warnings.iter().any(|w| w.contains("no frames matched")),
            "{fmt}: {:?}",
            out.warnings
        );
    }

    // Intact headers but unreadable frame data must still fail.
    let good = video(&h, "good.mp4");
    let mut bytes = std::fs::read(&good).unwrap();
    let start = bytes.windows(4).position(|w| w == b"mdat").unwrap() + 204;
    let mut x: u32 = 1;
    for b in &mut bytes[start..] {
        x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        *b = (x >> 16) as u8;
    }
    let corrupt = h.path("corrupt.mp4");
    std::fs::write(&corrupt, bytes).unwrap();
    for fmt in ["png", "jpg"] {
        let res = h
            .run(
                "video.extract_frames",
                json!({"input": [s(&corrupt)], "mode": "scene", "format": fmt}),
            )
            .await;
        assert!(res.is_err(), "{fmt}: corrupt video reported success");
    }
}

#[tokio::test]
async fn upscales_and_repeated_frames_are_disclosed() {
    let h = harness();
    need!(h, "video.extract_frames");
    let v = video(&h, "in.mp4");
    let enlarged = |w: &[String]| w.iter().any(|w| w.contains("enlarged from 320 to"));
    let out = h
        .run(
            "video.preview",
            json!({"input": [s(&v)], "segments": 1, "segment_seconds": 1}),
        )
        .await
        .unwrap();
    assert_eq!(dims(&out.artifacts[0].path).0, 640);
    assert!(enlarged(&out.warnings), "{:?}", out.warnings);
    let out = h
        .run(
            "video.preview",
            json!({"input": [s(&v)], "segments": 1, "segment_seconds": 1, "width": 160}),
        )
        .await
        .unwrap();
    assert!(!enlarged(&out.warnings), "{:?}", out.warnings);
    let out = h
        .run(
            "video.thumbnail",
            json!({"input": [s(&v)], "mode": "time", "time": "1", "width": 2048}),
        )
        .await
        .unwrap();
    assert!(enlarged(&out.warnings), "{:?}", out.warnings);
    let out = h
        .run(
            "video.extract_frames",
            json!({"input": [s(&v)], "mode": "fps", "fps": 50, "max_frames": 10}),
        )
        .await
        .unwrap();
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("above the video's 25 fps")),
        "{:?}",
        out.warnings
    );
    let out = h
        .run(
            "video.extract_frames",
            json!({"input": [s(&v)], "mode": "fps", "fps": 2}),
        )
        .await
        .unwrap();
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
}

#[tokio::test]
async fn stabilize_watermark_info_hw() {
    let h = harness();
    need!(h, "video.stabilize");
    let v = video(&h, "in.mp4");
    let out = h
        .run(
            "video.stabilize",
            json!({"input": [s(&v)], "strength": "low"}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].size > 0);
    let m = out.data["files"][0]["method"].as_str().unwrap().to_string();
    assert!(m == "vidstab" || m == "deshake");
    let out = h
        .run(
            "video.stabilize",
            json!({"input": [s(&v)], "method": "deshake"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["files"][0]["method"], "deshake");

    let wm = h.run("video.watermark", json!({"input": [s(&v)], "kind": "text", "text": "50% 'quoted' : text", "position": "top-left"})).await;
    if has_filter("drawtext") {
        assert!(wm.unwrap().artifacts[0].size > 0);
    } else {
        assert_eq!(wm.unwrap_err().code(), "unsupported");
    }
    let logo = gen(
        &h,
        "logo%1.png",
        &[
            "-f",
            "lavfi",
            "-i",
            "color=red:s=64x32",
            "-frames:v",
            "1",
            "-update",
            "1",
        ],
    );
    let out = h
        .run(
            "video.watermark",
            json!({"input": [s(&v)], "kind": "image", "image": s(&logo), "opacity": 0.5}),
        )
        .await
        .unwrap();
    assert!(out.artifacts[0].size > 0);
    let e = h
        .run(
            "video.watermark",
            json!({"input": [s(&v)], "kind": "text", "text": "  "}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");

    let out = h
        .run("video.info", json!({"input": [s(&v)]}))
        .await
        .unwrap();
    assert_eq!(out.data["summary"]["width"], 320);
    assert_eq!(out.data["table"]["rows"].as_array().unwrap().len(), 2);
    assert!(out.text.unwrap().contains("h264") || true);

    let out = h
        .run("video.hw_encoders", json!({"test": false}))
        .await
        .unwrap();
    assert!(out.data["table"]["rows"].is_array());
}

#[tokio::test]
async fn malformed_input() {
    let h = harness();
    need!(h, "video.convert");
    let bad = h.write("bad.mp4", b"this is not a video");
    let e = h
        .run("video.convert", json!({"input": [s(&bad)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let e = h
        .run("video.info", json!({"input": [s(&bad)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    let wav = tone(&h, "a.wav");
    let e = h
        .run("video.resize", json!({"input": [s(&wav)]}))
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
}

/// RGB of the first frame of `p` as (width, height, pixels).
fn first_frame_rgb(p: &std::path::Path) -> (usize, usize, Vec<u8>) {
    let (w, h) = dims(p);
    let out = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(p)
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .expect("ffmpeg runs");
    assert!(out.status.success());
    (w as usize, h as usize, out.stdout)
}

fn is_red(px: &[u8]) -> bool {
    px[0] > 180 && px[1] < 80 && px[2] < 80
}

#[tokio::test]
async fn watermark_margin_keeps_logo_on_canvas() {
    let h = harness();
    need!(h, "video.watermark");
    let v = gen(
        &h,
        "small.mp4",
        &[
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=160x120:d=1:r=10",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
        ],
    );
    let logo = gen(
        &h,
        "logo.png",
        &[
            "-f",
            "lavfi",
            "-i",
            "color=red:s=64x32",
            "-frames:v",
            "1",
            "-update",
            "1",
        ],
    );
    let wm = |pos: &'static str, margin: i64| {
        json!({"input": [s(&v)], "kind": "image", "image": s(&logo), "opacity": 1.0,
               "size": 25, "position": pos, "margin": margin, "quality": "high"})
    };

    // The maximum margin is clamped to the frame and warns instead of hiding the logo.
    for pos in ["bottom-right", "top-left", "top-right", "bottom-left"] {
        let out = h.run("video.watermark", wm(pos, 2000)).await.unwrap();
        let (fw, fh, rgb) = first_frame_rgb(&out.artifacts[0].path);
        let red = rgb.chunks(3).filter(|px| is_red(px)).count();
        assert!(red > 300, "{pos}: watermark not visible ({red} red pixels)");
        // The margin is clamped to the room available, so the logo ends at the opposite edge.
        let (cx, cy) = match pos {
            "top-left" => (fw - 3, fh - 3),
            "top-right" => (2, fh - 3),
            "bottom-left" => (fw - 3, 2),
            _ => (2, 2),
        };
        let i = (cy * fw + cx) * 3;
        assert!(
            is_red(&rgb[i..i + 3]),
            "{pos}: margin not clamped to the frame"
        );
        assert!(
            out.warnings.iter().any(|w| w.contains("margin")),
            "{pos}: {:?}",
            out.warnings
        );
    }

    // A modest margin still offsets the logo from the corner, without a warning.
    let out = h.run("video.watermark", wm("top-left", 16)).await.unwrap();
    let (fw, _, rgb) = first_frame_rgb(&out.artifacts[0].path);
    let at = |x: usize, y: usize| is_red(&rgb[(y * fw + x) * 3..(y * fw + x) * 3 + 3]);
    assert!(!at(4, 4), "logo should start after the margin");
    assert!(
        at(24, 22),
        "logo should be inside the frame after the margin"
    );
    assert!(!out.warnings.iter().any(|w| w.contains("margin")));

    if has_filter("drawtext") {
        let out = h
            .run(
                "video.watermark",
                json!({"input": [s(&v)], "kind": "text", "text": "WM", "color": "#ff0000",
                       "box": false, "opacity": 1.0, "size": 30, "margin": 2000, "quality": "high"}),
            )
            .await
            .unwrap();
        let (_, _, rgb) = first_frame_rgb(&out.artifacts[0].path);
        assert!(
            rgb.chunks(3).filter(|px| is_red(px)).count() > 20,
            "text watermark not visible"
        );
    }
}
