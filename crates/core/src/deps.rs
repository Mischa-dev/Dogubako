//! External dependency detection.
//!
//! Dogubako integrates mature external programs (FFmpeg, qpdf, Tesseract,
//! ...). This module knows how to find them, check versions and explain how
//! to install them. Nothing is ever installed system-wide automatically.

use crate::engines::{Lookup, ManagedEngines, Origin};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct InstallHints {
    pub apt: Option<&'static str>,
    pub dnf: Option<&'static str>,
    pub pacman: Option<&'static str>,
    pub brew: Option<&'static str>,
    pub winget: Option<&'static str>,
    pub url: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub purpose: &'static str,
    /// Candidate executable names, tried in order.
    pub binaries: &'static [&'static str],
    /// Candidates only valid on non-Windows (e.g. ImageMagick 6 `convert`).
    pub unix_only_binaries: &'static [&'static str],
    pub version_args: &'static [&'static str],
    /// Regex with one capture group for the version.
    pub version_regex: &'static str,
    pub license: &'static str,
    pub install: InstallHints,
}

macro_rules! hints {
    ($apt:expr, $dnf:expr, $pacman:expr, $brew:expr, $winget:expr, $url:expr) => {
        InstallHints {
            apt: $apt,
            dnf: $dnf,
            pacman: $pacman,
            brew: $brew,
            winget: $winget,
            url: $url,
        }
    };
}

