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

use super::errors::RenderError;
use crate::export::ExportAspectRatio;
use crate::keyframe;
use crate::media::{ColorMatrix, ColorTransfer, MediaAsset};
use crate::project::Sequence;
use crate::timeline::{ClipInstance, TrackKind};
use avbridge::Canvas;

/// Overrides `canvas` width/height to match `ratio`, keeping fps and bitrate unchanged.
/// Returns `canvas` unmodified when `ratio` is [`ExportAspectRatio::Original`].
pub fn apply_export_aspect_ratio(canvas: Canvas, ratio: ExportAspectRatio) -> Canvas {
    if ratio == ExportAspectRatio::Original {
        return canvas;
    }
    let (width, height) = ratio.dims_or((canvas.width, canvas.height));
    Canvas {
        width,
        height,
        ..canvas
    }
}

/// Resolves `sequence`'s first non-empty video track into export segments (in `start_secs`
/// order) plus the canvas every segment gets conformed onto — the snapshot
/// [`crate::export::ExportJob`] takes when a job enters the queue, and what "Add Export"
/// button actually captures, replacing a single raw asset. Covers the effect fields
/// [`crate::timeline::ClipInstance::video_filter_chain`] documents (crop, brightness/contrast/
/// saturation, color filter, vignette, sharpen, chroma key, blur) plus per-clip audio gain and
/// freeze frame ([`crate::timeline::ClipInstance::frozen`], resolved to
/// [`avbridge::ClipSegment::frozen`] rather than a filter string). The complete visible-track
/// audio snapshot is resolved separately by [`resolve_audio_segments`].
///
/// The canvas's resolution/frame rate is the first clip's own probed resolution/fps; its
/// bitrate is a duration-weighted average of every clip's own source bitrate — video is
/// re-encoded here (unlike [`render_export`]'s exact passthrough copy), so an exact
/// source-bitrate match isn't possible once effects require decode+filter+encode; this is the
/// closest equivalent for a multi-clip timeline.
///
/// Builds `clip`'s combined avfilter `video_filter` string (scale/rotation/opacity keyframe
/// stages, from [`ClipInstance::keyframe_video_filter_chain`], spliced onto the front of
/// [`ClipInstance::video_filter_chain`]'s own stages — the same position the old `zoom` stage
/// used to occupy, and [`ClipInstance::layer_scale_x`]/`_y`'s resize stage appended after
/// everything else — it must be the last geometric transform before compositing, so every
/// earlier stage still operates at native resolution) plus its `overlay` position expressions
/// (from [`keyframe::position_overlay_xy_expr`], empty strings when there's nothing to
/// animate). `is_overlay` gates the resize stage — a single/background track's final
/// canvas-size conform has no pad/fit step, so appending it there would hand the encoder a
/// mismatched-resolution frame instead of a smaller picture within the canvas; position/opacity
/// keyframes have the same overlay-only caveat for a compositing reason instead. Shared by
/// [`resolve_timeline_segments`] and [`resolve_timeline_segments_multi`] since both need
/// identical per-clip resolution.
/// A [`avbridge::ClipSegment`]'s own on-timeline duration — plain division for a constant
/// `speed_factor`, [`keyframe::smooth_speed_ramp_duration_secs`]'s log-based integral when
/// `smooth_speed_ramp_end_factor` is set (`> 0.0`, this FFI struct's own "no ramp" sentinel —
/// see its doc comment in `avbridge::lib`). Used for progress-reporting's total-duration
/// estimate, which otherwise silently under/over-counts a ramped segment.
pub(crate) fn clip_segment_duration_secs(segment: &avbridge::ClipSegment) -> f64 {
    let source_duration = segment.source_out_secs - segment.source_in_secs;
    if segment.smooth_speed_ramp_end_factor > 0.0 {
        keyframe::smooth_speed_ramp_duration_secs(
            source_duration,
            segment.speed_factor,
            segment.smooth_speed_ramp_end_factor,
        )
    } else {
        let speed = if segment.speed_factor > 0.0 {
            segment.speed_factor as f64
        } else {
            1.0
        };
        source_duration / speed
    }
}

/// Reference white the HDR to SDR conversion maps to SDR white: 203 nits, the level ITU-R
/// BT.2408 recommends for HDR graphics and game capture.
const HDR_REFERENCE_WHITE_NITS: u32 = 203;
/// Brightest HDR level kept distinct, in multiples of the reference white (about 1000 nits):
/// above it the curve clips.
const HDR_TONE_MAP_PEAK: f32 = 5.0;
/// Where the curve starts to roll highlights off, as a fraction of SDR white. Everything below
/// it is left exactly as it is, which keeps ordinary SDR-range colours unchanged.
const HDR_TONE_MAP_KNEE: f32 = 0.5;

