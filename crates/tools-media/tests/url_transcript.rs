//! speech.transcribe_url against a stand-in yt-dlp that serves recorded
//! metadata and caption files, so no network is needed.
#![cfg(unix)]

use dogubako_core::testing::TestHarness;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const YT_VTT: &str = "WEBVTT\nKind: captions\nLanguage: en\n\n\
00:00:00.000 --> 00:00:02.330 align:start position:0%\n \nall<00:00:00.400><c> right</c><00:00:00.800><c> so</c>\n\n\
00:00:02.330 --> 00:00:02.340 align:start position:0%\nall right so\n \n\n\
00:00:02.340 --> 00:00:05.100 align:start position:0%\nall right so\nhere<00:00:02.800><c> we</c><00:00:03.100><c> are</c> in front of the elephants\n\n\
00:00:05.100 --> 00:00:05.110 align:start position:0%\nhere we are in front of the elephants\n \n";

const YT_FR_VTT: &str =
    "WEBVTT\n\n00:00:00.000 --> 00:00:04.000\nnous voilà devant les éléphants\n";

const TED_VTT: &str = "WEBVTT\n\n00:00:00.000 --> 00:00:00.001\n \n\n\
00:00:02.103 --> 00:00:04.678\nGood morning. How are you?\n\n\
00:00:04.702 --> 00:00:06.105\n(Audience) Good.\n00:00:04.702 --> 00:00:06.105\n(Audience) Good.\n";

fn tracks(keys: &[&str]) -> Value {
    Value::Object(
        keys.iter()
            .map(|k| {
                (
                    k.to_string(),
                    json!([{"ext": "vtt", "url": format!("https://example.invalid/{k}.vtt")}]),
                )
            })
            .collect(),
    )
}

fn fake_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap().keep();
        let w = |n: &str, s: &str| std::fs::write(dir.join(n), s).unwrap();
        w("youtube.json", &json!({
            "id": "jNQXAC9IVRw", "title": "Me at the zoo", "uploader": "jawed",
            "webpage_url": "https://www.youtube.com/watch?v=jNQXAC9IVRw",
            "extractor": "youtube", "extractor_key": "Youtube", "duration": 19,
            "language": "en", "live_status": "not_live",
            "subtitles": {}, "automatic_captions": tracks(&["en", "en-orig", "fr"]),
        }).to_string());
        w("ted.json", &json!({
            "id": "66", "title": "Do schools kill creativity?", "uploader": "Sir Ken Robinson",
            "webpage_url": "https://www.ted.com/talks/sir_ken_robinson_do_schools_kill_creativity",
            "extractor": "TedTalk", "extractor_key": "TedTalk", "duration": 1151, "language": "en",
            "subtitles": tracks(&["de", "en"]),
        }).to_string());
        w("nocaps.json", &json!({
            "id": "nocaps", "title": "Silent film", "webpage_url": "https://vimeo.com/1",
            "extractor_key": "Vimeo", "duration": 100000, "subtitles": {},
        }).to_string());
        w("live.json", &json!({"id": "live", "title": "Live", "live_status": "is_live"}).to_string());
        w("jNQXAC9IVRw.en-orig.vtt", YT_VTT);
        w("jNQXAC9IVRw.fr.vtt", YT_FR_VTT);
        w("66.en.vtt", TED_VTT);
        let d = dir.display();
        let script = format!(
            r#"#!/bin/bash
export PATH=/usr/bin:/bin
fx="{d}"
printf '%s\n' "$*" >> "$fx/argv.log"
for a in "$@"; do [ "$a" = --version ] && {{ echo 2026.10.26; exit 0; }}; done
if [[ " $* " == *" --dump-single-json "* ]]; then
  url="${{@: -1}}"
  case "$url" in
    *bot*) echo "ERROR: [youtube] bot: Sign in to confirm you’re not a bot. Use --cookies-from-browser or --cookies" >&2; exit 1;;
    *gone*) echo "ERROR: [youtube] gone: Video unavailable. This video has been removed" >&2; exit 1;;
    *nosite*) echo "ERROR: Unsupported URL: $url" >&2; exit 1;;
    *playlist*) echo '{{"_type":"playlist","entries":[]}}';;
    *live*) cat "$fx/live.json";;
    *nocaps*) cat "$fx/nocaps.json";;
    *youtube*) cat "$fx/youtube.json";;
    *ted*) cat "$fx/ted.json";;
    *) echo "ERROR: No suitable extractor found for URL $url" >&2; exit 1;;
  esac
  exit 0
