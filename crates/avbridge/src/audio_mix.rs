// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::ffi::{CString, NulError};
use std::os::raw::c_int;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::ffi::{avbridge_mix_audio_timeline, avbridge_mux_video_audio, RawAudioSegment};

/// One independently placed audio contributor in a timeline mix. Unlike [`ClipSegment`], this
/// is valid for both video assets with embedded audio and audio-only assets; no video metadata
/// crosses the FFI boundary for this pass.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AudioSegment {
    pub source_path: std::path::PathBuf,
    pub source_in_secs: f64,
    pub source_out_secs: f64,
    pub timeline_start_secs: f64,
    pub gain_db: f32,
    pub speed_factor: f32,
    /// FFmpeg `volume` filter expression (linear multiplier, `t`-keyed from this segment's own
    /// trim start) driving a keyframed gain ramp instead of the constant `gain_db` above — built
    /// by `avcore::keyframe::gain_filter_db_expr` from a `ClipInstance`'s `gain_keyframes`. Empty
    /// string means "use `gain_db` unchanged" (the common case — no keyframed gain). `#[serde(
    /// default)]` so an `.ocqueue` job persisted before this field existed loads with no ramp.
    #[serde(default)]
    pub gain_keyframe_expr: String,
    /// Audio-ducking role (P2 item 6, "Auto Ducking"): `0` mixes in as-is, `1` is the sidechain
    /// trigger (still plays itself, and ducks every `2` branch under it), `2` is ducked under
    /// trigger branches via `sidechaincompress`. Built by `core`'s
    /// `avcore::timeline::AudioRole::to_duck_role_code()` — this crate doesn't depend on `core`,
    /// so it stays a raw code here rather than that enum, same convention
    /// [`ClipSegment::transition_in`] already uses for `TransitionType`. `#[serde(default)]` so
    /// an `.ocqueue` job persisted before this field existed loads as `0` (no ducking) rather
    /// than failing to parse.
    #[serde(default)]
    pub duck_role: u8,
    /// CF-03 "Gameplay Voice" cleanup (`spec/architecture/competitive-feature-plan.md`) — `true`
    /// splices `highpass=f=80,afftdn=nf=<voice_cleanup_noise_floor_db>,acompressor=threshold=
    /// <voice_cleanup_compressor_threshold_db>dB:ratio=<voice_cleanup_compressor_ratio>:
    /// attack=10:release=250:makeup=1.5,alimiter=limit=<voice_cleanup_ceiling_linear>` between
    /// this branch's own `volume` and `delay` stages in `audio_mix.c`'s `build_mix_graph`, ahead
    /// of the whole-mix `afftdn`/`loudnorm`/`alimiter` mastering pass every export already runs.
    /// Values match `scripts/Watch-Gameplay.ps1`'s own proven chain. `#[serde(default)]` so an
    /// `.ocqueue` job persisted before this field existed loads with cleanup off.
    #[serde(default)]
    pub voice_cleanup_enabled: bool,
    #[serde(default = "default_voice_cleanup_noise_floor_db")]
    pub voice_cleanup_noise_floor_db: f32,
    #[serde(default = "default_voice_cleanup_compressor_threshold_db")]
    pub voice_cleanup_compressor_threshold_db: f32,
    #[serde(default = "default_voice_cleanup_compressor_ratio")]
    pub voice_cleanup_compressor_ratio: f32,
    #[serde(default = "default_voice_cleanup_ceiling_linear")]
    pub voice_cleanup_ceiling_linear: f32,
}

pub(crate) fn default_voice_cleanup_noise_floor_db() -> f32 {
    -30.0
}

pub(crate) fn default_voice_cleanup_compressor_threshold_db() -> f32 {
    -18.0
}

pub(crate) fn default_voice_cleanup_compressor_ratio() -> f32 {
    3.0
}

pub(crate) fn default_voice_cleanup_ceiling_linear() -> f32 {
    0.95
}

/// Result of rendering a standalone multi-track audio mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMixOutcome {
    Completed,
    Cancelled,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioMixError {
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open an audio source")]
    OpenInput,
    #[error("failed to allocate the audio output")]
    AllocOutput,
    #[error("no timeline segment contains audio")]
    NoAudio,
    #[error("failed to build the audio mixing graph")]
    FilterGraph,
    #[error("failed to open the AAC encoder")]
    Encoder,
    #[error("failed to open the audio output")]
    OpenOutput,
    #[error("failed to write the audio header")]
    WriteHeader,
    #[error("audio mixing failed mid-stream")]
    Pipeline,
    #[error("unknown audio mix status code: {0}")]
    Unknown(c_int),
}

