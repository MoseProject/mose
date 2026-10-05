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

#[derive(Debug)]
pub enum RenderError {
    OpenInput,
    StreamInfo,
    AllocOutput,
    NewStream,
    OpenOutput,
    WriteHeader,
    WriteFrame,
    /// `source` has no audio stream to normalize.
    NoAudioStream,
    Decoder,
    FilterGraph,
    Encoder,
    /// A decode/filter/encode call failed mid-render (not at setup).
    Pipeline,
    /// The FFI bridge returned a status this crate doesn't know about.
    Unknown(i32),
    /// (`render_timeline_export` only) a segment has no video stream.
    NoVideoStream,
    /// (`render_timeline_export` only) a later clip's audio format doesn't match the first
    /// clip's — see `avbridge::EncodeError::AudioFormatMismatch`.
    AudioFormatMismatch,
    /// (`render_timeline_export` only) the sequence's video track is missing or empty.
    EmptyTimeline,
    /// (`render_timeline_export` only) a clip's `asset_id` isn't in the project's media
    /// library.
    MissingAsset,
    /// A caller supplied a canvas with no renderable dimensions or frame rate.
    InvalidCanvas,
    /// A caller supplied a timeline segment with an invalid source range or render parameter.
    InvalidTimelineSegment,
    /// Loudness targets must be finite before crossing the native encoder boundary.
    InvalidTargetLufs,
    /// A text overlay cannot be shaped within the engine's bounded resource contract.
    InvalidTextLayout,
    /// An audio mix contributor has an invalid source range or filter parameter.
    InvalidAudioSegment,
    /// The native multi-track audio mixing pass failed after the video render completed.
    AudioMix(String),
    /// Stream-copying the rendered video together with the mixed audio failed.
    MediaMux(String),
    /// Replacing the rendered file with a completed post-processing result failed.
    ReplaceOutput(std::io::Error),
    /// A clip's [`crate::timeline::ClipInstance::nested_sequence_id`] doesn't resolve to any
    /// [`crate::project::Sequence`] in the project.
    MissingNestedSequence,
    /// A nested-sequence clip (transitively) contains a clip nesting the same sequence again —
    /// [`crate::nested_sequence::materialize_nested_sequences`]'s cycle guard.
    CyclicNestedSequence,
    /// Probing a rendered nested-sequence file (to build its synthetic [`crate::MediaAsset`])
    /// failed.
    ProbeNestedSequence,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::OpenInput => write!(f, "failed to open input"),
            RenderError::StreamInfo => write!(f, "failed to read stream info"),
            RenderError::AllocOutput => write!(f, "failed to allocate output context"),
            RenderError::NewStream => write!(f, "failed to create an output stream"),
            RenderError::OpenOutput => write!(f, "failed to open output for writing"),
            RenderError::WriteHeader => write!(f, "failed to write output header"),
            RenderError::WriteFrame => write!(f, "failed to write a frame"),
            RenderError::NoAudioStream => write!(f, "source has no audio stream"),
            RenderError::Decoder => write!(f, "failed to open the audio decoder"),
            RenderError::FilterGraph => write!(f, "failed to build the audio filter graph"),
            RenderError::Encoder => write!(f, "failed to open the AAC encoder"),
            RenderError::Pipeline => write!(f, "decode/filter/encode pipeline failed mid-render"),
            RenderError::Unknown(code) => write!(f, "unknown render status code: {code}"),
            RenderError::NoVideoStream => write!(f, "a clip's source has no video stream"),
            RenderError::AudioFormatMismatch => {
                write!(
                    f,
                    "a clip's audio format doesn't match the sequence's first clip"
                )
            }
            RenderError::EmptyTimeline => write!(f, "sequence has no video clips to export"),
            RenderError::MissingAsset => write!(f, "a clip references a missing media asset"),
            RenderError::InvalidCanvas => {
                write!(f, "render canvas has invalid dimensions or frame rate")
            }
            RenderError::InvalidTimelineSegment => {
                write!(f, "timeline contains an invalid render segment")
            }
            RenderError::InvalidTargetLufs => write!(f, "loudness target must be finite"),
            RenderError::InvalidTextLayout => write!(f, "text overlay has invalid layout input"),
            RenderError::InvalidAudioSegment => write!(f, "audio mix contains an invalid segment"),
            RenderError::AudioMix(message) => write!(f, "audio mix failed: {message}"),
            RenderError::MediaMux(message) => write!(f, "audio mux failed: {message}"),
            RenderError::ReplaceOutput(error) => write!(f, "failed to replace export: {error}"),
            RenderError::MissingNestedSequence => {
                write!(
                    f,
                    "a compound clip references a sequence that no longer exists"
                )
            }
            RenderError::CyclicNestedSequence => {
                write!(f, "a compound clip (transitively) nests its own sequence")
            }
            RenderError::ProbeNestedSequence => {
                write!(f, "failed to probe a rendered nested-sequence file")
            }
        }
    }
}

impl std::error::Error for RenderError {}

impl From<avbridge::EncodeError> for RenderError {
    fn from(e: avbridge::EncodeError) -> Self {
        use avbridge::EncodeError as E;
        match e {
            E::InvalidPath(_) => RenderError::OpenInput,
            E::OpenInput => RenderError::OpenInput,
            E::StreamInfo => RenderError::StreamInfo,
            E::AllocOutput => RenderError::AllocOutput,
            E::NewStream => RenderError::NewStream,
            E::OpenOutput => RenderError::OpenOutput,
            E::WriteHeader => RenderError::WriteHeader,
            E::WriteFrame => RenderError::WriteFrame,
            E::NoAudioStream => RenderError::NoAudioStream,
            E::Decoder => RenderError::Decoder,
            E::FilterGraph => RenderError::FilterGraph,
            E::Encoder => RenderError::Encoder,
            E::Pipeline => RenderError::Pipeline,
            E::NoVideoStream => RenderError::NoVideoStream,
            E::AudioFormatMismatch => RenderError::AudioFormatMismatch,
            E::EmptyTimeline => RenderError::EmptyTimeline,
            E::Unknown(code) => RenderError::Unknown(code),
        }
    }
}

/// How a render ended: all the way through, or stopped early because `cancel` was set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOutcome {
    Completed,
    Cancelled,
}
