//! Optional local models (speech recognition, face detection, TTS voices).
//!
//! Models are never bundled. Users download them explicitly; downloads are
//! verified against published checksums where upstream provides them.

use crate::error::{ToolError, ToolResult};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct ModelSpec {
    pub id: &'static str,
    /// Models in a group are interchangeable for a tool (e.g. "whisper").
    pub group: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub url: &'static str,
    /// Additional files that must be downloaded next to the main file
    /// (e.g. Piper voice configs): (url, file_name).
    pub extra: &'static [(&'static str, &'static str)],
    pub file_name: &'static str,
    pub size_bytes: u64,
    pub sha1: Option<&'static str>,
    pub sha256: Option<&'static str>,
    pub license: &'static str,
    pub languages: &'static str,
}

const HF_WHISPER: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

macro_rules! whisper {
    ($id:expr, $file:expr, $name:expr, $desc:expr, $size:expr, $sha1:expr, $langs:expr) => {
        ModelSpec {
            id: $id,
            group: "whisper",
            name: $name,
            description: $desc,
            url: concat!(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/",
                $file
            ),
            extra: &[],
            file_name: $file,
            size_bytes: $size,
            sha1: Some($sha1),
            sha256: None,
            license: "MIT (OpenAI Whisper weights)",
            languages: $langs,
        }
    };
}

/// rembg's models, as published by rembg. The file names are the ones rembg
/// looks for in `U2NET_HOME`, so the tool can point it at the models folder.
macro_rules! rembg {
    ($model:expr, $name:expr, $desc:expr, $size:expr, $sha256:expr, $license:expr) => {
        ModelSpec {
            id: concat!("rembg-", $model),
            group: "rembg",
            name: $name,
            description: $desc,
            url: concat!(
                "https://github.com/danielgatis/rembg/releases/download/v0.0.0/",
                $model,
                ".onnx"
            ),
            extra: &[],
            file_name: concat!($model, ".onnx"),
            size_bytes: $size,
            sha1: None,
            sha256: Some($sha256),
            license: $license,
            languages: "",
        }
    };
}