/// The filters that turn an HDR source into Rec.709 SDR, or `None` when the asset does not need
/// it ([`MediaAsset::needs_tone_mapping`]). Goes in front of the clip's own filters, so every
/// effect after it sees SDR. The conversion is explicit about what the source is (PQ or HLG,
/// BT.2020, its range) instead of trusting frame tags, so a source forced to "Tone map" works
/// untagged too. Mobius with a knee, not Hable: Hable darkens SDR-range content (a 200,60,40
/// colour came out 129,39,25), Mobius leaves it alone (199,60,39) and still rolls off real HDR
/// highlights.
pub(crate) fn hdr_tone_map_filter(asset: &MediaAsset) -> Option<String> {
    if !asset.needs_tone_mapping() {
        return None;
    }
    let color = asset.streams.color;
    let transfer = match color.map(|c| c.transfer) {
        Some(ColorTransfer::Hlg) => "arib-std-b67",
        _ => "smpte2084",
    };
    let range = if color.is_some_and(|c| c.full_range) {
        "pc"
    } else {
        "tv"
    };
    Some(format!(
        "zscale=tin={transfer}:min=2020_ncl:pin=2020:rin={range}:t=linear:npl={HDR_REFERENCE_WHITE_NITS},\
         format=gbrpf32le,zscale=p=bt709,\
         tonemap=mobius:param={HDR_TONE_MAP_KNEE}:peak={HDR_TONE_MAP_PEAK}:desat=2,\
         zscale=t=bt709:m=bt709:r=tv,format=yuv420p"
    ))
}

/// The conversion that brings an SDR asset's YUV values to Rec.709 limited range, the one every
/// export is tagged with, or `None` when it already is (or nothing is known about it: assets
/// imported before the colour description existed). A clip is read with the matrix it is tagged
/// with, or by its size when untagged (BT.709 from 1280x720 up, BT.601 below), the rule the
/// preview uses (`avbridge_resolve_colorspace`), so what is shown is what is exported.
pub(crate) fn sdr_normalize_filter(asset: &MediaAsset) -> Option<String> {
    if asset.needs_tone_mapping() {
        return None;
    }
    let color = asset.streams.color?;
    let matrix = match color.matrix {
        ColorMatrix::Bt709 => "bt709",
        ColorMatrix::Bt601 => "bt601",
        ColorMatrix::Bt2020 => "bt2020",
        ColorMatrix::Unspecified | ColorMatrix::Other => match asset.resolution {
            Some((width, height)) if width >= 1280 || height >= 720 => "bt709",
            _ => "bt601",
        },
    };
    if matrix == "bt709" && !color.full_range {
        return None;
    }
    let range = if color.full_range { "pc" } else { "tv" };
    Some(format!(
        "scale=in_color_matrix={matrix}:in_range={range}:out_color_matrix=bt709:out_range=tv"
    ))
}

/// `video_filter` with the asset's colour conversion to Rec.709 SDR in front of it: tone mapping
/// for HDR, a matrix and range conversion for SDR that is not Rec.709 limited range.
fn with_tone_mapping(asset: &MediaAsset, video_filter: String) -> String {
    match hdr_tone_map_filter(asset).or_else(|| sdr_normalize_filter(asset)) {
        Some(tone_map) if video_filter.is_empty() => tone_map,
        Some(tone_map) => format!("{tone_map},{video_filter}"),
        None => video_filter,
    }
}

pub(crate) fn resolve_clip_filters(
    clip: &ClipInstance,
    fps_num: u32,
    fps_den: u32,
    duration_secs: f64,
    is_overlay: bool,
    // The export is one segment per graph, with the encoder numbering its own frames: a filter
    // that restarts timestamps (`zoompan`) is safe there, and not in an overlay graph that pairs
    // layers by timestamp.
    single_graph: bool,
) -> (String, String, String) {
    let base_filter = clip.video_filter_chain();
    let mut video_filter =
        match clip.keyframe_video_filter_chain(fps_num, fps_den, duration_secs, single_graph) {
            Some(kf) if base_filter.is_empty() => kf,
            Some(kf) => format!("{kf},{base_filter}"),
            None => base_filter,
        };
    if is_overlay && clip.has_layer_scale() {
        let resize = format!(
            "scale=iw*{:.4}:ih*{:.4}",
            clip.layer_scale_x, clip.layer_scale_y
        );
        video_filter = if video_filter.is_empty() {
            resize
        } else {
            format!("{video_filter},{resize}")
        };
    }
    let (position_x_expr, position_y_expr) =
        keyframe::position_overlay_xy_expr(&clip.position_keyframes, duration_secs)
            .unwrap_or_default();
    (video_filter, position_x_expr, position_y_expr)
}