/// Known external dependencies.
pub static KNOWN_DEPS: &[DepSpec] = &[
    DepSpec {
        id: "ffmpeg",
        name: "FFmpeg",
        purpose: "Video and audio conversion, filtering and encoding",
        binaries: &["ffmpeg"],
        unix_only_binaries: &[],
        version_args: &["-version"],
        version_regex: r"ffmpeg version (\S+)",
        license: "LGPL-2.1+ / GPL-2.0+ depending on build",
        install: hints!(
            Some("ffmpeg"),
            Some("ffmpeg-free"),
            Some("ffmpeg"),
            Some("ffmpeg"),
            Some("Gyan.FFmpeg"),
            "https://ffmpeg.org/download.html"
        ),
    },
    DepSpec {
        id: "ffprobe",
        name: "FFprobe",
        purpose: "Media stream and metadata inspection",
        binaries: &["ffprobe"],
        unix_only_binaries: &[],
        version_args: &["-version"],
        version_regex: r"ffprobe version (\S+)",
        license: "LGPL-2.1+ / GPL-2.0+ depending on build",
        install: hints!(
            Some("ffmpeg"),
            Some("ffmpeg-free"),
            Some("ffmpeg"),
            Some("ffmpeg"),
            Some("Gyan.FFmpeg"),
            "https://ffmpeg.org/download.html"
        ),
    },
    DepSpec {
        id: "imagemagick",
        name: "ImageMagick",
        purpose: "Extended image formats (HEIC, PSD, ...) and advanced image operations",
        binaries: &["magick"],
        unix_only_binaries: &["convert"],
        version_args: &["-version"],
        version_regex: r"ImageMagick (\S+)",
        license: "ImageMagick License (Apache-2.0 style)",
        install: hints!(
            Some("imagemagick"),
            Some("ImageMagick"),
            Some("imagemagick"),
            Some("imagemagick"),
            Some("ImageMagick.ImageMagick"),
            "https://imagemagick.org/script/download.php"
        ),
    },
    DepSpec {
        id: "exiftool",
        name: "ExifTool",
        purpose: "Read and write metadata for images, video, audio and documents",
        binaries: &["exiftool"],
        unix_only_binaries: &[],
        version_args: &["-ver"],
        version_regex: r"^(\d+\.\d+)",
        license: "Artistic-1.0 / GPL-1.0+ (Perl terms)",
        install: hints!(
            Some("libimage-exiftool-perl"),
            Some("perl-Image-ExifTool"),
            Some("perl-image-exiftool"),
            Some("exiftool"),
            Some("OliverBetz.ExifTool"),
            "https://exiftool.org/"
        ),
    },
    DepSpec {
        id: "qpdf",
        name: "qpdf",
        purpose: "PDF merging, splitting, rotation, encryption and repair",
        binaries: &["qpdf"],
        unix_only_binaries: &[],
        version_args: &["--version"],
        version_regex: r"qpdf version (\S+)",
        license: "Apache-2.0",
        install: hints!(
            Some("qpdf"),
            Some("qpdf"),
            Some("qpdf"),
            Some("qpdf"),
            Some("QPDF.QPDF"),
            "https://github.com/qpdf/qpdf/releases"
        ),
    },
    DepSpec {
        id: "poppler",
        name: "Poppler utilities",
        purpose: "PDF text extraction, rendering and image extraction",
        binaries: &["pdftoppm"],
        unix_only_binaries: &[],
        version_args: &["-v"],
        version_regex: r"pdftoppm version (\S+)",
        license: "GPL-2.0+",
        install: hints!(
            Some("poppler-utils"),
            Some("poppler-utils"),
            Some("poppler"),
            Some("poppler"),
            Some("oschwartz10612.Poppler"),
            "https://poppler.freedesktop.org/"
        ),
    },
    DepSpec {
        id: "ghostscript",
        name: "Ghostscript",
        purpose: "PDF compression and PostScript processing",
        binaries: &["gs", "gswin64c", "gswin32c"],
        unix_only_binaries: &[],
        version_args: &["--version"],
        version_regex: r"^(\S+)",
        license: "AGPL-3.0",
        install: hints!(
            Some("ghostscript"),
            Some("ghostscript"),
            Some("ghostscript"),
            Some("ghostscript"),
            Some("ArtifexSoftware.GhostScript"),
            "https://ghostscript.com/releases/"
        ),
    },
    DepSpec {
        id: "tesseract",
        name: "Tesseract OCR",
        purpose: "Optical character recognition for images and scanned PDFs",
        binaries: &["tesseract"],
        unix_only_binaries: &[],
        version_args: &["--version"],
        version_regex: r"tesseract v?(\S+)",
        license: "Apache-2.0",
        install: hints!(
            Some("tesseract-ocr"),
            Some("tesseract"),
            Some("tesseract"),
            Some("tesseract"),
            Some("UB-Mannheim.TesseractOCR"),
            "https://tesseract-ocr.github.io/tessdoc/Installation.html"
        ),
    },
    DepSpec {
        id: "whisper",
        name: "whisper.cpp",
        purpose: "Offline speech recognition",
        binaries: &["whisper-cli", "whisper-cpp", "whisper.cpp"],
        unix_only_binaries: &[],
        // whisper-cli prints no version; the app's own copy reports its pin.
        version_args: &["--help"],
        version_regex: r"whisper\.cpp v?(\d+\.\d+\.\d+)",
        license: "MIT",
        install: hints!(
            None,
            None,
            Some("whisper.cpp"),
            Some("whisper-cpp"),
            None,
            "https://github.com/ggml-org/whisper.cpp#quick-start"
        ),
    },
    DepSpec {
        id: "piper",
        name: "Piper TTS",
        purpose: "Offline neural text-to-speech",
        binaries: &["piper"],
        unix_only_binaries: &[],
        version_args: &["--version"],
        version_regex: r"(\d+\.\d+\.\d+)",
        license: "MIT (engine); voices have individual licenses",
        install: hints!(
            None,
            None,
            None,
            None,
            None,
            "https://github.com/rhasspy/piper/releases"
        ),
    },
    DepSpec {
        id: "rembg",
        name: "rembg",
        purpose: "Background removal with local U²-Net models",
        binaries: &["rembg"],
        unix_only_binaries: &[],
        version_args: &["--version"],
        version_regex: r"(\d+\.\d+\.\d+)",
        license: "MIT (models: Apache-2.0 / various)",
        install: hints!(
            None,
            None,
            None,
            None,
            None,
            "https://github.com/danielgatis/rembg#installation"
        ),
    },
    DepSpec {
        id: "realesrgan",
        name: "Real-ESRGAN (ncnn)",
        purpose: "Local AI image upscaling",
        binaries: &["realesrgan-ncnn-vulkan"],
        unix_only_binaries: &[],
        version_args: &["-h"],
        version_regex: r"(Usage)",
        license: "BSD-3-Clause",
        install: hints!(
            None,
            None,
            None,
            None,
            None,
            "https://github.com/xinntao/Real-ESRGAN-ncnn-vulkan/releases"
        ),
    },
    DepSpec {
        id: "libreoffice",
        name: "LibreOffice",
        purpose: "Office document conversion (DOCX, ODT, XLSX, PPTX → PDF, ...)",
        binaries: &["soffice", "libreoffice"],
        unix_only_binaries: &[],
        version_args: &["--version"],
        version_regex: r"LibreOffice (\S+)",
        license: "MPL-2.0",
        install: hints!(
            Some("libreoffice-core"),
            Some("libreoffice-core"),
            Some("libreoffice-fresh"),
            Some("--cask libreoffice"),
            Some("TheDocumentFoundation.LibreOffice"),
            "https://www.libreoffice.org/download/"
        ),
    },
    DepSpec {
        id: "pandoc",
        name: "Pandoc",
        purpose: "Markup document conversion (Markdown, DOCX, HTML, EPUB, ...)",
        binaries: &["pandoc"],
        unix_only_binaries: &[],
        version_args: &["--version"],
        version_regex: r"pandoc (\S+)",
        license: "GPL-2.0+",
        install: hints!(
            Some("pandoc"),
            Some("pandoc"),
            Some("pandoc"),
            Some("pandoc"),
            Some("JohnMacFarlane.Pandoc"),
            "https://pandoc.org/installing.html"
        ),
    },
    DepSpec {
        id: "7zip",
        name: "7-Zip",
        purpose: "7z / RAR archive extraction and creation",
        binaries: &["7zz", "7z", "7za"],
        unix_only_binaries: &[],
        version_args: &["i"],
        version_regex: r"7-Zip[^\d]*(\d+\.\d+)",
        license: "LGPL-2.1+ (unRAR restriction applies to RAR code)",
        install: hints!(
            Some("7zip"),
            Some("p7zip-plugins"),
            Some("7zip"),
            Some("sevenzip"),
            Some("7zip.7zip"),
            "https://www.7-zip.org/download.html"
        ),
    },
    DepSpec {
        id: "bwrap",
        name: "Bubblewrap",
        purpose: "Optional sandbox for external programs (Linux)",
        binaries: &["bwrap"],
        unix_only_binaries: &[],
        version_args: &["--version"],
        version_regex: r"bubblewrap (\S+)",
        license: "LGPL-2.0+",
        install: hints!(
            Some("bubblewrap"),
            Some("bubblewrap"),
            Some("bubblewrap"),
            None,
            None,
            "https://github.com/containers/bubblewrap"
        ),
    },
];

