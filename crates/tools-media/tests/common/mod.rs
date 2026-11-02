#![allow(dead_code)]
//! Shared fixtures for media tests (generated with ffmpeg's lavfi sources).

use dogubako_core::testing::TestHarness;
use std::path::PathBuf;
use std::process::Command;

pub fn harness() -> TestHarness {
    TestHarness::new(dogubako_tools_media::tools())
}

pub fn has_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run ffmpeg to produce a fixture in the harness directory.
pub fn gen(h: &TestHarness, name: &str, args: &[&str]) -> PathBuf {
    let out = h.path(name);
    let st = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .arg(&out)
        .status()
        .expect("ffmpeg runs");
    assert!(st.success(), "fixture {name} failed");
    out
}

/// 4 s 320x240 H.264/AAC test video.
pub fn video(h: &TestHarness, name: &str) -> PathBuf {
    gen(
        h,
        name,
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=s=320x240:d=4:r=25",
            "-f",
            "lavfi",
            "-i",
            "sine=f=440:d=4",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ],
    )
}

/// 3 s stereo tone as WAV.
pub fn tone(h: &TestHarness, name: &str) -> PathBuf {
    gen(
        h,
        name,
        &[
            "-f",
            "lavfi",
            "-i",
            "sine=f=440:d=3:sample_rate=44100",
            "-ac",
            "2",
        ],
    )
}

pub fn probe_json(p: &std::path::Path) -> serde_json::Value {
    let o = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(p)
        .output()
        .unwrap();
    serde_json::from_slice(&o.stdout).unwrap()
}

pub fn duration(p: &std::path::Path) -> f64 {
    probe_json(p)["format"]["duration"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

pub fn dims(p: &std::path::Path) -> (u64, u64) {
    let j = probe_json(p);
    let v = j["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["codec_type"] == "video")
        .unwrap()
        .clone();
    (v["width"].as_u64().unwrap(), v["height"].as_u64().unwrap())
}

pub fn codec(p: &std::path::Path, kind: &str) -> Option<String> {
    let j = probe_json(p);
    j["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["codec_type"] == kind)
        .and_then(|s| s["codec_name"].as_str().map(String::from))
}

pub fn s(p: &std::path::Path) -> String {
    p.to_string_lossy().to_string()
}