/// Fails with [`RenderError::EmptyTimeline`] if the sequence has no video track with at least
/// one clip, [`RenderError::MissingAsset`] if a clip's `asset_id` isn't in `media_library`.
pub fn resolve_timeline_segments(
    sequence: &Sequence,
    media_library: &[MediaAsset],
) -> Result<(Vec<avbridge::ClipSegment>, Canvas), RenderError> {
    let track = sequence
        .timeline
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video && !t.clips.is_empty())
        .ok_or(RenderError::EmptyTimeline)?;

    let mut clips: Vec<_> = track.clips.iter().collect();
    clips.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));

    let mut segments = Vec::with_capacity(clips.len());
    let mut total_duration_secs = 0.0;
    let mut weighted_bitrate_bps_secs = 0.0;
    let mut dimensions_fps = None;
    for clip in &clips {
        let asset = media_library
            .iter()
            .find(|a| a.id == clip.resolved_asset_id())
            .ok_or(RenderError::MissingAsset)?;
        if dimensions_fps.is_none() {
            let (width, height) = asset.resolution.ok_or(RenderError::NoVideoStream)?;
            dimensions_fps = Some((width, height, asset.fps.unwrap_or(30.0)));
        }
        let duration_secs = clip.duration_secs();
        total_duration_secs += duration_secs;
        weighted_bitrate_bps_secs += asset.source_bitrate_mbps as f64 * 1_000_000.0 * duration_secs;
        let (_, _, fps) = dimensions_fps.expect("just set above if it was None");
        let (fps_num, fps_den) = fps_to_rational(fps);
        let (video_filter, position_x_expr, position_y_expr) =
            resolve_clip_filters(clip, fps_num, fps_den, duration_secs, false, true);
        let video_filter = with_tone_mapping(asset, video_filter);
        segments.push(avbridge::ClipSegment {
            source_path: asset.source_path.clone(),
            source_in_secs: clip.source_in_secs,
            source_out_secs: clip.source_out_secs,
            gain_db: clip.gain_db,
            video_filter,
            frozen: clip.frozen,
            speed_factor: clip.speed_factor,
            smooth_speed_ramp_end_factor: clip.speed_ramp_end_factor.unwrap_or(0.0),
            position_x_expr,
            position_y_expr,
            transition_in: clip.transition_in.to_export_code(),
            transition_duration_secs: clip.transition_duration_secs,
            timeline_start_secs: clip.start_secs,
            // Ignored by avbridge_encode_timeline_export (this single-track path) regardless —
            // there's no compositing stage for the alpha to survive into, same reasoning as
            // resolve_clip_filters' is_overlay gate above.
            mask_video_path: String::new(),
            // Same reasoning as mask_video_path above — blend_mode is only meaningful on an
            // overlay track, which this single-track path never has.
            blend_mode: String::new(),
            anchor_x: clip.anchor_x,
            anchor_y: clip.anchor_y,
            fade_in_secs: clip.fade_in_secs,
            fade_out_secs: clip.fade_out_secs,
        });
    }
    let (width, height, fps) = dimensions_fps.ok_or(RenderError::EmptyTimeline)?;
    let (fps_num, fps_den) = fps_to_rational(fps);
    let bit_rate_bps = if total_duration_secs > 0.0 {
        (weighted_bitrate_bps_secs / total_duration_secs) as i64
    } else {
        0
    };
    let canvas = Canvas {
        width,
        height,
        fps_num,
        fps_den,
        bit_rate_bps,
    };

    Ok((segments, canvas))
}