pub fn dep_spec(id: &str) -> Option<&'static DepSpec> {
    KNOWN_DEPS.iter().find(|d| d.id == id)
}

#[derive(Debug, Clone, Serialize)]
pub struct DepStatus {
    pub id: String,
    pub name: String,
    pub purpose: String,
    pub found: bool,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub license: String,
    pub install_hint: String,
    pub install: InstallHints,
    /// Where the program was found: `bundled` / `downloaded` (the app's own
    /// engines) or `system`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<&'static str>,
    /// Set when the app can download this engine itself: the engine id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloadable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_bytes: Option<u64>,
}

/// Caches detection results. Cheap to clone via Arc.
pub struct DepRegistry {
    extra_dirs: Vec<PathBuf>,
    /// Checked after PATH: well-known install locations that a GUI launch
    /// may leave off PATH.
    fallback_dirs: Vec<PathBuf>,
    managed: ManagedEngines,
    cache: RwLock<HashMap<String, DepStatus>>,
}

impl DepRegistry {
    pub fn new(extra_dirs: Vec<PathBuf>) -> Self {
        Self::with_managed(extra_dirs, ManagedEngines::disabled())
    }

    /// Like [`DepRegistry::new`], with the app's own engines checked first
    /// (and, in managed mode, exclusively for the engines they cover).
    pub fn with_managed(extra_dirs: Vec<PathBuf>, managed: ManagedEngines) -> Self {
        DepRegistry {
            extra_dirs,
            fallback_dirs: platform_fallback_dirs(),
            managed,
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn managed(&self) -> &ManagedEngines {
        &self.managed
    }

    /// Clear cached detection (after the user installs something).
    pub fn refresh(&self) {
        self.cache.write().unwrap().clear();
    }

    pub fn status(&self, id: &str) -> DepStatus {
        if let Some(s) = self.cache.read().unwrap().get(id) {
            return s.clone();
        }
        let s = self.detect(id);
        self.cache
            .write()
            .unwrap()
            .insert(id.to_string(), s.clone());
        s
    }

    pub fn all(&self) -> Vec<DepStatus> {
        KNOWN_DEPS
            .iter()
            .filter(|d| applies_here(d))
            .map(|d| self.status(d.id))
            .collect()
    }

    pub fn path(&self, id: &str) -> Option<PathBuf> {
        self.status(id).path
    }

    pub fn available(&self, id: &str) -> bool {
        self.status(id).found
    }

    fn find_binary(&self, spec: &DepSpec) -> (Option<PathBuf>, Lookup) {
        let env_key = format!(
            "DOGUBAKO_{}_PATH",
            spec.id.to_ascii_uppercase().replace('-', "_")
        );
        if let Ok(p) = std::env::var(&env_key) {
            let p = PathBuf::from(p);
            if p.is_file() {
                return (Some(p), Lookup::Unmanaged);
            }
        }
        let mut names: Vec<&str> = spec.binaries.to_vec();
        if !cfg!(windows) {
            names.extend_from_slice(spec.unix_only_binaries);
        }
        let managed = self.managed.lookup(spec.id, &names);
        if let Lookup::Found { path, .. } = &managed {
            return (Some(path.clone()), managed);
        }
        if self.managed.exclusive(&managed) {
            return (None, managed);
        }
        (
            search_binary(&names, &self.extra_dirs, &self.fallback_dirs),
            managed,
        )
    }

    /// Extra environment for running `program` (set for the app's own engines).
    pub fn env_for(&self, program: &Path) -> Vec<(String, std::ffi::OsString)> {
        self.managed.env_for(program)
    }

    /// Whether a sibling binary may be searched for on PATH.
    pub fn system_lookup_allowed(&self, id: &str) -> bool {
        !matches!(self.status(id).source, Some("bundled" | "downloaded"))
    }

    fn detect(&self, id: &str) -> DepStatus {
        let Some(spec) = dep_spec(id) else {
            return DepStatus {
                id: id.to_string(),
                name: id.to_string(),
                purpose: String::new(),
                found: false,
                path: None,
                version: None,
                license: String::new(),
                install_hint: format!("unknown dependency '{id}'"),
                install: hints!(None, None, None, None, None, ""),
                source: None,
                downloadable: None,
                download_bytes: None,
            };
        };
        let (path, managed) = self.find_binary(spec);
        let version = path.as_ref().and_then(|p| {
            probe_version(p, spec).or_else(|| match &managed {
                // The app's own copy is the pinned version, even when the
                // binary can't say so itself.
                Lookup::Found { .. } => self
                    .managed
                    .pinned_version(spec.id)
                    .map(|v| v.trim_start_matches('v').to_string()),
                _ => None,
            })
        });
        let (source, downloadable, download_bytes, install_hint) = match managed {
            Lookup::Found { origin, .. } => (
                Some(match origin {
                    Origin::Bundled => "bundled",
                    Origin::Downloaded => "downloaded",
                }),
                None,
                None,
                install_hint(spec),
            ),
            Lookup::Downloadable { engine, bytes } if path.is_none() => (
                None,
                Some(engine.clone()),
                Some(bytes),
                format!(
                    "Download {} ({}) in Settings → Dependencies; Dogubako keeps it in its own folder",
                    spec.name,
                    crate::util::human_size(bytes)
                ),
            ),
            Lookup::Missing { .. } if path.is_none() => (
                None,
                None,
                None,
                format!(
                    "{} should be bundled with Dogubako but is missing; reinstall the app",
                    spec.name
                ),
            ),
            _ => (
                path.as_ref().map(|_| "system"),
                None,
                None,
                install_hint(spec),
            ),
        };
        let install_hint = if path.is_none() && in_docker_image() {
            docker_image_hint(spec)
        } else {
            install_hint
        };
        DepStatus {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            purpose: spec.purpose.to_string(),
            found: path.is_some(),
            path,
            version,
            license: spec.license.to_string(),
            install_hint,
            install: spec.install.clone(),
            source,
            downloadable,
            download_bytes,
        }
    }
}

/// Whether a dependency means anything on this platform (Bubblewrap is a
/// Linux sandbox, so it isn't listed as missing elsewhere).
fn applies_here(spec: &DepSpec) -> bool {
    spec.id != "bwrap" || cfg!(target_os = "linux")
}

/// The app's own directories first, then PATH, then the platform's
/// well-known install locations.
fn search_binary(names: &[&str], extra: &[PathBuf], fallback: &[PathBuf]) -> Option<PathBuf> {
    let in_dirs = |name: &str, dirs: &[PathBuf]| {
        dirs.iter()
            .map(|d| d.join(exe_name(name)))
            .find(|c| c.is_file())
    };
    for name in names {
        if let Some(p) = in_dirs(name, extra) {
            return Some(p);
        }
        if let Ok(p) = which::which(name) {
            return Some(p);
        }
    }
    names.iter().find_map(|name| in_dirs(name, fallback))
}

/// Apps opened from Finder get launchd's minimal PATH, which leaves out
/// Homebrew (Apple silicon and Intel) and MacPorts, so engines installed
/// there would show as missing.
fn platform_fallback_dirs() -> Vec<PathBuf> {
    if cfg!(target_os = "macos") {
        ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"]
            .iter()
            .map(PathBuf::from)
            .collect()
    } else {
        Vec::new()
    }
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn probe_version(path: &Path, spec: &DepSpec) -> Option<String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new(path)
        .args(spec.version_args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    // Bounded wait so a misbehaving binary can't hang detection.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                return None;
            }
        }
    }
    let out = child.wait_with_output().ok()?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let re = regex::RegexBuilder::new(spec.version_regex)
        .multi_line(true)
        .case_insensitive(true)
        .build()
        .ok()?;
    re.captures(&text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim_end_matches(',').to_string())
}