pub static KNOWN_MODELS: &[ModelSpec] = &[
    whisper!("whisper-tiny", "ggml-tiny.bin", "Whisper tiny", "Fastest, lowest accuracy (75 MiB)", 77_691_713, "bd577a113a864445d4c299885e0cb97d4ba92b5f", "multilingual"),
    whisper!("whisper-tiny.en", "ggml-tiny.en.bin", "Whisper tiny (English)", "Fastest English-only model (75 MiB)", 77_704_715, "c78c86eb1a8faa21b369bcd33207cc90d64ae9df", "en"),
    whisper!("whisper-base", "ggml-base.bin", "Whisper base", "Good balance for everyday use (142 MiB)", 147_951_465, "465707469ff3a37a2b9b8d8f89f2f99de7299dac", "multilingual"),
    whisper!("whisper-base.en", "ggml-base.en.bin", "Whisper base (English)", "English-only base model (142 MiB)", 147_964_211, "137c40403d78fd54d454da0f9bd998f78703390c", "en"),
    whisper!("whisper-small", "ggml-small.bin", "Whisper small", "Higher accuracy, slower (466 MiB)", 487_601_967, "55356645c2b361a969dfd0ef2c5a50d530afd8d5", "multilingual"),
    whisper!("whisper-small.en", "ggml-small.en.bin", "Whisper small (English)", "English-only small model (466 MiB)", 487_614_201, "db8a495a91d927739e50b3fc1cc4c6b8f6c2d022", "en"),
    whisper!("whisper-small.en-tdrz", "ggml-small.en-tdrz.bin", "Whisper small + tinydiarize", "English model that marks speaker turns (465 MiB)", 487_601_967, "b6c6e7e89af1a35c08e6de56b66ca6a02a2fdfa1", "en"),
    whisper!("whisper-large-v3-turbo-q5_0", "ggml-large-v3-turbo-q5_0.bin", "Whisper large-v3 turbo (q5)", "Best accuracy per MiB, quantized (547 MiB)", 574_041_195, "e050f7970618a659205450ad97eb95a18d69c9ee", "multilingual"),
    whisper!("whisper-large-v3-turbo", "ggml-large-v3-turbo.bin", "Whisper large-v3 turbo", "Highest accuracy (1.5 GiB)", 1_624_555_275, "4af2b29d7ec73d781377bfd1758ca957a807e941", "multilingual"),
    ModelSpec {
        id: "silero-vad",
        group: "whisper-vad",
        name: "Silero VAD v5.1.2 (ggml)",
        description: "Neural voice activity detection used by whisper.cpp --vad (885 KiB)",
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
        extra: &[],
        file_name: "ggml-silero-v5.1.2.bin",
        size_bytes: 885_098,
        sha1: None,
        sha256: None,
        license: "MIT",
        languages: "any",
    },
    ModelSpec {
        id: "seeta-face",
        group: "face-detection",
        name: "SeetaFace frontal face detector",
        description: "Small frontal face detection model used for face blurring (1.2 MiB)",
        url: "https://raw.githubusercontent.com/atomashpolskiy/rustface/master/model/seeta_fd_frontal_v1.0.bin",
        extra: &[],
        file_name: "seeta_fd_frontal_v1.0.bin",
        size_bytes: 1_209_904,
        sha1: None,
        sha256: Some("c4619d066ed35e84d9a8e842860b0dff567aba0cbb139881075538761db3ff5d"),
        license: "BSD-2-Clause",
        languages: "",
    },
    ModelSpec {
        id: "piper-en_US-lessac-medium",
        group: "piper",
        name: "Piper voice: English (US) Lessac, medium",
        description: "Natural US English voice for offline text-to-speech (63 MiB)",
        url: "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx",
        extra: &[("https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/medium/en_US-lessac-medium.onnx.json", "en_US-lessac-medium.onnx.json")],
        file_name: "en_US-lessac-medium.onnx",
        size_bytes: 63_201_294,
        sha1: None,
        sha256: None,
        license: "Blizzard / CC BY-SA (see voice card)",
        languages: "en-US",
    },
    ModelSpec {
        id: "piper-en_GB-alan-medium",
        group: "piper",
        name: "Piper voice: English (UK) Alan, medium",
        description: "British English voice for offline text-to-speech (63 MiB)",
        url: "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_GB/alan/medium/en_GB-alan-medium.onnx",
        extra: &[("https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_GB/alan/medium/en_GB-alan-medium.onnx.json", "en_GB-alan-medium.onnx.json")],
        file_name: "en_GB-alan-medium.onnx",
        size_bytes: 63_201_294,
        sha1: None,
        sha256: None,
        license: "CC BY-SA 4.0 (see voice card)",
        languages: "en-GB",
    },
    ModelSpec {
        id: "piper-de_DE-thorsten-medium",
        group: "piper",
        name: "Piper voice: German Thorsten, medium",
        description: "German voice for offline text-to-speech (63 MiB)",
        url: "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/de/de_DE/thorsten/medium/de_DE-thorsten-medium.onnx",
        extra: &[("https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/de/de_DE/thorsten/medium/de_DE-thorsten-medium.onnx.json", "de_DE-thorsten-medium.onnx.json")],
        file_name: "de_DE-thorsten-medium.onnx",
        size_bytes: 63_201_294,
        sha1: None,
        sha256: None,
        license: "CC0 (see voice card)",
        languages: "de-DE",
    },
    rembg!(
        "u2net",
        "U²-Net (general)",
        "General-purpose background removal (168 MiB)",
        175_997_641,
        "8d10d2f3bb75ae3b6d527c77944fc5e7dcd94b29809d47a739a7a728a912b491",
        "Apache-2.0"
    ),
    rembg!(
        "u2netp",
        "U²-Net lite",
        "Small, fast background removal (4.4 MiB)",
        4_574_861,
        "309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8",
        "Apache-2.0"
    ),
    rembg!(
        "u2net_human_seg",
        "U²-Net human segmentation",
        "Cuts out people (168 MiB)",
        175_997_641,
        "01eb6a29a5c4d8edb30b56adad9bb3a2a0535338e480724a213e0acfd2d1c73c",
        "Apache-2.0"
    ),
    rembg!(
        "isnet-general-use",
        "IS-Net (general)",
        "Sharper general-purpose background removal (170 MiB)",
        178_648_008,
        "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a",
        "Apache-2.0"
    ),
    rembg!(
        "isnet-anime",
        "IS-Net anime",
        "Background removal for anime and illustrations (168 MiB)",
        176_069_933,
        "f15622d853e8260172812b657053460e20806f04b9e05147d49af7bed31a6e99",
        "Apache-2.0"
    ),
    rembg!(
        "silueta",
        "Silueta",
        "Compact U²-Net variant (42 MiB)",
        44_173_029,
        "75da6c8d2f8096ec743d071951be73b4a8bc7b3e51d9a6625d63644f90ffeedb",
        "see rembg"
    ),
];

#[allow(dead_code)]
const _HF: &str = HF_WHISPER;

pub fn model_spec(id: &str) -> Option<&'static ModelSpec> {
    KNOWN_MODELS.iter().find(|m| m.id == id)
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    #[serde(flatten)]
    pub spec: ModelSpec,
    pub installed: bool,
    pub path: Option<PathBuf>,
    pub verified: bool,
}

pub struct ModelRegistry {
    dir: PathBuf,
}