/// Mixes `segments` into one AAC stream. Inputs without an audio stream are skipped by the C
/// bridge, allowing silent video overlays to coexist with actual audio tracks.
pub fn mix_audio_timeline(
    segments: &[AudioSegment],
    timeline_duration_secs: f64,
    out_path: &Path,
    target_lufs: f32,
    cancel: &AtomicBool,
) -> Result<AudioMixOutcome, AudioMixError> {
    if segments.is_empty() {
        return Err(AudioMixError::NoAudio);
    }
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(AudioMixError::InvalidPath)?;
    let paths: Vec<CString> = segments
        .iter()
        .map(|seg| {
            CString::new(seg.source_path.to_string_lossy().as_bytes())
                .map_err(AudioMixError::InvalidPath)
        })
        .collect::<Result<_, _>>()?;
    let gain_exprs: Vec<CString> = segments
        .iter()
        .map(|seg| {
            CString::new(seg.gain_keyframe_expr.as_bytes()).map_err(AudioMixError::InvalidPath)
        })
        .collect::<Result<_, _>>()?;
    let raw: Vec<RawAudioSegment> = segments
        .iter()
        .zip(&paths)
        .zip(&gain_exprs)
        .map(|((seg, path), gain_expr)| RawAudioSegment {
            source_path: path.as_ptr(),
            source_in_secs: seg.source_in_secs,
            source_out_secs: seg.source_out_secs,
            timeline_start_secs: seg.timeline_start_secs,
            gain_db: seg.gain_db,
            speed_factor: seg.speed_factor,
            gain_keyframe_expr: gain_expr.as_ptr(),
            duck_role: seg.duck_role as c_int,
            voice_cleanup_enabled: seg.voice_cleanup_enabled as c_int,
            voice_cleanup_noise_floor_db: seg.voice_cleanup_noise_floor_db,
            voice_cleanup_compressor_threshold_db: seg.voice_cleanup_compressor_threshold_db,
            voice_cleanup_compressor_ratio: seg.voice_cleanup_compressor_ratio,
            voice_cleanup_ceiling_linear: seg.voice_cleanup_ceiling_linear,
        })
        .collect();

    // SAFETY: raw and every CString backing its pointers remain alive for the whole call; the C
    // function does not retain them. AtomicBool has a byte-addressable 0/1 representation, the
    // same cancellation contract used by the encode functions above.
    let status = unsafe {
        avbridge_mix_audio_timeline(
            raw.as_ptr(),
            raw.len() as c_int,
            timeline_duration_secs,
            c_out.as_ptr(),
            target_lufs,
            cancel.as_ptr() as *const u8,
        )
    };
    mix_audio_timeline_status(status)
}

#[derive(Debug, thiserror::Error)]
pub enum MediaMuxError {
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open a mux input")]
    OpenInput,
    #[error("failed to allocate the mux output")]
    AllocOutput,
    #[error("video or audio input is missing its required stream")]
    MissingStream,
    #[error("failed to create a mux output stream")]
    NewStream,
    #[error("failed to open the mux output")]
    OpenOutput,
    #[error("failed to write the mux header")]
    WriteHeader,
    #[error("failed while stream-copying mux packets")]
    WriteFrame,
    #[error("unknown media mux status code: {0}")]
    Unknown(c_int),
}

/// Stream-copies the first video stream from `video_path` and first audio stream from
/// `audio_path` into `out_path`.
pub fn mux_video_audio(
    video_path: &Path,
    audio_path: &Path,
    out_path: &Path,
) -> Result<(), MediaMuxError> {
    let c_video = CString::new(video_path.to_string_lossy().as_bytes())
        .map_err(MediaMuxError::InvalidPath)?;
    let c_audio = CString::new(audio_path.to_string_lossy().as_bytes())
        .map_err(MediaMuxError::InvalidPath)?;
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(MediaMuxError::InvalidPath)?;
    // SAFETY: all three CStrings stay alive for the duration of the synchronous call.
    let status =
        unsafe { avbridge_mux_video_audio(c_video.as_ptr(), c_audio.as_ptr(), c_out.as_ptr()) };
    mux_video_audio_status(status)
}

/// Maps the status code the C side returned from `mix_audio_timeline` to its result.
pub(crate) fn mix_audio_timeline_status(status: c_int) -> Result<AudioMixOutcome, AudioMixError> {
    match status {
        0 => Ok(AudioMixOutcome::Completed),
        1 => Err(AudioMixError::OpenInput),
        2 => Err(AudioMixError::AllocOutput),
        3 => Err(AudioMixError::NoAudio),
        4 => Err(AudioMixError::FilterGraph),
        5 => Err(AudioMixError::Encoder),
        6 => Err(AudioMixError::OpenOutput),
        7 => Err(AudioMixError::WriteHeader),
        8 => Err(AudioMixError::Pipeline),
        9 => Ok(AudioMixOutcome::Cancelled),
        other => Err(AudioMixError::Unknown(other)),
    }
}

/// Maps the status code the C side returned from `mux_video_audio` to its result.
pub(crate) fn mux_video_audio_status(status: c_int) -> Result<(), MediaMuxError> {
    match status {
        0 => Ok(()),
        1 => Err(MediaMuxError::OpenInput),
        2 => Err(MediaMuxError::AllocOutput),
        3 => Err(MediaMuxError::MissingStream),
        4 => Err(MediaMuxError::NewStream),
        5 => Err(MediaMuxError::OpenOutput),
        6 => Err(MediaMuxError::WriteHeader),
        7 => Err(MediaMuxError::WriteFrame),
        other => Err(MediaMuxError::Unknown(other)),
    }
}