/// Running in the Docker image (the Dockerfile sets `DOGUBAKO_DOCKER_IMAGE=1`).
/// Its programs come only from the image, so a package manager can't add one;
/// a rebuild with a build option can.
pub fn in_docker_image() -> bool {
    std::env::var_os("DOGUBAKO_DOCKER_IMAGE").is_some_and(|v| v == "1")
}

/// The `.env` build option that adds a dependency to the Docker image.
pub fn docker_build_option(id: &str) -> Option<&'static str> {
    match id {
        "libreoffice" => Some("DOGUBAKO_WITH_OFFICE"),
        "whisper" => Some("DOGUBAKO_WITH_WHISPER"),
        "piper" => Some("DOGUBAKO_WITH_PIPER"),
        "rembg" => Some("DOGUBAKO_WITH_REMBG"),
        _ => None,
    }
}

fn docker_image_hint(spec: &DepSpec) -> String {
    match docker_build_option(spec.id) {
        Some(opt) => format!(
            "Not in this Docker image. Set {opt}=1 in .env, then rebuild: docker compose up -d --build"
        ),
        None => format!("{} isn't part of the Docker image", spec.name),
    }
}

pub fn install_hint(spec: &DepSpec) -> String {
    let h = &spec.install;
    let os_hint = if cfg!(target_os = "macos") {
        h.brew.map(|b| format!("brew install {b}"))
    } else if cfg!(windows) {
        h.winget.map(|w| format!("winget install {w}"))
    } else {
        h.apt
            .map(|a| format!("sudo apt install {a}"))
            .or(h.dnf.map(|d| format!("sudo dnf install {d}")))
    };
    match os_hint {
        Some(cmd) => format!("Install {} ({cmd}) or see {}", spec.name, h.url),
        None => format!("Install {} from {}", spec.name, h.url),
    }
}