/// Resolves all visible video tracks in `sequence` into per-track segment lists, suitable for
/// passing to [`render_export_job_multi`].  The returned `Vec` has one inner `Vec<ClipSegment>`
/// per visible video track that has at least one clip; the canvas is derived from the first
/// clip of the first track (same logic as [`resolve_timeline_segments`]).
///
/// When the sequence has exactly one visible video track, the result is a single-element outer
/// `Vec` that round-trips cleanly through [`render_export_job_multi`] (which delegates to
/// [`render_export_job`] in that case).
///
/// Fails with [`RenderError::EmptyTimeline`] if there are no visible video tracks with clips,
/// [`RenderError::MissingAsset`] if any clip's `asset_id` isn't in `media_library`.
pub fn resolve_timeline_segments_multi(
    sequence: &Sequence,
    media_library: &[MediaAsset],
) -> Result<(Vec<Vec<avbridge::ClipSegment>>, Canvas), RenderError> {
    let visible_video_tracks: Vec<_> = sequence
        .timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Video && t.visible && !t.clips.is_empty())
        .collect();

    if visible_video_tracks.is_empty() {
        return Err(RenderError::EmptyTimeline);
    }

    let mut all_track_segments: Vec<Vec<avbridge::ClipSegment>> = Vec::new();
    let mut canvas: Option<Canvas> = None;

    for track in &visible_video_tracks {
        let pieces = track.lane_pieces();
        let lane_count = pieces
            .iter()
            .map(|(_, lane)| *lane)
            .max()
            .map_or(1, |lane| lane + 1);
        for lane in 0..lane_count {
            // Track 0 (background) vs 1+ (overlay) — matches avbridge_encode_timeline_export_multi's
            // own convention and the same distinction position_keyframes/opacity_keyframes already
            // rely on: `all_track_segments.len()` is this lane's index since it fills in order. A
            // lane above 0 holds the clips that overlap an earlier one (cross-dissolves).
            let is_overlay = !all_track_segments.is_empty();
            // One visible track on one lane is rendered by the single-track path, which builds a
            // graph per segment; anything more composites layers in one overlay graph.
            let single_graph = visible_video_tracks.len() == 1 && lane_count == 1;
            let mut clips: Vec<_> = pieces
                .iter()
                .filter(|(_, piece_lane)| *piece_lane == lane)
                .map(|(clip, _)| clip)
                .collect();
            clips.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));

            let mut segments = Vec::with_capacity(clips.len());
            let mut total_duration_secs = 0.0;
            let mut weighted_bitrate_bps_secs = 0.0;
            let mut dimensions_fps = None;

            for clip in &clips {
                let asset = media_library
                    .iter()
                    .find(|a| a.id == clip.resolved_asset_id())
                    .ok_or(RenderError::MissingAsset)?;
                if dimensions_fps.is_none() {
                    let (width, height) = asset.resolution.ok_or(RenderError::NoVideoStream)?;
                    dimensions_fps = Some((width, height, asset.fps.unwrap_or(30.0)));
                }
                let duration_secs = clip.duration_secs();
                total_duration_secs += duration_secs;
                weighted_bitrate_bps_secs +=
                    asset.source_bitrate_mbps as f64 * 1_000_000.0 * duration_secs;
                let (_, _, fps) = dimensions_fps.expect("just set above if it was None");
                let (fps_num, fps_den) = fps_to_rational(fps);
                let (video_filter, position_x_expr, position_y_expr) = resolve_clip_filters(
                    clip,
                    fps_num,
                    fps_den,
                    duration_secs,
                    is_overlay,
                    single_graph,
                );
                let video_filter = with_tone_mapping(asset, video_filter);
                // Same overlay-only gate resolve_clip_filters' resize stage above uses (see its
                // doc comment) — a track-0/background clip's alpha has no compositing stage to
                // survive into (build_vfilter_descr's immediate format=yuv420p conform discards
                // it), so the matte is only worth decoding/alphamerging on an overlay track.
                let mask_video_path = if is_overlay && clip.background_removal_enabled {
                    clip.background_removal_mask_path.clone()
                } else {
                    String::new()
                };
                // Blend mode is only meaningful on an overlay track (track 1+) — a background
                // track's clip has no layer below it to blend against, same is_overlay gate
                // resolve_clip_filters' layer-scale resize stage uses above.
                let blend_mode = if is_overlay && clip.has_blend_mode() {
                    clip.blend_mode.ffmpeg_name().to_string()
                } else {
                    String::new()
                };
                segments.push(avbridge::ClipSegment {
                    source_path: asset.source_path.clone(),
                    source_in_secs: clip.source_in_secs,
                    source_out_secs: clip.source_out_secs,
                    gain_db: clip.gain_db,
                    video_filter,
                    frozen: clip.frozen,
                    speed_factor: clip.speed_factor,
                    smooth_speed_ramp_end_factor: clip.speed_ramp_end_factor.unwrap_or(0.0),
                    position_x_expr,
                    position_y_expr,
                    transition_in: clip.transition_in.to_export_code(),
                    transition_duration_secs: clip.transition_duration_secs,
                    timeline_start_secs: clip.start_secs,
                    mask_video_path,
                    blend_mode,
                    anchor_x: clip.anchor_x,
                    anchor_y: clip.anchor_y,
                    fade_in_secs: clip.fade_in_secs,
                    fade_out_secs: clip.fade_out_secs,
                });
            }

            if canvas.is_none() {
                if let Some((width, height, fps)) = dimensions_fps {
                    let (fps_num, fps_den) = fps_to_rational(fps);
                    let bit_rate_bps = if total_duration_secs > 0.0 {
                        (weighted_bitrate_bps_secs / total_duration_secs) as i64
                    } else {
                        0
                    };
                    canvas = Some(Canvas {
                        width,
                        height,
                        fps_num,
                        fps_den,
                        bit_rate_bps,
                    });
                }
            }

            all_track_segments.push(segments);
        }
    }

    let canvas = canvas.ok_or(RenderError::EmptyTimeline)?;
    Ok((all_track_segments, canvas))
}

