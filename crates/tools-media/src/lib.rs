//! Video, audio and speech tools for Dogubako, built on FFmpeg, whisper.cpp,
//! Piper and the WebRTC voice activity detector.

#![allow(clippy::too_many_arguments)]

use dogubako_core::prelude::*;

pub mod audio;
pub mod ffmpeg;
pub mod speech;
pub mod subtitles;
pub mod vad;
pub mod video;

/// All tools provided by this crate.
pub fn tools() -> Vec<Arc<dyn Tool>> {
    let mut v = vec![];
    v.extend(video::tools());
    v.extend(audio::tools());
    v.extend(subtitles::tools());
    v.extend(speech::tools());
    v.extend(vad::tools());
    v
}