#[cfg(test)]
mod docker_tests {
    use super::*;

    #[test]
    fn docker_hint_names_the_build_option_or_says_not_included() {
        let office = docker_image_hint(dep_spec("libreoffice").unwrap());
        assert!(office.contains("DOGUBAKO_WITH_OFFICE=1"), "{office}");
        let esrgan = docker_image_hint(dep_spec("realesrgan").unwrap());
        assert_eq!(esrgan, "Real-ESRGAN (ncnn) isn't part of the Docker image");
        for id in ["whisper", "piper", "rembg"] {
            assert!(docker_build_option(id).is_some(), "{id}");
        }
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::*;

    #[test]
    fn macos_checks_homebrew_and_macports() {
        let dirs = platform_fallback_dirs();
        if cfg!(target_os = "macos") {
            for d in ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"] {
                assert!(dirs.contains(&PathBuf::from(d)), "{d}");
            }
        } else {
            assert!(dirs.is_empty());
        }
    }

    #[test]
    fn fallback_dirs_find_binaries_missing_from_path() {
        let (extra, fallback) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let name = "dogubako-test-engine-not-on-path";
        let names = [name];
        let (e, f) = (
            vec![extra.path().to_path_buf()],
            vec![fallback.path().to_path_buf()],
        );
        assert_eq!(search_binary(&names, &e, &f), None);
        let in_fallback = fallback.path().join(exe_name(name));
        std::fs::write(&in_fallback, b"").unwrap();
        assert_eq!(search_binary(&names, &e, &f), Some(in_fallback));
        // The app's own directory still wins over the fallback.
        let in_extra = extra.path().join(exe_name(name));
        std::fs::write(&in_extra, b"").unwrap();
        assert_eq!(search_binary(&names, &e, &f), Some(in_extra));
    }
}