fi
prev=""; for a in "$@"; do
  case "$prev" in -P) dir="$a";; --sub-langs) langs="$a";; --load-info-json) info="$a";; esac; prev="$a"
done
key="${{langs#^}}"; key="${{key%\$}}"; key="${{key//\\/}}"
id=$(grep -o '"id":"[^"]*"' "$info" | head -1 | cut -d'"' -f4)
cp "$fx/$id.$key.vtt" "$dir/sub.$key.vtt" || {{ echo "ERROR: Unable to download video subtitles" >&2; exit 1; }}
"#
        );
        let bin = dir.join("yt-dlp");
        std::fs::write(&bin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        // SAFETY: set once before any harness looks up dependencies.
        #[allow(unused_unsafe)]
        unsafe {
            std::env::set_var("DOGUBAKO_YTDLP_PATH", &bin);
        }
        dir
    })
}

fn harness() -> TestHarness {
    fake_dir();
    TestHarness::new(dogubako_tools_media::tools())
}

const YT: &str = "https://www.youtube.com/watch?v=jNQXAC9IVRw";
const TED: &str = "https://www.ted.com/talks/sir_ken_robinson_do_schools_kill_creativity";

#[tokio::test]
async fn youtube_auto_captions_in_spoken_language() {
    let h = harness();
    assert!(h.available("speech.transcribe_url"));
    let out = h
        .run(
            "speech.transcribe_url",
            json!({"url": YT, "formats": ["txt", "srt", "json"]}),
        )
        .await
        .unwrap();
    let t = &out.data["transcript"];
    assert_eq!(t["kind"], "auto_captions");
    assert_eq!(t["language"], "en");
    assert_eq!(t["caption_track"], "en-orig");
    assert_eq!(out.data["source"]["url"], YT);
    assert_eq!(out.data["source"]["site"], "Youtube");
    let segs = out.data["segments"].as_array().unwrap();
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[1]["text"], "here we are in front of the elephants");
    assert_eq!(segs[1]["start"], 2.34);
    let text = out.text.as_deref().unwrap();
    assert!(text.starts_with("# Transcript: Me at the zoo"), "{text}");
    assert!(text.contains("**Source:** <https://www.youtube.com/watch?v=jNQXAC9IVRw> (Youtube)"));
    assert!(text.contains("**Language:** English (en)"));
    assert!(text.contains("auto-generated by the site"));
    assert!(text.contains("all right so"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);

    let names: Vec<String> = out
        .artifacts
        .iter()
        .map(|a| a.path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [
            "Me_at_the_zoo.txt",
            "Me_at_the_zoo.srt",
            "Me_at_the_zoo.json"
        ]
    );
    let txt = std::fs::read_to_string(&out.artifacts[0].path).unwrap();
    assert!(
        txt.contains("all right so here we are in front of the elephants"),
        "{txt}"
    );
    let srt = std::fs::read_to_string(&out.artifacts[1].path).unwrap();
    assert!(
        srt.contains("00:00:02,340 --> 00:00:05,100\nhere we are"),
        "{srt}"
    );
    let j: Value =
        serde_json::from_str(&std::fs::read_to_string(&out.artifacts[2].path).unwrap()).unwrap();
    assert_eq!(j["source"]["url"], YT);
    assert_eq!(j["transcript"]["kind"], "auto_captions");
    assert_eq!(j["segments"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn youtube_translated_track_is_flagged() {
    let h = harness();
    let out = h
        .run(
            "speech.transcribe_url",
            json!({"url": YT, "language": "fr"}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["transcript"]["caption_track"], "fr");
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("machine-translated")),
        "{:?}",
        out.warnings
    );
}

#[tokio::test]
async fn ted_published_captions() {
    let h = harness();
    let out = h
        .run(
            "speech.transcribe_url",
            json!({"url": TED, "formats": ["md", "vtt"]}),
        )
        .await
        .unwrap();
    assert_eq!(out.data["transcript"]["kind"], "captions");
    assert_eq!(out.data["source"]["site"], "TedTalk");
    assert_eq!(
        out.data["available_captions"]["captions"],
        json!(["de", "en"])
    );
    let segs = out.data["segments"].as_array().unwrap();
    assert_eq!(segs.len(), 2, "empty cue dropped");
    let md = std::fs::read_to_string(&out.artifacts[0].path).unwrap();
    assert!(
        md.contains("Good morning. How are you? (Audience) Good.\n"),
        "{md}"
    );
    assert!(!md.contains("-->"), "{md}");
    assert!(out.artifacts[1]
        .path
        .to_string_lossy()
        .ends_with("Do_schools_kill_creativity.vtt"));
    assert!(out.text.as_deref().unwrap().contains("**Duration:** 19:11"));
}

#[tokio::test]
async fn unavailable_language_lists_alternatives() {
    let h = harness();
    let e = h
        .run(
            "speech.transcribe_url",
            json!({"url": TED, "language": "ja"}),
        )
        .await
        .unwrap_err();
    assert_eq!(e.code(), "invalid_input");
    assert!(e.to_string().contains("available: de, en"), "{e}");
}

#[tokio::test]
async fn no_captions() {
    let h = harness();
    let e = h
        .run(
            "speech.transcribe_url",
            json!({"url": "https://vimeo.com/nocaps", "source": "captions"}),
        )
        .await
        .unwrap_err();
    assert!(e.to_string().contains("no captions"), "{e}");
    // Falls back to Whisper: either the setup it needs is explained, or the
    // video is longer than the download limit.
    let e = h
        .run(
            "speech.transcribe_url",
            json!({"url": "https://vimeo.com/nocaps", "max_minutes": 60}),
        )
        .await
        .unwrap_err();
    let msg = e.to_string();
    assert!(
        ["missing_dependency", "missing_model", "invalid_input"].contains(&e.code()),
        "{e}"
    );
    assert!(
        msg.contains("no captions") || msg.contains("60-minute limit"),
        "{msg}"
    );
}

#[tokio::test]
async fn retrieval_errors_are_actionable() {
    let h = harness();
    let run = |url: &str| h.run("speech.transcribe_url", json!({"url": url}));
    let e = run("https://www.youtube.com/watch?v=bot")
        .await
        .unwrap_err();
    assert_eq!(e.code(), "failed");
    assert!(e.to_string().contains("cookies.txt"), "{e}");
    for url in [
        "https://www.youtube.com/watch?v=gone",
        "https://example.com/nosite",
        "https://example.com/page",
        "https://www.youtube.com/playlist?list=1",
        "ftp://example.com/video",
        "not a url",
    ] {
        let e = run(url).await.unwrap_err();
        assert_eq!(e.code(), "invalid_input", "{url}: {e}");
    }
    let e = run("https://www.youtube.com/watch?v=live")
        .await
        .unwrap_err();
    assert_eq!(e.code(), "unsupported", "{e}");
}

#[tokio::test]
async fn cookies_are_passed_as_a_copy() {
    let h = harness();
    let c = h.write("my-cookies.txt", b"# Netscape HTTP Cookie File\n");
    h.run(
        "speech.transcribe_url",
        json!({"url": TED, "cookies": c.to_string_lossy()}),
    )
    .await
    .unwrap();
    let log = std::fs::read_to_string(fake_dir().join("argv.log")).unwrap();
    let line = log
        .lines()
        .find(|l| l.contains("--cookies"))
        .expect("cookies passed");
    assert!(!line.contains("my-cookies.txt"), "{line}");
    assert!(line.contains("--use-extractors default,-generic"), "{line}");
    assert!(std::fs::read(&c).is_ok());
}