/// Approximates `fps` as a small integer ratio for `avbridge::Canvas` — exact for whole frame
/// rates (`30.0` -> `30/1`), otherwise a fixed-point fraction (`29.97` -> `29970/1000`) rather
/// than a true lowest-terms rational (e.g. NTSC's canonical `30000/1001`), which is precise
/// enough for a canvas frame rate and doesn't need a full rational-approximation algorithm.
pub(crate) fn fps_to_rational(fps: f32) -> (u32, u32) {
    if !fps.is_finite() || fps <= 0.0 {
        return (30, 1);
    }
    let rounded = fps.round();
    if (fps - rounded).abs() < 0.001 {
        (rounded as u32, 1)
    } else {
        ((fps * 1000.0).round() as u32, 1000)
    }
}

/// The mix segment for one audio-carrying clip of `track`.
pub(crate) fn audio_segment(
    clip: &crate::timeline::ClipInstance,
    asset: &MediaAsset,
    track: &crate::timeline::Track,
) -> avbridge::AudioSegment {
    avbridge::AudioSegment {
        source_path: asset.source_path.clone(),
        source_in_secs: clip.source_in_secs,
        source_out_secs: clip.source_out_secs,
        timeline_start_secs: clip.start_secs,
        // The track's fader adds to the clip's own gain.
        gain_db: clip.gain_db + track.mix.volume_db,
        // AudioSegment has no ramp field of its own — atempo takes one fixed parameter,
        // not a `t`-keyed expression, so a smooth per-sample tempo ramp isn't achievable
        // on the audio side in this FFmpeg build (same limitation
        // ClipSegment::smooth_speed_ramp_end_factor's own doc comment describes). The
        // ramp's *average* speed stands in when one is active, matching the approximation
        // ClipSegment's own inline audio path already uses for a ramped background/
        // single-track clip.
        speed_factor: clip
            .speed_ramp_end_factor
            .map(|end| (clip.speed_factor + end) / 2.0)
            .unwrap_or(clip.speed_factor),
        // A keyframed gain replaces `gain_db` with a linear expression, so the fader multiplies it.
        gain_keyframe_expr: keyframe::gain_filter_db_expr(
            &clip.gain_keyframes,
            clip.duration_secs(),
        )
        .map(|expr| {
            if track.mix.volume_db == 0.0 {
                expr
            } else {
                format!("({expr})*{:.6}", track.mix.linear_gain())
            }
        })
        .unwrap_or_default(),
        duck_role: track.audio_role.to_duck_role_code(),
        voice_cleanup_enabled: clip.voice_cleanup_enabled,
        voice_cleanup_noise_floor_db: clip.voice_cleanup_noise_floor_db,
        voice_cleanup_compressor_threshold_db: clip.voice_cleanup_compressor_threshold_db,
        voice_cleanup_compressor_ratio: clip.voice_cleanup_compressor_ratio,
        voice_cleanup_ceiling_linear: clip.voice_cleanup_ceiling_linear,
        fade_in_secs: track.audio_fade_in_secs(clip),
        fade_out_secs: track.audio_fade_out_secs(clip),
        pan: track.mix.pan,
        strip: track.mix.strip(),
    }
}