impl ModelRegistry {
    pub fn new(dir: PathBuf) -> Self {
        ModelRegistry { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn file_path(&self, spec: &ModelSpec) -> PathBuf {
        self.dir.join(spec.file_name)
    }

    pub fn is_installed(&self, id: &str) -> bool {
        model_spec(id)
            .map(|s| {
                self.file_path(s).is_file()
                    && s.extra.iter().all(|(_, f)| self.dir.join(f).is_file())
            })
            .unwrap_or(false)
    }

    pub fn path(&self, id: &str) -> Option<PathBuf> {
        if self.is_installed(id) {
            model_spec(id).map(|s| self.file_path(s))
        } else {
            None
        }
    }

    /// Installed models in a group, in catalog order.
    pub fn installed_in_group(&self, group: &str) -> Vec<&'static ModelSpec> {
        KNOWN_MODELS
            .iter()
            .filter(|m| m.group == group && self.is_installed(m.id))
            .collect()
    }

    pub fn group_available(&self, group: &str) -> bool {
        !self.installed_in_group(group).is_empty()
    }

    pub fn list(&self) -> Vec<ModelStatus> {
        KNOWN_MODELS
            .iter()
            .map(|m| {
                let installed = self.is_installed(m.id);
                ModelStatus {
                    spec: m.clone(),
                    installed,
                    path: installed.then(|| self.file_path(m)),
                    verified: installed
                        && self.dir.join(format!("{}.verified", m.file_name)).is_file(),
                }
            })
            .collect()
    }

    pub fn remove(&self, id: &str) -> ToolResult<()> {
        let spec =
            model_spec(id).ok_or_else(|| ToolError::input(format!("unknown model '{id}'")))?;
        for f in std::iter::once(spec.file_name).chain(spec.extra.iter().map(|(_, f)| *f)) {
            let _ = std::fs::remove_file(self.dir.join(f));
        }
        let _ = std::fs::remove_file(self.dir.join(format!("{}.verified", spec.file_name)));
        Ok(())
    }

    /// Download a model (and companion files), verifying checksums.
    pub async fn download(
        &self,
        id: &str,
        progress: impl Fn(u64, u64) + Send + Sync,
        cancel: tokio_util::sync::CancellationToken,
    ) -> ToolResult<PathBuf> {
        use futures::StreamExt;
        use sha1::Digest as _;
        use tokio::io::AsyncWriteExt;

        let spec =
            model_spec(id).ok_or_else(|| ToolError::input(format!("unknown model '{id}'")))?;
        std::fs::create_dir_all(&self.dir)?;
        let client = reqwest::Client::builder()
            .user_agent(concat!("dogubako/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| ToolError::failed(e.to_string()))?;
        let mut files: Vec<(&str, &str, bool)> = vec![(spec.url, spec.file_name, true)];
        files.extend(spec.extra.iter().map(|(u, f)| (*u, *f, false)));
        for (url, file_name, primary) in files {
            let resp = client
                .get(url)
                .send()
                .await
                .map_err(|e| ToolError::failed(format!("download failed: {e}")))?;
            if !resp.status().is_success() {
                return Err(ToolError::failed(format!(
                    "download failed: HTTP {} for {url}",
                    resp.status()
                )));
            }
            let total = resp
                .content_length()
                .unwrap_or(if primary { spec.size_bytes } else { 0 });
            let part = self.dir.join(format!("{file_name}.part"));
            let mut out = tokio::fs::File::create(&part).await?;
            let mut s1 = sha1::Sha1::new();
            let mut s256 = sha2::Sha256::new();
            let mut done = 0u64;
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                if cancel.is_cancelled() {
                    drop(out);
                    let _ = std::fs::remove_file(&part);
                    return Err(ToolError::Cancelled);
                }
                let chunk =
                    chunk.map_err(|e| ToolError::failed(format!("download interrupted: {e}")))?;
                s1.update(&chunk);
                sha2::Digest::update(&mut s256, &chunk);
                out.write_all(&chunk).await?;
                done += chunk.len() as u64;
                if primary {
                    progress(done, total);
                }
            }
            out.flush().await?;
            drop(out);
            if primary {
                let got1 = hex::encode(s1.finalize());
                let got256 = hex::encode(sha2::Digest::finalize(s256));
                let mut verified = false;
                if let Some(want) = spec.sha1 {
                    if want != got1 {
                        let _ = std::fs::remove_file(&part);
                        return Err(ToolError::failed(format!(
                            "checksum mismatch for {id}: expected sha1 {want}, got {got1}"
                        )));
                    }
                    verified = true;
                }
                if let Some(want) = spec.sha256 {
                    if want != got256 {
                        let _ = std::fs::remove_file(&part);
                        return Err(ToolError::failed(format!(
                            "checksum mismatch for {id}: expected sha256 {want}, got {got256}"
                        )));
                    }
                    verified = true;
                }
                if verified {
                    std::fs::write(
                        self.dir.join(format!("{file_name}.verified")),
                        format!("sha256 {got256}\n"),
                    )?;
                } else {
                    tracing::warn!(model = id, sha256 = %got256, "no upstream checksum published; recorded local hash");
                    std::fs::write(
                        self.dir.join(format!("{file_name}.sha256")),
                        format!("{got256}\n"),
                    )?;
                }
            }
            std::fs::rename(&part, self.dir.join(file_name))?;
        }
        Ok(self.file_path(spec))
    }
}