#[cfg(test)]
mod managed_tests {
    use super::*;

    #[test]
    fn managed_engines_shadow_and_exclude_the_system() {
        let t = tempfile::tempdir().unwrap();
        let bundled = t.path().join("engines");
        std::fs::create_dir_all(bundled.join("bin")).unwrap();
        std::fs::write(bundled.join("bin").join(exe_name("qpdf")), b"").unwrap();
        std::fs::write(
            bundled.join("manifest.json"),
            serde_json::json!({
                "schema": 1, "target": "darwin-arm64", "lock_id": "t",
                "bundled": [{"id": "qpdf", "deps": ["qpdf"], "version": "12", "binaries": ["qpdf"], "license": "Apache-2.0"}],
                "on_demand": [{"id": "ghostscript", "deps": ["ghostscript"], "version": "10", "binaries": ["gs"],
                    "license": "AGPL-3.0", "url": "https://example.invalid/gs.tar.gz", "sha256": "00",
                    "download_bytes": 20000000, "installed_bytes": 46000000}]
            })
            .to_string(),
        )
        .unwrap();
        // A "system" gs that managed mode must ignore.
        let sys = t.path().join("sys");
        std::fs::create_dir_all(&sys).unwrap();
        std::fs::write(sys.join(exe_name("gs")), b"").unwrap();
        let managed = ManagedEngines::load(Some(bundled.clone()), t.path().join("installed"));
        if managed.mode() != crate::engines::EngineMode::Managed {
            return; // DOGUBAKO_ENGINES overrides the default in this environment
        }
        let reg = DepRegistry::with_managed(vec![sys], managed);
        let q = reg.status("qpdf");
        assert_eq!(q.source, Some("bundled"));
        assert_eq!(q.path, Some(bundled.join("bin").join(exe_name("qpdf"))));
        // A binary that can't report its version shows the pinned one.
        assert_eq!(q.version.as_deref(), Some("12"));
        let gs = reg.status("ghostscript");
        assert!(!gs.found, "managed engine must not come from the system");
        assert_eq!(gs.downloadable.as_deref(), Some("ghostscript"));
        assert!(
            gs.install_hint.contains("Settings → Dependencies"),
            "{}",
            gs.install_hint
        );
        // Unmanaged deps keep the old lookup.
        assert_eq!(reg.status("libreoffice").downloadable, None);
    }

    #[test]
    fn lists_only_dependencies_for_this_platform() {
        let reg = DepRegistry::new(vec![]);
        let ids: Vec<String> = reg.all().into_iter().map(|d| d.id).collect();
        assert_eq!(
            ids.contains(&"bwrap".to_string()),
            cfg!(target_os = "linux")
        );
        assert!(ids.contains(&"ffmpeg".to_string()));
    }
}