/// A segment standing in for a fully silenced sequence: the first clip that carries audio.
pub(crate) fn silent_stand_in(
    sequence: &Sequence,
    media_library: &[MediaAsset],
) -> Option<avbridge::AudioSegment> {
    for track in &sequence.timeline.tracks {
        if !matches!(track.kind, TrackKind::Video | TrackKind::Audio) {
            continue;
        }
        for clip in &track.clips {
            let asset = media_library
                .iter()
                .find(|asset| asset.id == clip.resolved_asset_id())?;
            if asset.has_audio {
                return Some(audio_segment(clip, asset, track));
            }
        }
    }
    None
}

/// Resolves every audible clip from visible video and audio tracks. Returns an empty list when
/// the first video track is the only audio contributor, allowing the established one-pass
/// export path to avoid an unnecessary second AAC encode. Once any additional track contributes
/// audio, the returned list includes the background clips too because the post-pass replaces,
/// rather than layers on top of, the original output audio.
pub fn resolve_audio_segments(
    sequence: &Sequence,
    media_library: &[MediaAsset],
) -> Result<Vec<avbridge::AudioSegment>, RenderError> {
    resolve_audio_segments_inner(sequence, media_library, false)
}

/// Every audible clip of `sequence` as mix segments, for an audio-only export: unlike
/// [`resolve_audio_segments`], there is no video file whose own audio could be copied instead, so
/// the whole mix is always built. Empty only when nothing in the sequence carries sound.
pub fn resolve_audio_segments_for_audio_export(
    sequence: &Sequence,
    media_library: &[MediaAsset],
) -> Result<Vec<avbridge::AudioSegment>, RenderError> {
    resolve_audio_segments_inner(sequence, media_library, true)
}

fn resolve_audio_segments_inner(
    sequence: &Sequence,
    media_library: &[MediaAsset],
    always_mix: bool,
) -> Result<Vec<avbridge::AudioSegment>, RenderError> {
    let first_video_track = sequence.timeline.tracks.iter().position(|track| {
        track.kind == TrackKind::Video && track.visible && !track.clips.is_empty()
    });

    let mut resolved = Vec::new();
    let mut has_additional_contributor = false;
    // Set when a muted (or not-soloed) track holds clips: the one-pass export would still carry
    // the first video track's own audio, so the post-pass mix must run to leave it out.
    let mut silenced_clips = false;
    for (track_index, track) in sequence.timeline.tracks.iter().enumerate() {
        if !track.visible || !matches!(track.kind, TrackKind::Video | TrackKind::Audio) {
            continue;
        }
        if !sequence.timeline.track_is_audible(track) {
            silenced_clips |= !track.clips.is_empty();
            continue;
        }
        // A fader or pan needs the mixed audio path: the one-pass export copies the source's audio.
        if !track.mix.is_neutral() {
            has_additional_contributor = true;
        }
        // A hard cut between two clips needs the mixed path, where the short declick fade lives;
        // the one-pass export would join the two sounds with a click.
        if track
            .clips
            .iter()
            .any(|clip| track.audio_has_hard_cut(clip))
        {
            has_additional_contributor = true;
        }
        // Clips that overlap (a cross-dissolve) need the mixed audio path, which plays both.
        if track.overlap_lanes().values().any(|&lane| lane > 0) {
            has_additional_contributor = true;
        }
        for clip in &track.clips {
            let asset = media_library
                .iter()
                .find(|asset| asset.id == clip.resolved_asset_id())
                .ok_or(RenderError::MissingAsset)?;
            if !asset.has_audio {
                continue;
            }
            if Some(track_index) != first_video_track {
                has_additional_contributor = true;
            }
            resolved.push(audio_segment(clip, asset, track));
        }
    }
    if silenced_clips && resolved.is_empty() {
        // Everything is silenced. An empty list would mean "keep the original audio", so
        // return one near-silent segment instead (-120 dB is inaudible and keeps the mix path).
        if let Some(mut quiet) = silent_stand_in(sequence, media_library) {
            quiet.gain_db = -120.0;
            return Ok(vec![quiet]);
        }
        return Ok(Vec::new());
    }
    if !always_mix && !has_additional_contributor && !silenced_clips {
        return Ok(Vec::new());
    }
    resolved.sort_by(|a, b| a.timeline_start_secs.total_cmp(&b.timeline_start_secs));
    Ok(resolved)
}

pub(crate) fn timeline_duration(track_segments: &[Vec<avbridge::ClipSegment>]) -> f64 {
    track_segments
        .first()
        .map(|track| track.iter().map(clip_segment_duration_secs).sum())
        .unwrap_or(0.0)
}
