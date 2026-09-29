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

//! Minimal GStreamer-based preview pipeline — plays a media file via `playbin`, decoding
//! independent of the UI thread (GStreamer drives its own internal threads once the pipeline
//! is `Playing`/`Paused`, no subprocess). Video frames are pulled on demand as raw RGBA
//! ([`Self::current_frame`]) rather than pushed to a live display — [`ui`] is
//! responsible for polling and uploading them to an egui texture; this module doesn't know
//! egui exists.

use std::path::Path;
use std::sync::OnceLock;

use gstreamer as gst;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;

use gst::prelude::*;

use crate::blend_mode::blend_channel;
use crate::timeline::{BlendMode, ClipInstance, ColorFilter, ShapeClip, TextClip, TransitionType};

/// Hardware video decoder factories whose rank has been raised above the usual software
/// decoders. GStreamer's decoder ranks are process-global, so this happens at most once; a
/// pipeline whose caller disabled acceleration still opts out independently through
/// `force-sw-decoders` instead of changing the ranks back underneath another live preview.
static PRIORITIZED_HARDWARE_DECODERS: OnceLock<Vec<String>> = OnceLock::new();

fn prioritize_hardware_video_decoders() -> &'static [String] {
    PRIORITIZED_HARDWARE_DECODERS.get_or_init(|| {
        let factories = gst::ElementFactory::factories_with_type(
            gst::ElementFactoryType::DECODER
                | gst::ElementFactoryType::MEDIA_VIDEO
                | gst::ElementFactoryType::HARDWARE,
            gst::Rank::NONE,
        );
        let preferred_rank = gst::Rank::PRIMARY + 1;
        let mut names = Vec::new();
        for factory in factories {
            // `HARDWARE` is derived from the factory's Klass metadata. Keep the explicit check
            // as a guard against plugins with overly broad or malformed metadata.
            if !factory.klass().split('/').any(|part| part == "Hardware") {
                continue;
            }
            if factory.rank() < preferred_rank {
                factory.set_rank(preferred_rank);
            }
            names.push(factory.name().to_string());
        }
        if names.is_empty() {
            tracing::debug!("no GStreamer hardware video decoder is available; using CPU");
        } else {
            tracing::info!(decoders = ?names, "prioritized GStreamer hardware video decoders");
        }
        names
    })
}

fn configure_playbin_decoder_preference(playbin: &gst::Element, hardware_decode: bool) {
    if hardware_decode {
        prioritize_hardware_video_decoders();
        return;
    }

    // `playbin` exposes the software-only switch as a member of its `flags` property rather
    // than as a standalone bool (unlike `uridecodebin`). Preserve every existing default flag
    // and add only `force-sw-decoders`.
    let flags = playbin.property_value("flags");
    let flags_class = gst::glib::FlagsClass::with_type(flags.type_())
        .expect("playbin's flags property always has a registered flags type");
    let flags = flags_class
        .builder_with_value(flags)
        .expect("playbin flags value matches its registered flags type")
        .set_by_nick("force-sw-decoders")
        .build()
        .expect("playbin exposes the force-sw-decoders flag");
    playbin.set_property_from_value("flags", &flags);
}

fn build_uri_decodebin(uri: &str, hardware_decode: bool) -> Result<gst::Element, PreviewError> {
    if hardware_decode {
        prioritize_hardware_video_decoders();
    }
    gst::ElementFactory::make("uridecodebin")
        .property("uri", uri)
        .property("force-sw-decoders", !hardware_decode)
        .build()
        .map_err(PreviewError::CreateElement)
}

#[derive(Debug)]
pub enum PreviewError {
    Init(gst::glib::Error),
    CreateElement(gst::glib::BoolError),
    UriConversion(gst::glib::Error),
    StateChange(gst::StateChangeError),
    Seek(gst::glib::BoolError),
    /// Couldn't probe `path` for its actual decoded resolution — needed to convert
    /// [`ClipInstance`]'s normalized crop rect into `videocrop`'s pixel properties.
    Probe(avbridge::ProbeError),
    /// Failed wiring the per-clip effects filter bin together (linking elements or adding
    /// ghost pads).
    FilterBin(gst::glib::BoolError),
    /// Failed wiring the multi-track compositor pipeline together (adding elements, linking a
    /// branch's chain, or linking a branch into `compositor`).
    Compositing(gst::glib::BoolError),
    /// `compositor` refused to hand out a new `sink_%u` request pad.
    RequestPad,
    /// Failed linking a branch's last element into its `compositor` request pad.
    PadLink(gst::PadLinkError),
    /// [`Preview::open_composited`]'s background input has no video stream to size the canvas
    /// from.
    NoBackgroundVideo,
    /// Failed pushing a rasterized text/shape overlay buffer into its `appsrc`, or signalling
    /// end-of-stream on it (see [`build_static_overlay_branch`]).
    PushBuffer(gst::FlowError),
}

impl std::fmt::Display for PreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PreviewError::Init(e) => write!(f, "failed to initialize GStreamer: {e}"),
            PreviewError::CreateElement(e) => write!(f, "failed to create playbin element: {e}"),
            PreviewError::UriConversion(e) => {
                write!(f, "failed to convert path to a file URI: {e}")
            }
            PreviewError::StateChange(e) => write!(f, "failed to change pipeline state: {e}"),
            PreviewError::Seek(e) => write!(f, "failed to seek: {e}"),
            PreviewError::Probe(e) => write!(f, "failed to probe for the effects filter bin: {e}"),
            PreviewError::FilterBin(e) => write!(f, "failed to build the effects filter bin: {e}"),
            PreviewError::Compositing(e) => {
                write!(f, "failed to build the compositor pipeline: {e}")
            }
            PreviewError::RequestPad => {
                write!(f, "compositor refused to hand out a sink request pad")
            }
            PreviewError::PadLink(e) => write!(f, "failed to link into compositor: {e:?}"),
            PreviewError::NoBackgroundVideo => {
                write!(
                    f,
                    "background input has no video stream to size the canvas from"
                )
            }
            PreviewError::PushBuffer(e) => {
                write!(f, "failed pushing a static overlay buffer: {e:?}")
            }
        }
    }
}

impl std::error::Error for PreviewError {}

/// One decoded video frame, pixels in tightly-packed row-major RGBA (no stride padding —
/// [`Self::rgba`] is exactly `width * height * 4` bytes, ready to hand to an egui
/// `ColorImage`/texture without extra copying logic).
#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl VideoFrame {
    /// Writes this frame to `path` as a PNG — what the preview transport row's snapshot button
    /// does with the most recently displayed frame (`ui`'s `PreviewState::last_frame` holds a
    /// clone of it, since the decoded pixels this struct wraps are otherwise dropped right after
    /// upload to the egui texture).
    pub fn save_png(&self, path: &std::path::Path) -> image::ImageResult<()> {
        image::save_buffer_with_format(
            path,
            &self.rgba,
            self.width,
            self.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
    }
}

/// Builds a `gst::Bin` chaining the subset of `clip`'s effects GStreamer can apply live,
/// suitable for `playbin`'s `video-filter` property — `None` if every covered effect is
/// neutral (leaves `video-filter` unset). Stage order matches
/// [`crate::timeline::ClipInstance::video_filter_chain`]'s, plus scale keyframes
/// (`ClipInstance::scale_keyframes`) applied first, ahead of everything else — mirroring
/// `ClipInstance::keyframe_video_filter_chain`, which applies it at the canvas level before the
/// per-clip filter chain for export — for consistency with what export applies, even though the
/// element set differs (GStreamer elements here, avfilter there) and the covered subset is
/// narrower (no vignette — no matching element in this GStreamer install; no chroma key — only
/// meaningful once layering exists; gain is handled by the separate audio pipeline; no
/// glitch/transitions — animated per-frame in export via avfilter's `n` frame-count expressions
/// with no static element equivalent GStreamer-side; position keyframes are applied by the
/// composited-overlay branch's compositor-pad probe
/// — they need a compositing (`overlay`) stage, which this single-clip pipeline doesn't have,
/// the same reason position has no visible effect on a single/background track in export either
/// — see `keyframe` module docs; scale/rotation/opacity each get a pad probe re-evaluating
/// `evaluate_keyframes` per buffer (`rotate`'s `angle` property and `alpha`'s `alpha` property
/// are both settable per-buffer the same way `videocrop`'s edges are), keyed off the buffer's
/// own PTS rather than a frame count, since preview has no fixed canvas fps to convert a frame
/// count against the way export's `N` does).
///
/// `resolution`, if known (`None` for an audio-only source, which shouldn't reach here but is
/// handled by just skipping the zoom/crop/pixelize/shake stages), is the *actual* decoded frame
/// size — needed since `videocrop`'s properties and the zoom/pixelize/shake downscale/upscale
/// target sizes are plain pixel counts, and `path` may be a lower-resolution editing proxy
/// rather than the original asset.
// Transitions (ClipInstance::transition_in/transition_duration_secs — a *different* concept
// from the scale_keyframes Ken-Burns-style animation above: an entry animation over just the
// clip's first transition_duration_secs) are now covered — Fade shares the opacity block's
// `alpha` element, Zoom reuses the scale_keyframes crop+upscale shape driven by transition
// progress instead of keyframes, and Slide uses two chained `videobox` elements (crop, then an
// equal-and-opposite black border) so the output frame size stays constant while the crop/pad
// split moves — see each block below for specifics. HardCut and None both still render nothing
// here, same as export.
//
// Position keyframes require compositing, so the single-clip path has no placement work to do;
// composited overlay branches update their compositor pad's `xpos` and `ypos` per buffer below.
/// `include_opacity` is `false` for an ordinary overlay branch in
/// [`Preview::open_composited`]'s pipeline — there, opacity keyframes drive `compositor`'s own
/// per-pad `alpha` property instead (real alpha blending against whatever's under it), so baking
/// a second, redundant uniform-alpha stage in here via the `alpha` element would just
/// double-apply the same ramp. Blend-mode overlays bypass `compositor`, so they keep this stage
/// and [`Preview::current_frame`] applies its resulting alpha during CPU compositing.
/// Every other caller (the single-clip [`Preview::open`] path, and a composited pipeline's
/// background branch, which has nothing under it to blend against) keeps the old behavior.
///
/// The name [`build_video_filter_bin`] gives its `videobalance` element for `clip_id`, if it
/// builds one at all — shared with [`Preview::set_live_balance`] so a later live update can
/// find the exact same element again by name (`gst::Bin::by_name` searches recursively, so
/// this works whether `clip_id` is the single-clip [`Preview::open`] pipeline's only clip or
/// one branch of a [`Preview::open_composited`] pipeline).
fn live_balance_element_name(clip_id: u64) -> String {
    format!("oca_balance_{clip_id}")
}

/// The name [`build_video_filter_bin`] gives its `gaussianblur` element for `clip_id`, if it
/// builds one at all — same "shared with the live-update method so it can find the exact same
/// element again by name" contract [`live_balance_element_name`] has.
fn live_blur_element_name(clip_id: u64) -> String {
    format!("oca_blur_{clip_id}")
}

/// The name [`build_video_filter_bin`] gives its static-crop `videocrop` element for `clip_id`,
/// if it builds one at all (only when [`ClipInstance::is_cropped`] is true) — same "shared with
/// the live-update method so it can find the exact same element again by name" contract
/// [`live_balance_element_name`]/[`live_blur_element_name`] have. Distinct from the *other*
/// `videocrop` elements this function builds for scale-keyframe zoom and shake — those are never
/// named, since P1 item 3's remaining live-preview-update gap only covers this static crop stage
/// (zoom is keyframe-driven, already live; shake gets its own [`ShakeMarginHandle`] mechanism).
fn live_crop_element_name(clip_id: u64) -> String {
    format!("oca_crop_{clip_id}")
}

/// The name [`build_video_filter_bin`] gives the downscale-stage `capsfilter` of its pixelize
/// block for `clip_id`, if it builds one at all (only when `pixelize_intensity > 0.0`) — same
/// "shared with the live-update method" contract as [`live_crop_element_name`]. Only the
/// downscale caps need renaming/renegotiating on a live update; the upscale caps stay pinned to
/// the branch's own fixed output resolution regardless of block size.
fn live_pixelize_caps_name(clip_id: u64) -> String {
    format!("oca_pixelize_caps_{clip_id}")
}

/// Shared, atomically-updatable pixel margin a shake stage's buffer probe reads every frame,
/// instead of the closed-over plain `i32`s a non-live shake block would use — lets
/// [`Preview::set_live_shake`] change `total_trim_w`/`total_trim_h` without touching the
/// running pipeline's element graph at all, just the numbers the already-installed probe reads.
/// `resolution` is cached alongside so the live setter can redo the intensity-to-pixel-margin
/// math itself without needing the caller (`ui`'s `App`) to know the decoded frame size.
struct ShakeMarginHandle {
    total_trim_w: std::sync::Arc<std::sync::atomic::AtomicI32>,
    total_trim_h: std::sync::Arc<std::sync::atomic::AtomicI32>,
    resolution: (u32, u32),
}

/// `shake_intensity`-derived margin (`0.08` px-fraction-per-unit-intensity, a judgment call,
/// same style as this module's other intensity-to-pixel scale factors) to `videocrop`'s own
/// "total pixels trimmed per axis" convention — shared by the initial build in
/// [`build_video_filter_bin`] and a later live update in [`Preview::set_live_shake`] so both
/// compute the exact same numbers from the exact same formula.
fn shake_margin_pixels(width: u32, height: u32, margin: f32) -> (i32, i32) {
    let total_trim_w =
        ((width as f32 * 2.0 * margin).round() as i32).clamp(0, (width as i32 - 2).max(0));
    let total_trim_h =
        ((height as f32 * 2.0 * margin).round() as i32).clamp(0, (height as i32 - 2).max(0));
    (total_trim_w, total_trim_h)
}

fn build_video_filter_bin(
    clip: &ClipInstance,
    resolution: Option<(u32, u32)>,
    include_opacity: bool,
) -> Result<(Option<gst::Element>, Option<ShakeMarginHandle>), PreviewError> {
    let mut elements: Vec<gst::Element> = Vec::new();
    let mut shake_handle: Option<ShakeMarginHandle> = None;

    if let Some((width, height)) = resolution {
        if clip.has_scale_keyframes() {
            // Scale keyframes sit before video_filter_chain's own stages in export (see
            // ClipInstance::keyframe_video_filter_chain, applied at the canvas level ahead of
            // the per-clip filter chain) — mirrored here by building it first, ahead of the
            // user-crop stage below. Keyed off the buffer's own PTS (seconds within the *source
            // file*, since ui's preview seeks to clip.source_in_secs + offset rather than 0)
            // instead of a frame counter — preview has no fixed canvas fps to convert a frame
            // count against, unlike export. evaluate_keyframes is the same piecewise-linear
            // interpolation export's scale_filter_expr compiles into an avfilter expression —
            // this reuses it directly instead of duplicating the math, and naturally supports
            // any number of keyframes (not just the old zoom_start/zoom_end two-point case).
            let scale_keyframes = clip.scale_keyframes.clone();
            let source_in_secs = clip.source_in_secs;
            let clip_duration_secs = (clip.source_out_secs - clip.source_in_secs).max(1e-6);

            let zoom_crop = gst::ElementFactory::make("videocrop")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let zoom_crop_for_probe = zoom_crop.clone();
            let sink_pad = zoom_crop
                .static_pad("sink")
                .expect("videocrop always has a sink pad");
            sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
                let secs = info
                    .buffer()
                    .and_then(|b| b.pts())
                    .map(|t| t.seconds_f64())
                    .unwrap_or(source_in_secs);
                let frac = ((secs - source_in_secs) / clip_duration_secs).clamp(0.0, 1.0) as f32;
                let zoom = crate::keyframe::evaluate_keyframes(&scale_keyframes, frac, 1.0)
                    .clamp(0.1, 20.0) as f64;
                let crop_w = (width as f64 / zoom).round().max(2.0);
                let crop_h = (height as f64 / zoom).round().max(2.0);
                let side_w = ((width as f64 - crop_w) / 2.0).round().max(0.0) as i32;
                let side_h = ((height as f64 - crop_h) / 2.0).round().max(0.0) as i32;
                zoom_crop_for_probe.set_property("left", side_w);
                zoom_crop_for_probe.set_property("right", side_w);
                zoom_crop_for_probe.set_property("top", side_h);
                zoom_crop_for_probe.set_property("bottom", side_h);
                gst::PadProbeReturn::Ok
            });

            let upscale = gst::ElementFactory::make("videoscale")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let full_caps = gst::ElementFactory::make("capsfilter")
                .property(
                    "caps",
                    gst::Caps::builder("video/x-raw")
                        .field("width", width as i32)
                        .field("height", height as i32)
                        .build(),
                )
                .build()
                .map_err(PreviewError::CreateElement)?;
            elements.push(zoom_crop);
            elements.push(upscale);
            elements.push(full_caps);
        }
    }

    if clip.has_rotation_keyframes() {
        // Mirrors export's rotation_filter_angle_expr: keyed off elapsed seconds within the
        // source file (clip.source_in_secs + offset), same PTS-based fraction the scale probe
        // above uses, since preview has no fixed canvas fps to convert a frame count against.
        let rotation_keyframes = clip.rotation_keyframes.clone();
        let source_in_secs = clip.source_in_secs;
        let clip_duration_secs = (clip.source_out_secs - clip.source_in_secs).max(1e-6);

        let rotate = gst::ElementFactory::make("rotate")
            .build()
            .map_err(PreviewError::CreateElement)?;
        let rotate_for_probe = rotate.clone();
        let sink_pad = rotate
            .static_pad("sink")
            .expect("rotate always has a sink pad");
        sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
            let secs = info
                .buffer()
                .and_then(|b| b.pts())
                .map(|t| t.seconds_f64())
                .unwrap_or(source_in_secs);
            let frac = ((secs - source_in_secs) / clip_duration_secs).clamp(0.0, 1.0) as f32;
            let angle_deg = crate::keyframe::evaluate_keyframes(&rotation_keyframes, frac, 0.0);
            rotate_for_probe.set_property("angle", angle_deg.to_radians() as f64);
            gst::PadProbeReturn::Ok
        });
        elements.push(rotate);
    }

    // Fade transition (ClipInstance::transition_in == Fade) shares this same "alpha" element/
    // pad-probe with opacity keyframes rather than getting its own — "alpha"'s `method=set`
    // overwrites the buffer's alpha outright, so two separate `alpha` elements chained would
    // just have the second silently discard the first's ramp instead of combining with it.
    // Both factors default to a no-op (opacity 1.0, fade 1.0 once past transition_duration_secs)
    // so either one alone still works exactly as before.
    let has_fade = include_opacity && clip.transition_in == TransitionType::Fade;
    if include_opacity && (clip.has_opacity_keyframes() || has_fade) {
        // "alpha" (gst-plugins-good) sets a uniform per-buffer alpha on its output — the RGBA
        // path is already forced by the AppSink's fixed caps in Preview::open, and egui draws
        // ColorImage::from_rgba_unmultiplied with alpha blending, so a reduced alpha here is
        // actually visible in the preview (unlike position, which would need a compositing
        // stage this single-clip pipeline doesn't have).
        let opacity_keyframes = clip.opacity_keyframes.clone();
        let source_in_secs = clip.source_in_secs;
        let clip_duration_secs = (clip.source_out_secs - clip.source_in_secs).max(1e-6);
        let transition_duration_secs = clip.transition_duration_secs.max(1e-6) as f64;

        let alpha = gst::ElementFactory::make("alpha")
            .property_from_str("method", "set")
            .build()
            .map_err(PreviewError::CreateElement)?;
        let alpha_for_probe = alpha.clone();
        let sink_pad = alpha
            .static_pad("sink")
            .expect("alpha always has a sink pad");
        sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
            let secs = info
                .buffer()
                .and_then(|b| b.pts())
                .map(|t| t.seconds_f64())
                .unwrap_or(source_in_secs);
            let elapsed = secs - source_in_secs;
            let frac = (elapsed / clip_duration_secs).clamp(0.0, 1.0) as f32;
            let opacity =
                crate::keyframe::evaluate_keyframes(&opacity_keyframes, frac, 1.0).clamp(0.0, 1.0);
            let fade = if has_fade {
                (elapsed / transition_duration_secs).clamp(0.0, 1.0)
            } else {
                1.0
            };
            alpha_for_probe.set_property("alpha", (opacity as f64) * fade);
            gst::PadProbeReturn::Ok
        });
        elements.push(alpha);
    }

    // Zoom transition (ClipInstance::transition_in == Zoom) — an entry animation over just
    // transition_duration_secs, distinct from the Ken-Burns scale_keyframes block above (which
    // covers the clip's whole duration). Mirrors export's own z(N) formula in
    // `timeline_export_multi.c`'s `build_vfilter_descr` (`z = 0.5 + 0.5*min(N/tf, 1)`, sampling
    // source coordinates scaled by `1/z`) as a crop-then-upscale zoom factor of `1/z` instead —
    // `1/z` ranges `2.0` (transition start, cropped to a small central region and upscaled —
    // "zoomed in") down to `1.0` (transition end, full frame) — same zoom_crop/upscale/capsfilter
    // shape as the scale_keyframes block, just driven by transition progress instead of
    // keyframes, and safe to stack with it (each is its own crop+upscale stage).
    if include_opacity && clip.transition_in == TransitionType::Zoom {
        if let Some((width, height)) = resolution {
            let source_in_secs = clip.source_in_secs;
            let transition_duration_secs = clip.transition_duration_secs.max(1e-6) as f64;

            let zoom_crop = gst::ElementFactory::make("videocrop")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let zoom_crop_for_probe = zoom_crop.clone();
            let sink_pad = zoom_crop
                .static_pad("sink")
                .expect("videocrop always has a sink pad");
            sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
                let secs = info
                    .buffer()
                    .and_then(|b| b.pts())
                    .map(|t| t.seconds_f64())
                    .unwrap_or(source_in_secs);
                let progress = ((secs - source_in_secs) / transition_duration_secs).clamp(0.0, 1.0);
                let z = 0.5 + 0.5 * progress;
                let zoom = (1.0 / z).clamp(0.1, 20.0);
                let crop_w = (width as f64 / zoom).round().max(2.0);
                let crop_h = (height as f64 / zoom).round().max(2.0);
                let side_w = ((width as f64 - crop_w) / 2.0).round().max(0.0) as i32;
                let side_h = ((height as f64 - crop_h) / 2.0).round().max(0.0) as i32;
                zoom_crop_for_probe.set_property("left", side_w);
                zoom_crop_for_probe.set_property("right", side_w);
                zoom_crop_for_probe.set_property("top", side_h);
                zoom_crop_for_probe.set_property("bottom", side_h);
                gst::PadProbeReturn::Ok
            });

            let upscale = gst::ElementFactory::make("videoscale")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let full_caps = gst::ElementFactory::make("capsfilter")
                .property(
                    "caps",
                    gst::Caps::builder("video/x-raw")
                        .field("width", width as i32)
                        .field("height", height as i32)
                        .build(),
                )
                .build()
                .map_err(PreviewError::CreateElement)?;
            elements.push(zoom_crop);
            elements.push(upscale);
            elements.push(full_caps);
        }
    }

    // Slide transition (ClipInstance::transition_in == Slide) — a left-to-right wipe reveal:
    // mirrors export's geq wipe (visible = source pixels left of a moving edge, black
    // everywhere past it) via two chained `videobox` elements instead of one, so the output
    // frame size stays constant throughout (same reason the zoom stages above always follow a
    // crop with an upscale back to the fixed canvas size, rather than letting frame size drift
    // per buffer): the first crops `hidden_w` pixels off the right (shrinking the frame down to
    // just the already-revealed region), the second re-pads that same `hidden_w` back onto the
    // right as a black (`fill`'s default) border, netting zero size change overall.
    if include_opacity && clip.transition_in == TransitionType::Slide {
        if let Some((width, height)) = resolution {
            let source_in_secs = clip.source_in_secs;
            let transition_duration_secs = clip.transition_duration_secs.max(1e-6) as f64;

            let reveal_crop = gst::ElementFactory::make("videobox")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let reveal_pad = gst::ElementFactory::make("videobox")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let crop_for_probe = reveal_crop.clone();
            let pad_for_probe = reveal_pad.clone();
            let sink_pad = reveal_crop
                .static_pad("sink")
                .expect("videobox always has a sink pad");
            sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
                let secs = info
                    .buffer()
                    .and_then(|b| b.pts())
                    .map(|t| t.seconds_f64())
                    .unwrap_or(source_in_secs);
                let progress = ((secs - source_in_secs) / transition_duration_secs).clamp(0.0, 1.0);
                let hidden_w = (width as f64 * (1.0 - progress)).round().max(0.0) as i32;
                crop_for_probe.set_property("right", hidden_w);
                pad_for_probe.set_property("right", -hidden_w);
                gst::PadProbeReturn::Ok
            });
            elements.push(reveal_crop);
            elements.push(reveal_pad);

            // The crop and its equal-and-opposite pad were observed to occasionally net one
            // pixel off the canvas width (an internal videobox rounding quirk against
            // chroma-subsampled formats, not something either property alone controls) — a
            // trailing rescale-to-exact-size stage (`videoscale` + a pinning `capsfilter`, same
            // pair the zoom stages above use after their own crop) irons that out regardless.
            let rescale = gst::ElementFactory::make("videoscale")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let full_caps = gst::ElementFactory::make("capsfilter")
                .property(
                    "caps",
                    gst::Caps::builder("video/x-raw")
                        .field("width", width as i32)
                        .field("height", height as i32)
                        .build(),
                )
                .build()
                .map_err(PreviewError::CreateElement)?;
            elements.push(rescale);
            elements.push(full_caps);
        }
    }

    if let Some((width, height)) = resolution {
        if clip.is_cropped() {
            let (width, height) = (width as f32, height as f32);
            let left = (clip.crop_x * width).round().max(0.0) as i32;
            let top = (clip.crop_y * height).round().max(0.0) as i32;
            let right = ((1.0 - clip.crop_x - clip.crop_w) * width).round().max(0.0) as i32;
            let bottom = ((1.0 - clip.crop_y - clip.crop_h) * height)
                .round()
                .max(0.0) as i32;
            let crop = gst::ElementFactory::make("videocrop")
                .name(live_crop_element_name(clip.id))
                .property("left", left)
                .property("top", top)
                .property("right", right)
                .property("bottom", bottom)
                .build()
                .map_err(PreviewError::CreateElement)?;
            elements.push(crop);
        }
    }

    let effective_saturation = if clip.color_filter == ColorFilter::BlackAndWhite {
        0.0
    } else {
        clip.saturation as f64
    };
    if clip.brightness != 0.0 || clip.contrast != 1.0 || effective_saturation != 1.0 {
        // Named per clip id (not left auto-generated) so a live properties-panel edit can find
        // this exact element again later via Preview::set_live_balance -- P1 item 3's remaining
        // live-preview-update gap (see that method's own doc comment for the full picture, and
        // why only brightness/contrast/saturation get this treatment).
        let balance = gst::ElementFactory::make("videobalance")
            .name(live_balance_element_name(clip.id))
            .property("brightness", clip.brightness as f64)
            .property("contrast", clip.contrast as f64)
            .property("saturation", effective_saturation)
            .build()
            .map_err(PreviewError::CreateElement)?;
        elements.push(balance);
    }

    if clip.color_filter == ColorFilter::Sepia {
        let sepia = gst::ElementFactory::make("coloreffects")
            .property_from_str("preset", "sepia")
            .build()
            .map_err(PreviewError::CreateElement)?;
        elements.push(sepia);
    }

    // gaussianblur's single signed `sigma` covers both blur_intensity (positive) and sharpen
    // (negative) — the *4.0 scale is a judgment call, same style as video_filter_chain's own
    // per-effect scale factors, not a value derived from anything.
    let net_sigma = clip.blur_intensity as f64 * 4.0 - clip.sharpen as f64 * 4.0;
    if net_sigma != 0.0 {
        let blur = gst::ElementFactory::make("gaussianblur")
            .name(live_blur_element_name(clip.id))
            .property("sigma", net_sigma)
            .build()
            .map_err(PreviewError::CreateElement)?;
        elements.push(blur);
    }

    if let Some((width, height)) = resolution {
        if clip.pixelize_intensity > 0.0 {
            // Same block-size formula as video_filter_chain's: 2px (subtle) to 50px (heavy
            // censorship). Two videoscale elements (nearest-neighbour, for the hard mosaic
            // look — bilinear would just blur) with a capsfilter pinning each stage's output
            // size stand in for avfilter's single scale-down/scale-up expression pair.
            let block = (2.0 + clip.pixelize_intensity * 48.0).round().max(1.0) as u32;
            let small_w = (width / block).max(1) as i32;
            let small_h = (height / block).max(1) as i32;

            let downscale = gst::ElementFactory::make("videoscale")
                .property_from_str("method", "nearest-neighbour")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let small_caps = gst::ElementFactory::make("capsfilter")
                .name(live_pixelize_caps_name(clip.id))
                .property(
                    "caps",
                    gst::Caps::builder("video/x-raw")
                        .field("width", small_w)
                        .field("height", small_h)
                        .build(),
                )
                .build()
                .map_err(PreviewError::CreateElement)?;
            let upscale = gst::ElementFactory::make("videoscale")
                .property_from_str("method", "nearest-neighbour")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let full_caps = gst::ElementFactory::make("capsfilter")
                .property(
                    "caps",
                    gst::Caps::builder("video/x-raw")
                        .field("width", width as i32)
                        .field("height", height as i32)
                        .build(),
                )
                .build()
                .map_err(PreviewError::CreateElement)?;
            elements.push(downscale);
            elements.push(small_caps);
            elements.push(upscale);
            elements.push(full_caps);
        }
    }

    if let Some((width, height)) = resolution {
        if clip.shake_intensity > 0.0 {
            // avfilter's crop=...:iw*margin*(1+sin(n*0.31)):ih*margin*(1+cos(n*0.23)) slides a
            // fixed-size crop *window* around within a margin, keyed off n (frame count).
            // videocrop has no expression support, so a buffer probe on its sink pad recomputes
            // left/top/right/bottom from a running frame counter before every frame — same
            // sinusoids, translated from ffmpeg's "window position" framing to videocrop's
            // "pixels trimmed per edge" one: left + right (and top + bottom) are kept summing
            // to a constant total_trim (only the split between them oscillates), so videocrop's
            // *output* size never changes frame to frame and the fixed-size upscale after it
            // never needs to renegotiate caps mid-stream.
            let margin = clip.shake_intensity * 0.08_f32;
            let (total_trim_w, total_trim_h) = shake_margin_pixels(width, height, margin);

            // Shared, live-updatable margin (see [`ShakeMarginHandle`]) instead of the plain
            // `i32`s a non-live shake block would close over — Preview::set_live_shake writes
            // new values here directly, no pipeline restructuring needed for P1 item 3's
            // remaining live-preview-update gap.
            let total_trim_w_atomic =
                std::sync::Arc::new(std::sync::atomic::AtomicI32::new(total_trim_w));
            let total_trim_h_atomic =
                std::sync::Arc::new(std::sync::atomic::AtomicI32::new(total_trim_h));
            shake_handle = Some(ShakeMarginHandle {
                total_trim_w: total_trim_w_atomic.clone(),
                total_trim_h: total_trim_h_atomic.clone(),
                resolution: (width, height),
            });

            let crop = gst::ElementFactory::make("videocrop")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let crop_for_probe = crop.clone();
            let frame_counter = std::sync::atomic::AtomicU64::new(0);
            let sink_pad = crop
                .static_pad("sink")
                .expect("videocrop always has a sink pad");
            sink_pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
                let n = frame_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as f64;
                let total_trim_w = total_trim_w_atomic.load(std::sync::atomic::Ordering::Relaxed);
                let total_trim_h = total_trim_h_atomic.load(std::sync::atomic::Ordering::Relaxed);
                let frac_x = (1.0 + (n * 0.31).sin()) / 2.0;
                let frac_y = (1.0 + (n * 0.23).cos()) / 2.0;
                let left = (total_trim_w as f64 * frac_x).round() as i32;
                let top = (total_trim_h as f64 * frac_y).round() as i32;
                crop_for_probe.set_property("left", left);
                crop_for_probe.set_property("right", total_trim_w - left);
                crop_for_probe.set_property("top", top);
                crop_for_probe.set_property("bottom", total_trim_h - top);
                gst::PadProbeReturn::Ok
            });

            let upscale = gst::ElementFactory::make("videoscale")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let full_caps = gst::ElementFactory::make("capsfilter")
                .property(
                    "caps",
                    gst::Caps::builder("video/x-raw")
                        .field("width", width as i32)
                        .field("height", height as i32)
                        .build(),
                )
                .build()
                .map_err(PreviewError::CreateElement)?;
            elements.push(crop);
            elements.push(upscale);
            elements.push(full_caps);
        }
    }

    if clip.flipped_h {
        let flip = gst::ElementFactory::make("videoflip")
            .property_from_str("method", "horizontal-flip")
            .build()
            .map_err(PreviewError::CreateElement)?;
        elements.push(flip);
    }

    if elements.is_empty() {
        return Ok((None, shake_handle));
    }

    let bin = gst::Bin::new();
    bin.add_many(&elements).map_err(PreviewError::FilterBin)?;
    gst::Element::link_many(&elements).map_err(PreviewError::FilterBin)?;

    let sink_pad = elements
        .first()
        .and_then(|e| e.static_pad("sink"))
        .expect("every filter element above has a static sink pad");
    let src_pad = elements
        .last()
        .and_then(|e| e.static_pad("src"))
        .expect("every filter element above has a static src pad");
    let ghost_sink = gst::GhostPad::with_target(&sink_pad).map_err(PreviewError::FilterBin)?;
    let ghost_src = gst::GhostPad::with_target(&src_pad).map_err(PreviewError::FilterBin)?;
    bin.add_pad(&ghost_sink).map_err(PreviewError::FilterBin)?;
    bin.add_pad(&ghost_src).map_err(PreviewError::FilterBin)?;

    Ok((Some(bin.upcast::<gst::Element>()), shake_handle))
}

/// Builds the `alpha` element's chroma-key configuration for `clip` — `method=custom` against
/// its own `chroma_key_color` rather than the fixed `green`/`blue` presets, so an arbitrary key
/// color (not just a standard green/blue screen) works the same as export's avfilter `colorkey`
/// stage does. `chroma_key_tolerance` (`0.0..=1.0`) maps onto `alpha`'s `black-sensitivity`/
/// `white-sensitivity` (`0..=128`, default `100`) linearly — a judgment call, same as this
/// module's other intensity-to-property scale factors, not a value derived from anything.
fn build_chroma_key_element(clip: &ClipInstance) -> Result<gst::Element, PreviewError> {
    let [r, g, b] = clip.chroma_key_color;
    let sensitivity = (clip.chroma_key_tolerance.clamp(0.0, 1.0) * 128.0).round() as u32;
    gst::ElementFactory::make("alpha")
        .name(live_chroma_key_element_name(clip.id))
        .property_from_str("method", "custom")
        .property("target-r", r as u32)
        .property("target-g", g as u32)
        .property("target-b", b as u32)
        .property("black-sensitivity", sensitivity)
        .property("white-sensitivity", sensitivity)
        .build()
        .map_err(PreviewError::CreateElement)
}

/// The name [`build_chroma_key_element`] gives its `alpha` element for `clip_id`, if it builds
/// one at all — same "shared with the live-update method so it can find the exact same element
/// again by name" contract [`live_balance_element_name`]/[`live_blur_element_name`] have.
fn live_chroma_key_element_name(clip_id: u64) -> String {
    format!("oca_chromakey_{clip_id}")
}

/// `ClipInstance::gain_db` (a dB offset, matching export's own `volume=%.4fdB` avfilter option)
/// converted to the linear scale factor GStreamer's `volume` element property actually expects
/// (`1.0` = unity gain, unlike avfilter which takes the dB string directly) — standard dB-to-
/// amplitude conversion, `10^(dB/20)`.
fn gain_db_to_linear(gain_db: f32) -> f64 {
    10f64.powf(gain_db as f64 / 20.0)
}

/// Builds a `gst::Bin` wrapping a `volume` element set to `clip.gain_db`'s linear equivalent —
/// `None` if `gain_db` is `0.0` (unity gain, a no-op), same "`None` when neutral" convention
/// [`build_video_filter_bin`] uses. Suitable for `playbin`'s `audio-filter` property
/// ([`Preview::open`]) — [`Preview::open_composited`]'s own manually-built audio chain applies
/// the `volume` element directly instead, since it isn't `playbin`-managed.
fn build_audio_filter_bin(clip: &ClipInstance) -> Result<Option<gst::Element>, PreviewError> {
    if clip.gain_db == 0.0 {
        return Ok(None);
    }
    let volume = gst::ElementFactory::make("volume")
        .property("volume", gain_db_to_linear(clip.gain_db).clamp(0.0, 10.0))
        .build()
        .map_err(PreviewError::CreateElement)?;

    let bin = gst::Bin::new();
    bin.add(&volume).map_err(PreviewError::FilterBin)?;
    let sink_pad = volume
        .static_pad("sink")
        .expect("volume always has a sink pad");
    let src_pad = volume
        .static_pad("src")
        .expect("volume always has a src pad");
    let ghost_sink = gst::GhostPad::with_target(&sink_pad).map_err(PreviewError::FilterBin)?;
    let ghost_src = gst::GhostPad::with_target(&src_pad).map_err(PreviewError::FilterBin)?;
    bin.add_pad(&ghost_sink).map_err(PreviewError::FilterBin)?;
    bin.add_pad(&ghost_src).map_err(PreviewError::FilterBin)?;

    Ok(Some(bin.upcast::<gst::Element>()))
}

/// Links `decodebin`'s first audio output pad to `target_sink` once it appears — the audio twin
/// of [`connect_decodebin_video_pad`], shared by every branch feeding the composited preview's
/// `audiomixer`.
fn connect_decodebin_audio_pad(decodebin: &gst::Element, target_sink: gst::Pad) {
    decodebin.connect_pad_added(move |_dbin, src_pad| {
        if target_sink.is_linked() {
            return;
        }
        let is_audio = src_pad
            .current_caps()
            .or_else(|| Some(src_pad.query_caps(None)))
            .and_then(|caps| caps.structure(0).map(|s| s.name().starts_with("audio/")))
            .unwrap_or(false);
        if !is_audio {
            return;
        }
        if let Err(e) = src_pad.link(&target_sink) {
            tracing::warn!(error = ?e, "failed to link decodebin audio pad into the audio chain");
        }
    });
}

/// Connects one decoded source to an `audiomixer`, applying the clip's static gain before the
/// mix. A queue isolates each source's decode scheduling so one slow branch cannot block every
/// other input upstream of the mixer.
fn attach_audio_mix_branch(
    pipeline: &gst::Pipeline,
    mixer: &gst::Element,
    decodebin: &gst::Element,
    gain_db: f32,
) -> Result<(), PreviewError> {
    let queue = gst::ElementFactory::make("queue")
        .build()
        .map_err(PreviewError::CreateElement)?;
    let convert = gst::ElementFactory::make("audioconvert")
        .build()
        .map_err(PreviewError::CreateElement)?;
    let resample = gst::ElementFactory::make("audioresample")
        .build()
        .map_err(PreviewError::CreateElement)?;
    let volume = gst::ElementFactory::make("volume")
        .property("volume", gain_db_to_linear(gain_db).clamp(0.0, 10.0))
        .build()
        .map_err(PreviewError::CreateElement)?;
    pipeline
        .add_many([&queue, &convert, &resample, &volume])
        .map_err(PreviewError::Compositing)?;
    gst::Element::link_many([&queue, &convert, &resample, &volume])
        .map_err(PreviewError::Compositing)?;
    let mixer_pad = mixer
        .request_pad_simple("sink_%u")
        .ok_or(PreviewError::RequestPad)?;
    volume
        .static_pad("src")
        .expect("volume always has a src pad")
        .link(&mixer_pad)
        .map_err(PreviewError::PadLink)?;
    connect_decodebin_audio_pad(
        decodebin,
        queue
            .static_pad("sink")
            .expect("queue always has a sink pad"),
    );
    Ok(())
}

/// Creates the shared mixed-audio output chain and returns its `audiomixer` input element,
/// along with the live [`AudioLevel`] snapshot [`build_metering_audio_sink`] wires into the
/// chain's own tail.
fn build_audio_mix_output(
    pipeline: &gst::Pipeline,
) -> Result<(gst::Element, std::sync::Arc<std::sync::Mutex<AudioLevel>>), PreviewError> {
    let mixer = gst::ElementFactory::make("audiomixer")
        .build()
        .map_err(PreviewError::CreateElement)?;
    let convert = gst::ElementFactory::make("audioconvert")
        .build()
        .map_err(PreviewError::CreateElement)?;
    let resample = gst::ElementFactory::make("audioresample")
        .build()
        .map_err(PreviewError::CreateElement)?;
    let sink = gst::ElementFactory::make("autoaudiosink")
        .build()
        .map_err(PreviewError::CreateElement)?;
    let (metering_sink, audio_level) = build_metering_audio_sink(sink)?;
    pipeline
        .add_many([&mixer, &convert, &resample, &metering_sink])
        .map_err(PreviewError::Compositing)?;
    gst::Element::link_many([&mixer, &convert, &resample, &metering_sink])
        .map_err(PreviewError::Compositing)?;
    Ok((mixer, audio_level))
}

/// Links `decodebin`'s first video output pad to `target_sink` once it appears — `decodebin`/
/// `uridecodebin` expose pads dynamically (`pad-added`, possibly more than one: video, audio,
/// subtitle), so this can't be a static link at bin-build time like every other element pair in
/// [`build_video_filter_bin`]. Ignores non-video pads (an audio pad with nothing downstream just
/// sits unused — GStreamer doesn't require every source pad to be linked) and a second video pad
/// if one somehow appears (multi-video-stream files aren't a case this preview handles).
fn connect_decodebin_video_pad(decodebin: &gst::Element, target_sink: gst::Pad) {
    decodebin.connect_pad_added(move |_dbin, src_pad| {
        if target_sink.is_linked() {
            return;
        }
        let is_video = src_pad
            .current_caps()
            .or_else(|| Some(src_pad.query_caps(None)))
            .and_then(|caps| caps.structure(0).map(|s| s.name().starts_with("video/")))
            .unwrap_or(false);
        if !is_video {
            return;
        }
        if let Err(e) = src_pad.link(&target_sink) {
            tracing::warn!(error = ?e, "failed to link decodebin video pad into its branch chain");
        }
    });
}

/// Wires a static rasterized `GRAY8` mask (`mask_gray`, from
/// [`crate::overlay_render::render_mask_shape_gray8`]) onto `chain_tail`'s own output via
/// `alphacombine`, returning the new final element ([`build_composite_branch`]'s next
/// `chain_tail`) plus the `appsrc` feeding it. Mirrors that function's own
/// background-removal-matte `alphacombine` stage, just with a static `appsrc`+
/// `imagefreeze(allow-replace=true)` pair standing in for a decoded matte file —
/// `alphacombine`'s `alpha` pad accepts `GRAY8` directly, so unlike the matte's own decode
/// branch, no `videoconvert`/`capsfilter` stage is needed on this side at all. `mask_shape` has
/// no keyframes, so one buffer suffices for playback same as
/// [`build_static_overlay_branch`]'s shape branches, but `allow_replace` stays enabled (unlike
/// those) so [`Preview::set_live_mask`] can push a freshly rasterized buffer in later —
/// P1 item 3's remaining live-preview-update gap.
fn build_mask_shape_stage(
    pipeline: &gst::Pipeline,
    chain_tail: &gst::Element,
    mask_gray: Vec<u8>,
    (width, height): (u32, u32),
) -> Result<(gst::Element, gst_app::AppSrc), PreviewError> {
    let mask_caps = gst::Caps::builder("video/x-raw")
        .field("format", "GRAY8")
        .field("width", width as i32)
        .field("height", height as i32)
        // Same "still image" sentinel `build_static_overlay_branch` needs ahead of
        // `imagefreeze` — its sink pad's caps template requires a `framerate` field.
        .field("framerate", gst::Fraction::new(0, 1))
        // Same explicit, identical colorimetry `build_composite_branch`'s matte stage needs
        // on both its GRAY8 and I420 caps — `alphacombine` refuses to combine two inputs with
        // mismatched color range otherwise ("Color range mismatch").
        .field("colorimetry", "bt601")
        .build();
    let mask_appsrc = gst_app::AppSrc::builder()
        .caps(&mask_caps)
        .format(gst::Format::Time)
        .build();
    let mut buffer = gst::Buffer::with_size(mask_gray.len()).map_err(PreviewError::Compositing)?;
    {
        let buffer_mut = buffer.get_mut().expect("freshly created, uniquely owned");
        buffer_mut.set_pts(gst::ClockTime::ZERO);
        let mut map = buffer_mut
            .map_writable()
            .map_err(|_| PreviewError::PushBuffer(gst::FlowError::Error))?;
        map.copy_from_slice(&mask_gray);
    }
    mask_appsrc
        .push_buffer(buffer)
        .map_err(PreviewError::PushBuffer)?;
    let mask_appsrc_element = mask_appsrc.clone().upcast::<gst::Element>();
    let mask_imagefreeze = gst::ElementFactory::make("imagefreeze")
        .property("allow-replace", true)
        .build()
        .map_err(PreviewError::CreateElement)?;

    let sink_caps = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("format", "I420")
                .field("width", width as i32)
                .field("height", height as i32)
                .field("colorimetry", "bt601")
                .build(),
        )
        .build()
        .map_err(PreviewError::CreateElement)?;
    let alphacombine = gst::ElementFactory::make("alphacombine")
        .build()
        .map_err(PreviewError::CreateElement)?;
    let post_convert = gst::ElementFactory::make("videoconvert")
        .build()
        .map_err(PreviewError::CreateElement)?;

    pipeline
        .add_many([
            &mask_appsrc_element,
            &mask_imagefreeze,
            &sink_caps,
            &alphacombine,
            &post_convert,
        ])
        .map_err(PreviewError::Compositing)?;
    mask_appsrc_element
        .link(&mask_imagefreeze)
        .map_err(PreviewError::Compositing)?;

    chain_tail
        .link(&sink_caps)
        .map_err(PreviewError::Compositing)?;
    let color_src = sink_caps
        .static_pad("src")
        .expect("capsfilter always has a src pad");
    let color_sink = alphacombine
        .static_pad("sink")
        .expect("alphacombine always has a sink pad");
    color_src.link(&color_sink).map_err(PreviewError::PadLink)?;

    let alpha_src = mask_imagefreeze
        .static_pad("src")
        .expect("imagefreeze always has a src pad");
    let alpha_sink = alphacombine
        .static_pad("alpha")
        .expect("alphacombine always has an alpha sink pad");
    alpha_src.link(&alpha_sink).map_err(PreviewError::PadLink)?;

    alphacombine
        .link(&post_convert)
        .map_err(PreviewError::Compositing)?;

    Ok((post_convert, mask_appsrc))
}

/// One input (background or overlay) feeding [`Preview::open_composited`]'s `compositor`.
struct CompositeBranch<'a> {
    path: &'a Path,
    clip: Option<&'a ClipInstance>,
    resolution: Option<(u32, u32)>,
    hardware_decode: bool,
    /// `false` for the background (track 0) branch — position/opacity keyframes only drive
    /// `compositor`'s pad properties on an overlay branch, matching export's own
    /// position/opacity-is-overlay-only convention (see `crate::keyframe` module docs).
    is_overlay: bool,
    zorder: u32,
}

/// Builds one branch of [`Preview::open_composited`]'s pipeline — `uridecodebin` (dynamically
/// linked once its video pad appears) through this branch's own effects chain (layer-scale
/// resize, [`build_video_filter_bin`]'s subset, chroma key, background-removal matte) into a
/// freshly requested `compositor` sink pad — and returns that branch's `uridecodebin` element
/// (what [`Preview::seek_composited`] seeks independently, since each branch's source file has
/// its own, unrelated time base — a single pipeline-wide seek would send every branch to the
/// same absolute source time, which is only ever correct by coincidence when clips have
/// different trim points), plus a second `uridecodebin` for the matte file if
/// `clip.background_removal_enabled` and a matte path is set — mirroring export's own
/// `ClipSegment::mask_video_path` gate (see that field's doc comment in `bridge.h`): only
/// consulted on an overlay branch, same as chroma key.
fn build_composite_branch(
    pipeline: &gst::Pipeline,
    compositor: &gst::Element,
    canvas: (u32, u32),
    branch: CompositeBranch,
) -> Result<
    (
        gst::Element,
        Option<gst::Element>,
        Option<ShakeMarginHandle>,
        Option<gst_app::AppSrc>,
        Option<gst_app::AppSink>,
    ),
    PreviewError,
> {
    let uri = gst::glib::filename_to_uri(branch.path, None).map_err(PreviewError::UriConversion)?;
    let decodebin = build_uri_decodebin(uri.as_str(), branch.hardware_decode)?;

    let mut chain: Vec<gst::Element> = vec![gst::ElementFactory::make("videoconvert")
        .build()
        .map_err(PreviewError::CreateElement)?];

    if branch.is_overlay {
        if let (Some(clip), Some((width, height))) = (branch.clip, branch.resolution) {
            if clip.has_layer_scale() {
                // Mirrors export's own layer_scale handling (resolve_clip_filters): the
                // overlay's own decoded frame is resized before compositing, not the
                // compositor's placement — `compositor` pad width/height are left at their
                // default (-1, meaning "use the input's own negotiated size").
                let target_w = ((width as f32 * clip.layer_scale_x).round().max(2.0)) as i32;
                let target_h = ((height as f32 * clip.layer_scale_y).round().max(2.0)) as i32;
                chain.push(
                    gst::ElementFactory::make("videoscale")
                        .build()
                        .map_err(PreviewError::CreateElement)?,
                );
                chain.push(
                    gst::ElementFactory::make("capsfilter")
                        .property(
                            "caps",
                            gst::Caps::builder("video/x-raw")
                                .field("width", target_w)
                                .field("height", target_h)
                                .build(),
                        )
                        .build()
                        .map_err(PreviewError::CreateElement)?,
                );
            }
        }
    }

    let mut shake_handle: Option<ShakeMarginHandle> = None;
    if let Some(clip) = branch.clip {
        let (filter_bin, branch_shake_handle) = build_video_filter_bin(
            clip,
            branch.resolution,
            !branch.is_overlay || clip.has_blend_mode(),
        )?;
        if let Some(filter_bin) = filter_bin {
            chain.push(filter_bin);
        }
        shake_handle = branch_shake_handle;
        if branch.is_overlay && clip.chroma_key_enabled {
            chain.push(build_chroma_key_element(clip)?);
        }
    }

    chain.push(
        gst::ElementFactory::make("videoconvert")
            .build()
            .map_err(PreviewError::CreateElement)?,
    );

    // At this point `chain` already holds this branch's complete pre-matte effects sequence
    // (layer scale, filter bin, chroma key, trailing videoconvert) — every element in it is
    // added/linked together, exactly once, by the single `add_many`/`link_many` call right
    // below. The matte apparatus below is deliberately kept out of `chain` and added/linked
    // separately instead, since it needs its own second input (the matte decode branch) that
    // a plain linear chain can't express — pushing `alphacombine` into `chain` too would give
    // the pipeline two elements sharing the same auto-generated name and fail to add.
    pipeline
        .add(&decodebin)
        .map_err(PreviewError::Compositing)?;
    pipeline
        .add_many(&chain)
        .map_err(PreviewError::Compositing)?;
    gst::Element::link_many(&chain).map_err(PreviewError::Compositing)?;

    let chain_sink = chain
        .first()
        .and_then(|e| e.static_pad("sink"))
        .expect("chain always starts with a videoconvert, which always has a sink pad");
    connect_decodebin_video_pad(&decodebin, chain_sink);

    let chain_tail = chain
        .last()
        .expect("chain always has at least the trailing videoconvert")
        .clone();

    // Layer mask (ClipInstance::mask_shape) — same overlay-only gate as chroma key/matte (a
    // single/background track's alpha never survives to the compositor regardless), applied via
    // `alphacombine` exactly like the matte block below, just with a static rasterized GRAY8
    // buffer (crate::overlay_render::render_mask_shape_gray8) standing in for a decoded matte
    // file — mask_shape has no keyframes, so one buffer suffices, same as
    // build_static_overlay_branch's text/shape branches. Runs *before* the matte block so a
    // clip with both set gets the same "matte replaces, doesn't combine" precedence export's own
    // `bridge.h` documents for `mask_video_path` (the matte's own alphacombine below forces its
    // own I420 input from whatever this stage's alpha-carrying output negotiates down to,
    // discarding this mask's alpha in the process).
    let (chain_tail, mask_appsrc) =
        if branch.is_overlay && branch.clip.is_some_and(|c| c.is_masked()) {
            let clip = branch.clip.expect("checked by is_some_and above");
            let (width, height) = branch.resolution.unwrap_or(canvas);
            let mask_gray = crate::overlay_render::render_mask_shape_gray8(
                clip.mask_shape,
                clip.mask_corner_radius,
                width,
                height,
            );
            let (chain_tail, mask_appsrc) =
                build_mask_shape_stage(pipeline, &chain_tail, mask_gray, (width, height))?;
            (chain_tail, Some(mask_appsrc))
        } else {
            (chain_tail, None)
        };

    // Background-removal matte — mirrors export's `ClipSegment::mask_video_path`/`alphamerge`
    // stage: only meaningful on an overlay branch, only when a matte was actually generated for
    // this clip. `alphacombine` (gst-plugins-bad's `codecalpha` plugin, confirmed present via
    // `gst-inspect-1.0` on this dev machine) takes the `sink` pad's own video and the `alpha`
    // pad's luma plane, producing an alpha-capable output (`A420`/etc.) — the GStreamer
    // counterpart to avfilter's `alphamerge`. Both inputs are forced to the branch's own
    // resolution so their planes line up regardless of the matte's own encoded size (see
    // `crate::background_removal`'s doc comment on how it's generated). Returns the matte's own
    // `uridecodebin` (for independent seeking, see [`Preview::matte_branches`]) and this
    // branch's true final video element — `chain_tail` itself when there's no matte.
    let (matte_decodebin, branch_output) = if branch.is_overlay
        && branch.clip.is_some_and(|c| {
            c.background_removal_enabled && !c.background_removal_mask_path.is_empty()
        }) {
        let clip = branch.clip.expect("checked by is_some_and above");
        let (width, height) = branch.resolution.unwrap_or(canvas);
        let mask_path = Path::new(&clip.background_removal_mask_path);
        match gst::glib::filename_to_uri(mask_path, None) {
            Ok(uri) => {
                let matte_decodebin = build_uri_decodebin(uri.as_str(), branch.hardware_decode)?;
                let matte_convert = gst::ElementFactory::make("videoconvert")
                    .build()
                    .map_err(PreviewError::CreateElement)?;
                let matte_scale = gst::ElementFactory::make("videoscale")
                    .build()
                    .map_err(PreviewError::CreateElement)?;
                let matte_caps = gst::ElementFactory::make("capsfilter")
                    .property(
                        "caps",
                        gst::Caps::builder("video/x-raw")
                            .field("format", "GRAY8")
                            .field("width", width as i32)
                            .field("height", height as i32)
                            // Explicit, identical colorimetry on both this and `sink_caps`
                            // below — `alphacombine` refuses to combine two inputs with
                            // mismatched color range ("Color range mismatch"), which the
                            // matte's own encode and the main chain's own negotiated caps
                            // otherwise don't guarantee agree on, confirmed empirically
                            // (`gst_alpha_combine_negotiate`'s error message, not guessed).
                            .field("colorimetry", "bt601")
                            .build(),
                    )
                    .build()
                    .map_err(PreviewError::CreateElement)?;
                // Forces `chain_tail`'s output into I420 before it reaches `alphacombine`'s
                // `sink` pad — its pad template doesn't accept the unconstrained/RGBA-negotiated
                // caps `chain_tail` otherwise produces.
                let sink_caps = gst::ElementFactory::make("capsfilter")
                    .property(
                        "caps",
                        gst::Caps::builder("video/x-raw")
                            .field("format", "I420")
                            .field("width", width as i32)
                            .field("height", height as i32)
                            .field("colorimetry", "bt601")
                            .build(),
                    )
                    .build()
                    .map_err(PreviewError::CreateElement)?;
                let alphacombine = gst::ElementFactory::make("alphacombine")
                    .build()
                    .map_err(PreviewError::CreateElement)?;
                let post_convert = gst::ElementFactory::make("videoconvert")
                    .build()
                    .map_err(PreviewError::CreateElement)?;

                pipeline
                    .add_many([
                        &matte_decodebin,
                        &matte_convert,
                        &matte_scale,
                        &matte_caps,
                        &sink_caps,
                        &alphacombine,
                        &post_convert,
                    ])
                    .map_err(PreviewError::Compositing)?;
                gst::Element::link_many([&matte_convert, &matte_scale, &matte_caps])
                    .map_err(PreviewError::Compositing)?;
                let matte_sink = matte_convert
                    .static_pad("sink")
                    .expect("videoconvert always has a sink pad");
                connect_decodebin_video_pad(&matte_decodebin, matte_sink);

                chain_tail
                    .link(&sink_caps)
                    .map_err(PreviewError::Compositing)?;

                let color_src = sink_caps
                    .static_pad("src")
                    .expect("capsfilter always has a src pad");
                let color_sink = alphacombine
                    .static_pad("sink")
                    .expect("alphacombine always has a sink pad");
                color_src.link(&color_sink).map_err(PreviewError::PadLink)?;

                let alpha_src = matte_caps
                    .static_pad("src")
                    .expect("capsfilter always has a src pad");
                let alpha_sink = alphacombine
                    .static_pad("alpha")
                    .expect("alphacombine always has an alpha sink pad");
                alpha_src.link(&alpha_sink).map_err(PreviewError::PadLink)?;

                alphacombine
                    .link(&post_convert)
                    .map_err(PreviewError::Compositing)?;

                (Some(matte_decodebin), post_convert)
            }
            Err(e) => {
                // Same "degrade rather than abort" posture export's matte handling has for a
                // stale/deleted cache file — no matte compositing this branch, not a failed
                // preview pipeline.
                tracing::warn!(error = ?e, path = %mask_path.display(), "failed to resolve matte path for preview, compositing without it");
                (None, chain_tail.clone())
            }
        }
    } else {
        (None, chain_tail.clone())
    };

    // A blend-mode overlay bypasses `compositor` entirely (it only knows Porter-Duff
    // `source`/`over`/`add` — verified via `gst-inspect-1.0 compositor` — not the 40
    // Photoshop-style modes `BlendMode` models) and instead lands in its own dedicated
    // `appsink`, forced to the canvas's own size (`ClipInstance::blend_mode`'s doc comment
    // already documents that a blend-mode layer ignores position/PIP) — [`Preview::current_frame`]
    // pulls this alongside the main `compositor` output and composites it in on the CPU via
    // [`crate::blend_mode::blend_channel`], the same formulas export's `blend=all_mode=` uses.
    if branch.is_overlay && branch.clip.is_some_and(|c| c.has_blend_mode()) {
        let (width, height) = canvas;
        let scale = gst::ElementFactory::make("videoscale")
            .build()
            .map_err(PreviewError::CreateElement)?;
        let caps = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-raw")
                    .field("format", "RGBA")
                    .field("width", width as i32)
                    .field("height", height as i32)
                    .build(),
            )
            .build()
            .map_err(PreviewError::CreateElement)?;
        let appsink = gst_app::AppSink::builder()
            .caps(
                &gst::Caps::builder("video/x-raw")
                    .field("format", "RGBA")
                    .build(),
            )
            .sync(true)
            .max_buffers(1)
            .drop(true)
            .build();
        pipeline
            .add_many([&scale, &caps, appsink.upcast_ref()])
            .map_err(PreviewError::Compositing)?;
        gst::Element::link_many([&scale, &caps, appsink.upcast_ref()])
            .map_err(PreviewError::Compositing)?;

        branch_output
            .link(&scale)
            .map_err(PreviewError::Compositing)?;

        return Ok((
            decodebin,
            matte_decodebin,
            shake_handle,
            mask_appsrc,
            Some(appsink),
        ));
    }

    let sink_pad = compositor
        .request_pad_simple("sink_%u")
        .ok_or(PreviewError::RequestPad)?;
    sink_pad.set_property("zorder", branch.zorder);
    sink_pad.set_property("xpos", 0i32);
    sink_pad.set_property("ypos", 0i32);
    sink_pad.set_property("alpha", 1.0f64);

    if branch.is_overlay {
        if let Some(clip) = branch.clip {
            let position_keyframes = clip.position_keyframes.clone();
            let opacity_keyframes = clip.opacity_keyframes.clone();
            let source_in_secs = clip.source_in_secs;
            let clip_duration_secs = (clip.source_out_secs - clip.source_in_secs).max(1e-6);
            let (canvas_w, canvas_h) = canvas;
            let pad_for_probe = sink_pad.clone();

            let chain_src = branch_output
                .static_pad("src")
                .expect("branch_output is always a videoconvert, which always has a src pad");
            chain_src.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
                let secs = info
                    .buffer()
                    .and_then(|b| b.pts())
                    .map(|t| t.seconds_f64())
                    .unwrap_or(source_in_secs);
                let frac = ((secs - source_in_secs) / clip_duration_secs).clamp(0.0, 1.0) as f32;
                let pos = crate::keyframe::evaluate_keyframes(
                    &position_keyframes,
                    frac,
                    crate::keyframe::Position { x: 0.0, y: 0.0 },
                );
                let alpha = crate::keyframe::evaluate_keyframes(&opacity_keyframes, frac, 1.0)
                    .clamp(0.0, 1.0);
                pad_for_probe.set_property("xpos", (pos.x * canvas_w as f32).round() as i32);
                pad_for_probe.set_property("ypos", (pos.y * canvas_h as f32).round() as i32);
                pad_for_probe.set_property("alpha", alpha as f64);
                gst::PadProbeReturn::Ok
            });
        }
    }

    let chain_out = branch_output
        .static_pad("src")
        .expect("branch_output is always a videoconvert, which always has a src pad");
    chain_out.link(&sink_pad).map_err(PreviewError::PadLink)?;

    Ok((decodebin, matte_decodebin, shake_handle, mask_appsrc, None))
}

fn push_rgba_overlay_buffer(appsrc: &gst_app::AppSrc, rgba: Vec<u8>) -> Result<(), PreviewError> {
    let mut buffer = gst::Buffer::with_size(rgba.len()).map_err(PreviewError::Compositing)?;
    {
        let buffer_mut = buffer.get_mut().expect("freshly created, uniquely owned");
        buffer_mut.set_pts(gst::ClockTime::ZERO);
        let mut map = buffer_mut
            .map_writable()
            .map_err(|_| PreviewError::PushBuffer(gst::FlowError::Error))?;
        map.copy_from_slice(&rgba);
    }
    appsrc
        .push_buffer(buffer)
        .map(|_| ())
        .map_err(PreviewError::PushBuffer)
}

/// Builds one text/shape overlay branch of [`Preview::open_composited`]'s pipeline:
/// `appsrc` (pushed exactly one already-rasterized full-canvas RGBA buffer, see
/// [`crate::overlay_render`]) through `imagefreeze` (repeats that single buffer indefinitely,
/// deriving fresh timestamps off the pipeline's own clock) into a freshly requested `compositor`
/// sink pad. `allow_replace` enables `imagefreeze`'s verified `allow-replace` property and keeps
/// `appsrc` open so word-highlight text can replace this buffer as playback crosses a timing
/// boundary; shape branches still send EOS after their one immutable frame. Unlike
/// [`build_composite_branch`], there's no seeking — [`TextClip`]/[`ShapeClip`] have no source
/// file of their own — so this needs no entry in [`Preview::branches`].
/// `rgba` already covers the whole canvas with the text/shape positioned within it (matching
/// export's own absolute-pixel-position convention), so the compositor pad is placed at
/// `(0, 0)` at the canvas's own size rather than needing per-pad position math.
fn build_static_overlay_branch(
    pipeline: &gst::Pipeline,
    compositor: &gst::Element,
    canvas: (u32, u32),
    rgba: Vec<u8>,
    zorder: u32,
    allow_replace: bool,
) -> Result<gst_app::AppSrc, PreviewError> {
    let (width, height) = canvas;
    let caps = gst::Caps::builder("video/x-raw")
        .field("format", "RGBA")
        .field("width", width as i32)
        .field("height", height as i32)
        // `0/1` is the conventional "still image, not a real framerate" sentinel decoders like
        // `jpegdec`/`pngdec` use ahead of `imagefreeze` — its sink pad's caps template requires
        // a `framerate` field to negotiate at all (a fixed caps with none omitted entirely was
        // rejected outright, confirmed empirically: `imagefreeze0:sink> caps ... not accepted`).
        .field("framerate", gst::Fraction::new(0, 1))
        .build();

    let appsrc = gst_app::AppSrc::builder()
        .caps(&caps)
        .format(gst::Format::Time)
        .build();
    push_rgba_overlay_buffer(&appsrc, rgba)?;
    if !allow_replace {
        appsrc.end_of_stream().map_err(PreviewError::PushBuffer)?;
    }

    let imagefreeze = gst::ElementFactory::make("imagefreeze")
        .property("allow-replace", allow_replace)
        .build()
        .map_err(PreviewError::CreateElement)?;
    let appsrc_element = appsrc.clone().upcast::<gst::Element>();

    pipeline
        .add_many([&appsrc_element, &imagefreeze])
        .map_err(PreviewError::Compositing)?;
    gst::Element::link_many([&appsrc_element, &imagefreeze]).map_err(PreviewError::Compositing)?;

    let sink_pad = compositor
        .request_pad_simple("sink_%u")
        .ok_or(PreviewError::RequestPad)?;
    sink_pad.set_property("zorder", zorder);
    sink_pad.set_property("xpos", 0i32);
    sink_pad.set_property("ypos", 0i32);
    sink_pad.set_property("alpha", 1.0f64);

    let src_pad = imagefreeze
        .static_pad("src")
        .expect("imagefreeze always has a src pad");
    src_pad.link(&sink_pad).map_err(PreviewError::PadLink)?;

    Ok(appsrc)
}

struct TextOverlayBranch {
    clip_id: u64,
    appsrc: gst_app::AppSrc,
    active_word_index: Option<usize>,
    canvas_width: u32,
    canvas_height: u32,
}

/// A replaceable `appsrc ! imagefreeze(allow-replace=true)` branch for one [`ShapeClip`] overlay
/// built by [`open_composited`] — same replace-in-place shape [`TextOverlayBranch`] has, so
/// [`Preview::refresh_shape_overlay`] can push a freshly rasterized buffer without rebuilding the
/// pipeline. Closes P1 item 3's "Shapes (non-text overlays) still have no live-preview path" gap
/// (not to be confused with [`MaskShapeBranch`], which is a background-removal matte input, not
/// a `TrackKind::Shape` overlay clip).
struct ShapeOverlayBranch {
    clip_id: u64,
    appsrc: gst_app::AppSrc,
    canvas_width: u32,
    canvas_height: u32,
}

/// A replaceable `appsrc ! imagefreeze(allow-replace=true)` mask branch built by
/// [`build_mask_shape_stage`] for one overlay clip's `mask_shape` — same replace-in-place shape
/// [`TextOverlayBranch`] has, so [`Preview::set_live_mask`] can push a freshly rasterized GRAY8
/// buffer without rebuilding the pipeline (P1 item 3's remaining live-preview-update gap).
struct MaskShapeBranch {
    clip_id: u64,
    appsrc: gst_app::AppSrc,
    width: u32,
    height: u32,
}

/// Real-time audio level snapshot (linear `0.0..=1.0` amplitude, not dBFS) computed from the
/// preview's own downstream audio-sink pad probe — per `spec/ROADMAP.md` P4 item 30, "Real-time
/// audio level meter (VU/peak) during playback". `peak` is the loudest single sample's absolute
/// value seen in the most recently probed buffer; `rms` is that buffer's root-mean-square,
/// combined across every channel (a stereo/5.1 buffer's interleaved samples treated as one flat
/// sequence). `peak_l`/`rms_l`/`peak_r`/`rms_r` are the same two measures split to channel 0
/// ("L") and channel 1 ("R") of the source — a mono buffer reports the same values on both (a
/// mono source, like a real hardware VU meter, still reads identically on both bars); a buffer
/// with more than two channels (5.1, etc.) still counts every channel toward `peak`/`rms`
/// above, but only channels 0/1 feed the L/R split — this stays a small stereo meter widget,
/// not a full per-channel Fairlight-style one.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct AudioLevel {
    pub peak: f32,
    pub rms: f32,
    pub peak_l: f32,
    pub rms_l: f32,
    pub peak_r: f32,
    pub rms_r: f32,
}

/// Builds a small `audioconvert ! capsfilter(F32LE) ! sink` bin usable as `playbin`'s
/// `audio-sink` property ([`Preview::open`]) or in place of a bare sink element in a manually
/// built pipeline ([`build_audio_mix_output`]) — forcing a known sample format lets the buffer
/// probe below parse raw bytes directly instead of branching on whatever format the pipeline
/// happened to negotiate. `sink` is the real output element (an `autoaudiosink`, or a
/// `fakesink` fallback when no audio device is available). The returned `Arc<Mutex<AudioLevel>>`
/// is updated from GStreamer's own streaming thread on every buffer — callers read it from the
/// UI thread via [`Preview::current_audio_level`], never inside the probe itself.
fn build_metering_audio_sink(
    sink: gst::Element,
) -> Result<(gst::Element, std::sync::Arc<std::sync::Mutex<AudioLevel>>), PreviewError> {
    let convert = gst::ElementFactory::make("audioconvert")
        .build()
        .map_err(PreviewError::CreateElement)?;
    let capsfilter = gst::ElementFactory::make("capsfilter")
        .property(
            "caps",
            gst::Caps::builder("audio/x-raw")
                .field("format", "F32LE")
                .build(),
        )
        .build()
        .map_err(PreviewError::CreateElement)?;

    let bin = gst::Bin::new();
    bin.add_many([&convert, &capsfilter, &sink])
        .map_err(PreviewError::FilterBin)?;
    gst::Element::link_many([&convert, &capsfilter, &sink]).map_err(PreviewError::FilterBin)?;
    let sink_pad = convert
        .static_pad("sink")
        .expect("audioconvert always has a sink pad");
    let ghost_sink = gst::GhostPad::with_target(&sink_pad).map_err(PreviewError::FilterBin)?;
    bin.add_pad(&ghost_sink).map_err(PreviewError::FilterBin)?;

    let level = std::sync::Arc::new(std::sync::Mutex::new(AudioLevel::default()));
    let level_for_probe = level.clone();
    let capsfilter_src = capsfilter
        .static_pad("src")
        .expect("capsfilter always has a src pad");
    capsfilter_src.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        if let Some(buffer) = info.buffer() {
            if let Ok(map) = buffer.map_readable() {
                let bytes = map.as_slice();
                let sample_count = bytes.len() / 4;
                // Negotiated channel count, read off the pad's own caps (same `current_caps`
                // pattern used elsewhere in this file) -- needed to de-interleave channel 0
                // ("L") from channel 1 ("R") below. Missing/zero falls back to 1 (mono), the
                // safe default: every sample then counts as both L and R via the `channels <=
                // 1` branch after the loop, never an out-of-bounds or wrong split.
                let channels = pad
                    .current_caps()
                    .and_then(|caps| {
                        caps.structure(0)
                            .and_then(|s| s.get::<i32>("channels").ok())
                    })
                    .filter(|&c| c > 0)
                    .unwrap_or(1) as usize;

                let mut peak = 0.0f32;
                let mut sum_sq = 0.0f64;
                let mut peak_l = 0.0f32;
                let mut sum_sq_l = 0.0f64;
                let mut count_l = 0usize;
                let mut peak_r = 0.0f32;
                let mut sum_sq_r = 0.0f64;
                let mut count_r = 0usize;
                for i in 0..sample_count {
                    let sample = f32::from_le_bytes([
                        bytes[i * 4],
                        bytes[i * 4 + 1],
                        bytes[i * 4 + 2],
                        bytes[i * 4 + 3],
                    ]);
                    let abs = sample.abs();
                    if abs > peak {
                        peak = abs;
                    }
                    sum_sq += (sample as f64) * (sample as f64);

                    match i % channels {
                        0 => {
                            if abs > peak_l {
                                peak_l = abs;
                            }
                            sum_sq_l += (sample as f64) * (sample as f64);
                            count_l += 1;
                        }
                        1 => {
                            if abs > peak_r {
                                peak_r = abs;
                            }
                            sum_sq_r += (sample as f64) * (sample as f64);
                            count_r += 1;
                        }
                        // Channel 2+ (5.1, etc.) -- still counted in the combined peak/rms
                        // above, not split into a third/fourth bar this small meter doesn't have.
                        _ => {}
                    }
                }
                let rms = if sample_count > 0 {
                    (sum_sq / sample_count as f64).sqrt() as f32
                } else {
                    0.0
                };
                let (peak_l, rms_l, peak_r, rms_r) = if channels <= 1 {
                    (peak, rms, peak, rms)
                } else {
                    let rms_l = if count_l > 0 {
                        (sum_sq_l / count_l as f64).sqrt() as f32
                    } else {
                        0.0
                    };
                    let rms_r = if count_r > 0 {
                        (sum_sq_r / count_r as f64).sqrt() as f32
                    } else {
                        0.0
                    };
                    (peak_l, rms_l, peak_r, rms_r)
                };
                if let Ok(mut level) = level_for_probe.lock() {
                    *level = AudioLevel {
                        peak,
                        rms,
                        peak_l,
                        rms_l,
                        peak_r,
                        rms_r,
                    };
                }
            }
        }
        gst::PadProbeReturn::Ok
    });

    Ok((bin.upcast::<gst::Element>(), level))
}

/// A media pipeline loaded for preview playback — either a single file via `playbin`
/// ([`Self::open`]) or a multi-track `compositor` pipeline ([`Self::open_composited`]). Owns
/// the pipeline; dropping it tears the pipeline down (`State::Null`) so GStreamer releases any
/// decoder/output resources.
pub struct Preview {
    pipeline: gst::Element,
    video_sink: gst_app::AppSink,
    /// Each branch's own `uridecodebin`, in the same order [`Self::open_composited`]'s
    /// `overlays` were given (background first) — what [`Self::seek_composited`] seeks
    /// independently. Empty for a [`Self::open`]-opened single-clip pipeline, which uses the
    /// plain [`Self::seek`]/[`Self::seek_with_rate`] instead.
    branches: Vec<gst::Element>,
    /// `(branch_index, matte uridecodebin, clip.source_in_secs)` for every overlay branch with
    /// a background-removal matte. `branch_index` indexes into [`Self::branches`]/the offsets
    /// [`Self::seek_composited`] is given — the matte plays its own 0-based clip (see
    /// `ClipSegment::mask_video_path`'s doc comment in `bridge.h` for why: it was sampled
    /// directly from the overlay clip's own trimmed source range, not the timeline), so
    /// `seek_composited` derives its seek target from the matching branch's own offset minus
    /// `source_in_secs` rather than taking a separate offset from the caller — `ui`'s `App`
    /// never needs to know mattes exist.
    matte_branches: Vec<(usize, gst::Element, f64)>,
    /// Replaceable `appsrc ! imagefreeze(allow-replace=true)` branches for active text clips.
    /// Their order and ids mirror `open_composited`'s `text_overlays` argument.
    text_overlay_branches: Vec<TextOverlayBranch>,
    /// Replaceable `appsrc ! imagefreeze(allow-replace=true)` branches for active shape overlay
    /// clips. Their order and ids mirror `open_composited`'s `shape_overlays` argument — see
    /// [`Self::refresh_shape_overlay`].
    shape_overlay_branches: Vec<ShapeOverlayBranch>,
    /// Replaceable mask branches, keyed implicitly by [`MaskShapeBranch::clip_id`] — see
    /// [`Self::set_live_mask`].
    mask_shape_branches: Vec<MaskShapeBranch>,
    /// Live audio level, updated on GStreamer's own streaming thread by a buffer probe
    /// [`build_metering_audio_sink`] installs just ahead of the real audio output element —
    /// read from the UI thread via [`Self::current_audio_level`]. Per `spec/ROADMAP.md` P4
    /// item 30.
    audio_level: std::sync::Arc<std::sync::Mutex<AudioLevel>>,
    /// Each previewed clip's actual decoded pixel size, as last resolved when its branch was
    /// built — [`Self::set_live_crop`]/[`Self::set_live_pixelize`] need this to redo the
    /// fraction-to-pixel math themselves, so `ui`'s `App` (which only ever deals in
    /// [`ClipInstance`]'s normalized `0.0..=1.0` crop/pixelize fields) doesn't need to know or
    /// track decoded frame sizes at all.
    clip_resolutions: std::collections::HashMap<u64, (u32, u32)>,
    /// Live-updatable shake margins, one per clip with a non-neutral `shake_intensity` when its
    /// branch was built — see [`ShakeMarginHandle`]/[`Self::set_live_shake`].
    shake_handles: std::collections::HashMap<u64, ShakeMarginHandle>,
    /// `(appsink, mode, zorder)` for every overlay branch with a non-[`BlendMode::Normal`]
    /// `ClipInstance::blend_mode` — [`build_composite_branch`] diverts these away from the
    /// shared `compositor` into their own dedicated `appsink` (`compositor` has no blend-mode
    /// concept of its own), and [`Self::current_frame`] composites their frames onto the
    /// `compositor` output itself, in ascending `zorder`, via [`crate::blend_mode::blend_channel`].
    /// Always empty for a [`Self::open`]-opened single-clip pipeline. Documented scope limit
    /// (same as export's own, see `ClipInstance::blend_mode`'s doc comment): these layers always
    /// end up on top of the whole `compositor` stack, not correctly interleaved with Normal-mode
    /// overlay layers above them in track order, and ignore position/PIP.
    blend_branches: Vec<(gst_app::AppSink, BlendMode, u32)>,
}

impl Preview {
    /// Opens `path` — the clip's resolved source (proxy or original) — and brings the
    /// pipeline up to `Paused` (decodes enough to preroll, so
    /// [`Self::position_secs`]/[`Self::duration_secs`]/[`Self::current_frame`] have something
    /// to report). `clip`, if given, has its effects (the subset [`build_video_filter_bin`]
    /// covers) applied via `playbin`'s `video-filter` property — probed from `path` itself
    /// first (not passed in) since `path` may be a lower-resolution editing proxy, and
    /// `videocrop`'s properties need the actual decoded pixel size. `None` skips both the
    /// probe and any filtering — for callers with no clip in scope at all (e.g. filmstrip
    /// thumbnail extraction, which always shows the raw source regardless of applied effects).
    pub fn open(path: &Path, clip: Option<&ClipInstance>) -> Result<Self, PreviewError> {
        Self::open_with_hardware_decode(path, clip, true)
    }

    /// Opens a single-source preview while allowing the caller to disable hardware decoding.
    /// Hardware acceleration is preferred when enabled and available; if decoder preroll
    /// fails, the pipeline is rebuilt once with software-only decoding before returning an
    /// error. This keeps [`Self::open`]'s default fast without sacrificing CPU compatibility.
    pub fn open_with_hardware_decode(
        path: &Path,
        clip: Option<&ClipInstance>,
        hardware_decode: bool,
    ) -> Result<Self, PreviewError> {
        gst::init().map_err(PreviewError::Init)?;
        let hardware_decode = hardware_decode && !prioritize_hardware_video_decoders().is_empty();
        match Self::open_once(path, clip, hardware_decode) {
            Err(PreviewError::StateChange(error)) if hardware_decode => {
                tracing::warn!(
                    error = %error,
                    "hardware preview decoder failed to preroll; retrying with CPU"
                );
                Self::open_once(path, clip, false)
            }
            result => result,
        }
    }

    fn open_once(
        path: &Path,
        clip: Option<&ClipInstance>,
        hardware_decode: bool,
    ) -> Result<Self, PreviewError> {
        gst::init().map_err(PreviewError::Init)?;

        let uri = gst::glib::filename_to_uri(path, None).map_err(PreviewError::UriConversion)?;

        let pipeline = gst::ElementFactory::make("playbin")
            .build()
            .map_err(PreviewError::CreateElement)?;
        pipeline.set_property("uri", uri.as_str());
        configure_playbin_decoder_preference(&pipeline, hardware_decode);

        let mut clip_resolutions = std::collections::HashMap::new();
        let mut shake_handles = std::collections::HashMap::new();
        if let Some(clip) = clip {
            let resolution = avbridge::probe(path)
                .map_err(PreviewError::Probe)?
                .resolution;
            if let Some(resolution) = resolution {
                clip_resolutions.insert(clip.id, resolution);
            }
            let (filter_bin, shake_handle) = build_video_filter_bin(clip, resolution, true)?;
            if let Some(filter_bin) = filter_bin {
                pipeline.set_property("video-filter", &filter_bin);
            }
            if let Some(shake_handle) = shake_handle {
                shake_handles.insert(clip.id, shake_handle);
            }
            if let Some(audio_filter_bin) = build_audio_filter_bin(clip)? {
                pipeline.set_property("audio-filter", &audio_filter_bin);
            }
        }

        // Fixed RGBA caps: whatever the source's actual pixel format is (planar YUV, etc.),
        // playbin inserts the conversion elements needed to match this — callers of
        // current_frame() never need to handle more than one, simple, packed format.
        let video_caps = gst::Caps::builder("video/x-raw")
            .field("format", "RGBA")
            .build();
        let video_sink = gst_app::AppSink::builder()
            .caps(&video_caps)
            .sync(true)
            .max_buffers(1)
            .drop(true)
            .build();
        pipeline.set_property("video-sink", &video_sink);

        // Real audio output — `autoaudiosink` picks whatever's actually available (wasapi/
        // directsound on Windows, alsa/pulse on Linux). If this machine has no usable audio
        // device at all, the Paused transition below fails; falls back to `fakesink` (silent,
        // matching the old always-fakesink behavior) and retries once rather than making the
        // whole preview unavailable over a missing/broken audio device — same "degrade rather
        // than abort" posture the rest of this module already has for a missing matte file or
        // an unloadable font.
        let audio_sink = gst::ElementFactory::make("autoaudiosink")
            .build()
            .map_err(PreviewError::CreateElement)?;
        let (metering_sink, mut audio_level) = build_metering_audio_sink(audio_sink)?;
        pipeline.set_property("audio-sink", &metering_sink);

        pipeline
            .set_state(gst::State::Paused)
            .map_err(PreviewError::StateChange)?;
        // Block until the Paused transition actually completes (it's commonly Async — the
        // preroll happens on GStreamer's own threads) so duration/position/frame queries made
        // right after `open()` returns are reliable instead of racing the preroll.
        let (result, _current, _pending) = pipeline.state(gst::ClockTime::from_seconds(5));
        if result.is_err() {
            tracing::warn!(
                "failed to preroll with a real audio sink, retrying muted (no audio device?)"
            );
            pipeline
                .set_state(gst::State::Null)
                .map_err(PreviewError::StateChange)?;
            let fakesink = gst::ElementFactory::make("fakesink")
                .build()
                .map_err(PreviewError::CreateElement)?;
            let (metering_fakesink, fakesink_audio_level) = build_metering_audio_sink(fakesink)?;
            audio_level = fakesink_audio_level;
            pipeline.set_property("audio-sink", &metering_fakesink);
            pipeline
                .set_state(gst::State::Paused)
                .map_err(PreviewError::StateChange)?;
            let (result, _current, _pending) = pipeline.state(gst::ClockTime::from_seconds(5));
            result.map_err(PreviewError::StateChange)?;
        }

        Ok(Self {
            pipeline,
            video_sink,
            branches: Vec::new(),
            matte_branches: Vec::new(),
            text_overlay_branches: Vec::new(),
            shape_overlay_branches: Vec::new(),
            mask_shape_branches: Vec::new(),
            audio_level,
            clip_resolutions,
            shake_handles,
            blend_branches: Vec::new(),
        })
    }

    /// Opens a multi-track composited pipeline: `background` (track 0, `is_overlay: false`)
    /// underneath every entry in `overlays` (in the given order — later entries draw on top,
    /// matching [`crate::render::resolve_timeline_segments_multi`]'s "track index = z-order"
    /// convention), mixed live via `compositor` instead of `playbin`'s single-source decode.
    /// Wires up the subset of overlay-only effects export already gates the same way (see
    /// `crate::keyframe` module docs): position/opacity keyframes drive `compositor`'s own
    /// per-pad `xpos`/`ypos`/`alpha`, layer scale resizes the branch's decoded frame before
    /// compositing, and chroma key removes a background color before the branch reaches the
    /// mixer — all recomputed per buffer off the branch's own PTS, same pad-probe technique
    /// [`build_video_filter_bin`]'s scale/rotation keyframes already use.
    ///
    /// Each branch's `uridecodebin` seeks independently ([`Self::seek_composited`]) rather than
    /// through a single pipeline-wide seek — the branches are different source files with
    /// unrelated time bases (different trim points), so one absolute seek position wouldn't be
    /// simultaneously correct for all of them. Once each branch's own starting offset is set,
    /// `Playing` advances every branch together at the same rate off the pipeline's one shared
    /// clock — no further per-branch bookkeeping needed during playback itself.
    ///
    /// `background`'s resolution sizes the canvas every overlay's layer-scale/position math is
    /// expressed in pixels against — [`PreviewError::NoBackgroundVideo`] if it can't be probed.
    /// Every branch's own `ClipInstance::speed_factor` is honored via
    /// [`Self::seek_composited`]'s `rates` argument, called right after this returns — this
    /// method itself only opens the pipeline (implicitly rate `1.0` until the first seek).
    ///
    /// `audio_overlays` contains active clips from audio-only timeline tracks. Their decodebins,
    /// together with every video branch that has embedded audio, feed one `audiomixer`; each
    /// branch applies its own gain and is included in `branches` so independent trim-aware seek
    /// offsets and playback rates stay aligned with the picture.
    ///
    /// `text_overlays`/`shape_overlays` — the clips covering the playhead on any
    /// [`crate::timeline::TrackKind::Text`]/[`crate::timeline::TrackKind::Shape`] track, if
    /// any — are rasterized ([`crate::overlay_render`]) and composited on top of
    /// every `overlays` video branch (drawn last, matching export's own text/shape
    /// post-processing passes running after the main timeline composite). Shapes stay static;
    /// text branches use `imagefreeze(allow-replace=true)` so [`Self::update_text_overlays`]
    /// can replace the repeated image at word boundaries without rebuilding this pipeline.
    ///
    /// Each `text_overlays` entry pairs a clip with the elapsed time since its own
    /// `start_secs` — the instant [`crate::overlay_render::render_text_clip_rgba`] resolves
    /// [`crate::timeline::TextClip::highlight_enabled`]'s initial word against. The UI then
    /// calls [`Self::update_text_overlays`] during uninterrupted playback and explicit seeks;
    /// unchanged words are no-ops rather than full-canvas uploads every frame.
    pub fn open_composited(
        background_path: &Path,
        background_clip: Option<&ClipInstance>,
        overlays: &[(&Path, &ClipInstance)],
        audio_overlays: &[(&Path, &ClipInstance)],
        text_overlays: &[(&TextClip, f64)],
        shape_overlays: &[&ShapeClip],
    ) -> Result<Self, PreviewError> {
        Self::open_composited_with_hardware_decode(
            background_path,
            background_clip,
            overlays,
            audio_overlays,
            text_overlays,
            shape_overlays,
            true,
        )
    }

    /// Opens a composited preview while allowing the caller to force every `uridecodebin`
    /// branch to software decoding. Enabled hardware decoding gets the same one-time CPU retry
    /// as [`Self::open_with_hardware_decode`] if pipeline preroll fails.
    #[allow(clippy::too_many_arguments)]
    pub fn open_composited_with_hardware_decode(
        background_path: &Path,
        background_clip: Option<&ClipInstance>,
        overlays: &[(&Path, &ClipInstance)],
        audio_overlays: &[(&Path, &ClipInstance)],
        text_overlays: &[(&TextClip, f64)],
        shape_overlays: &[&ShapeClip],
        hardware_decode: bool,
    ) -> Result<Self, PreviewError> {
        gst::init().map_err(PreviewError::Init)?;
        let hardware_decode = hardware_decode && !prioritize_hardware_video_decoders().is_empty();
        match Self::open_composited_once(
            background_path,
            background_clip,
            overlays,
            audio_overlays,
            text_overlays,
            shape_overlays,
            hardware_decode,
        ) {
            Err(PreviewError::StateChange(error)) if hardware_decode => {
                tracing::warn!(
                    error = %error,
                    "hardware composited preview decoder failed to preroll; retrying with CPU"
                );
                Self::open_composited_once(
                    background_path,
                    background_clip,
                    overlays,
                    audio_overlays,
                    text_overlays,
                    shape_overlays,
                    false,
                )
            }
            result => result,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn open_composited_once(
        background_path: &Path,
        background_clip: Option<&ClipInstance>,
        overlays: &[(&Path, &ClipInstance)],
        audio_overlays: &[(&Path, &ClipInstance)],
        text_overlays: &[(&TextClip, f64)],
        shape_overlays: &[&ShapeClip],
        hardware_decode: bool,
    ) -> Result<Self, PreviewError> {
        gst::init().map_err(PreviewError::Init)?;

        let background_info = avbridge::probe(background_path).map_err(PreviewError::Probe)?;
        let canvas = background_info
            .resolution
            .ok_or(PreviewError::NoBackgroundVideo)?;
        let overlay_infos: Vec<avbridge::ProbeInfo> = overlays
            .iter()
            .map(|(path, _)| avbridge::probe(path).map_err(PreviewError::Probe))
            .collect::<Result<_, _>>()?;
        let audio_infos: Vec<avbridge::ProbeInfo> = audio_overlays
            .iter()
            .map(|(path, _)| avbridge::probe(path).map_err(PreviewError::Probe))
            .collect::<Result<_, _>>()?;
        let has_any_audio = background_info.has_audio
            || overlay_infos.iter().any(|info| info.has_audio)
            || audio_infos.iter().any(|info| info.has_audio);

        let pipeline = gst::Pipeline::new();

        let compositor = gst::ElementFactory::make("compositor")
            .build()
            .map_err(PreviewError::CreateElement)?;
        let out_convert = gst::ElementFactory::make("videoconvert")
            .build()
            .map_err(PreviewError::CreateElement)?;
        let out_caps = gst::ElementFactory::make("capsfilter")
            .property(
                "caps",
                gst::Caps::builder("video/x-raw")
                    .field("format", "RGBA")
                    .build(),
            )
            .build()
            .map_err(PreviewError::CreateElement)?;
        pipeline
            .add_many([&compositor, &out_convert, &out_caps])
            .map_err(PreviewError::Compositing)?;
        gst::Element::link_many([&compositor, &out_convert, &out_caps])
            .map_err(PreviewError::Compositing)?;

        let video_caps = gst::Caps::builder("video/x-raw")
            .field("format", "RGBA")
            .build();
        let video_sink = gst_app::AppSink::builder()
            .caps(&video_caps)
            .sync(true)
            .max_buffers(1)
            .drop(true)
            .build();
        pipeline
            .add(&video_sink)
            .map_err(PreviewError::Compositing)?;
        out_caps
            .link(&video_sink)
            .map_err(PreviewError::Compositing)?;

        let (audio_mixer, audio_level) = match has_any_audio
            .then(|| build_audio_mix_output(&pipeline))
            .transpose()?
        {
            Some((mixer, level)) => (Some(mixer), level),
            // No audio anywhere in this composited timeline -- nothing ever updates the level,
            // so it just stays at its silent default rather than needing an Option everywhere
            // downstream.
            None => (
                None,
                std::sync::Arc::new(std::sync::Mutex::new(AudioLevel::default())),
            ),
        };

        let mut clip_resolutions = std::collections::HashMap::new();
        let mut shake_handles = std::collections::HashMap::new();
        let mut mask_shape_branches = Vec::new();

        let (background_decodebin, background_matte, background_shake, background_mask, _) =
            build_composite_branch(
                &pipeline,
                &compositor,
                canvas,
                CompositeBranch {
                    path: background_path,
                    clip: background_clip,
                    resolution: Some(canvas),
                    hardware_decode,
                    is_overlay: false,
                    zorder: 0,
                },
            )?;
        if let Some(clip) = background_clip {
            clip_resolutions.insert(clip.id, canvas);
            if let Some(shake_handle) = background_shake {
                shake_handles.insert(clip.id, shake_handle);
            }
            if let Some(appsrc) = background_mask {
                mask_shape_branches.push(MaskShapeBranch {
                    clip_id: clip.id,
                    appsrc,
                    width: canvas.0,
                    height: canvas.1,
                });
            }
        }
        if background_info.has_audio {
            attach_audio_mix_branch(
                &pipeline,
                audio_mixer
                    .as_ref()
                    .expect("has_any_audio guarantees a mixer"),
                &background_decodebin,
                background_clip.map_or(0.0, |clip| clip.gain_db),
            )?;
        }

        let mut branches = vec![background_decodebin];
        let mut matte_branches: Vec<(usize, gst::Element, f64)> = Vec::new();
        if let Some(matte) = background_matte {
            // Unreachable in practice — matte compositing is gated to overlay branches only
            // (see `build_composite_branch`'s doc comment) — but kept for completeness rather
            // than silently dropping a matte decodebin if that gate ever changes.
            matte_branches.push((0, matte, background_clip.map_or(0.0, |c| c.source_in_secs)));
        }
        let mut blend_branches: Vec<(gst_app::AppSink, BlendMode, u32)> = Vec::new();
        for (i, ((path, clip), info)) in overlays.iter().zip(&overlay_infos).enumerate() {
            let resolution = info.resolution;
            let zorder = (i + 1) as u32;
            let (decodebin, matte, shake_handle, mask_appsrc, blend_appsink) =
                build_composite_branch(
                    &pipeline,
                    &compositor,
                    canvas,
                    CompositeBranch {
                        path,
                        clip: Some(clip),
                        resolution,
                        hardware_decode,
                        is_overlay: true,
                        zorder,
                    },
                )?;
            if let Some(resolution) = resolution {
                clip_resolutions.insert(clip.id, resolution);
            }
            if let Some(shake_handle) = shake_handle {
                shake_handles.insert(clip.id, shake_handle);
            }
            if let Some(appsrc) = mask_appsrc {
                let (width, height) = resolution.unwrap_or(canvas);
                mask_shape_branches.push(MaskShapeBranch {
                    clip_id: clip.id,
                    appsrc,
                    width,
                    height,
                });
            }
            if let Some(appsink) = blend_appsink {
                blend_branches.push((appsink, clip.blend_mode, zorder));
            }
            if info.has_audio {
                attach_audio_mix_branch(
                    &pipeline,
                    audio_mixer
                        .as_ref()
                        .expect("has_any_audio guarantees a mixer"),
                    &decodebin,
                    clip.gain_db,
                )?;
            }
            let branch_index = branches.len();
            branches.push(decodebin);
            if let Some(matte) = matte {
                matte_branches.push((branch_index, matte, clip.source_in_secs));
            }
        }

        // Audio-only timeline tracks have no compositor branch, but still need their own
        // independently seekable decode source feeding the same mixer as embedded video audio.
        for ((path, clip), info) in audio_overlays.iter().zip(&audio_infos) {
            let uri =
                gst::glib::filename_to_uri(path, None).map_err(PreviewError::UriConversion)?;
            let decodebin = build_uri_decodebin(uri.as_str(), hardware_decode)?;
            pipeline
                .add(&decodebin)
                .map_err(PreviewError::Compositing)?;
            if info.has_audio {
                attach_audio_mix_branch(
                    &pipeline,
                    audio_mixer
                        .as_ref()
                        .expect("has_any_audio guarantees a mixer"),
                    &decodebin,
                    clip.gain_db,
                )?;
            }
            branches.push(decodebin);
        }

        let mut next_zorder = (overlays.len() + 1) as u32;
        let mut text_overlay_branches = Vec::with_capacity(text_overlays.len());
        for (clip, local_time_secs) in text_overlays {
            let rgba = crate::overlay_render::render_text_clip_rgba(
                clip,
                canvas.0,
                canvas.1,
                *local_time_secs,
            );
            let appsrc = build_static_overlay_branch(
                &pipeline,
                &compositor,
                canvas,
                rgba,
                next_zorder,
                true,
            )?;
            text_overlay_branches.push(TextOverlayBranch {
                clip_id: clip.id,
                appsrc,
                active_word_index: crate::overlay_render::active_highlight_word_index(
                    clip,
                    *local_time_secs,
                ),
                canvas_width: canvas.0,
                canvas_height: canvas.1,
            });
            next_zorder += 1;
        }
        let mut shape_overlay_branches = Vec::with_capacity(shape_overlays.len());
        for clip in shape_overlays {
            let rgba = crate::overlay_render::render_shape_clip_rgba(clip, canvas.0, canvas.1);
            let appsrc = build_static_overlay_branch(
                &pipeline,
                &compositor,
                canvas,
                rgba,
                next_zorder,
                false,
            )?;
            shape_overlay_branches.push(ShapeOverlayBranch {
                clip_id: clip.id,
                appsrc,
                canvas_width: canvas.0,
                canvas_height: canvas.1,
            });
            next_zorder += 1;
        }

        // Unlike [`Self::open`]'s playbin-managed audio-sink property (a one-line swap-and-
        // retry on failure), this pipeline's audio sink is already linked into a manually-built
        // chain by this point — no cheap retry-with-fakesink available here, so a missing/broken
        // audio device fails this whole `open_composited` call, same as any other setup failure
        // in this function (a probe failing, a missing background video, etc.).
        blend_branches.sort_by_key(|&(_, _, zorder)| zorder);

        let pipeline = pipeline.upcast::<gst::Element>();
        pipeline
            .set_state(gst::State::Paused)
            .map_err(PreviewError::StateChange)?;
        let (result, _current, _pending) = pipeline.state(gst::ClockTime::from_seconds(5));
        result.map_err(PreviewError::StateChange)?;

        Ok(Self {
            pipeline,
            video_sink,
            branches,
            matte_branches,
            text_overlay_branches,
            shape_overlay_branches,
            mask_shape_branches,
            audio_level,
            clip_resolutions,
            shake_handles,
            blend_branches,
        })
    }

    /// Seeks a [`Self::open_composited`] pipeline's branches independently — `offsets[i]` is
    /// seconds within branch `i`'s own source file (background first, then `overlays` in the
    /// order [`Self::open_composited`] was given), not a shared timeline position, and
    /// `rates[i]` is that branch's own `ClipInstance::speed_factor` (missing/shorter than
    /// `offsets` defaults to `1.0`) — each branch is seeked independently via
    /// `Element::seek` (not `seek_simple`) sent directly to that branch's own `uridecodebin`,
    /// same technique [`Self::seek_with_rate`] uses pipeline-wide for the single-clip path.
    /// Seeking one element's own upstream segment doesn't affect any other branch's rate — this
    /// closes the gap [`Self::open_composited`]'s doc comment used to describe as "every branch
    /// plays at a uniform rate `1.0`". A no-op for any branch beyond `offsets`' length; extra
    /// offsets/rates past [`Self::branches`]' length are ignored. Returns the first branch's
    /// seek error, if any, after attempting every branch (partial application is preferable to
    /// leaving some branches on their old offset/rate with no indication which).
    ///
    /// Any [`Self::matte_branches`] entry tied to a branch that got seeked here is seeked too,
    /// at that same branch's own rate, to that branch's own offset minus its `source_in_secs`
    /// (the matte's own 0-based clip timeline — see [`Self::matte_branches`]' doc comment) —
    /// the caller never passes a separate offset or rate for it.
    pub fn seek_composited(&self, offsets: &[f64], rates: &[f64]) -> Result<(), PreviewError> {
        let mut first_err = None;
        for (i, (branch, &offset)) in self.branches.iter().zip(offsets).enumerate() {
            let rate = rates.get(i).copied().unwrap_or(1.0).max(0.01);
            if let Err(e) = branch.seek(
                rate,
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::SeekType::Set,
                gst::ClockTime::from_seconds_f64(offset.max(0.0)),
                gst::SeekType::None,
                gst::ClockTime::NONE,
            ) {
                first_err.get_or_insert(PreviewError::Seek(e));
            }
        }
        for &(branch_index, ref matte, source_in_secs) in &self.matte_branches {
            let Some(&offset) = offsets.get(branch_index) else {
                continue;
            };
            let rate = rates.get(branch_index).copied().unwrap_or(1.0).max(0.01);
            let matte_offset = (offset - source_in_secs).max(0.0);
            if let Err(e) = matte.seek(
                rate,
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::SeekType::Set,
                gst::ClockTime::from_seconds_f64(matte_offset),
                gst::SeekType::None,
                gst::ClockTime::NONE,
            ) {
                first_err.get_or_insert(PreviewError::Seek(e));
            }
        }
        match first_err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Re-rasterizes and replaces only text branches whose active word changed. The branch set
    /// must still match the one passed to [`Self::open_composited`]; a mismatch returns `Ok(0)`
    /// because the UI will rebuild the whole pipeline on its next `ensure_preview_loaded` pass.
    /// Returns the number of full-canvas buffers replaced, allowing integration tests and
    /// callers to verify that repeated UI frames inside one word stay allocation/upload-free.
    pub fn update_text_overlays(
        &mut self,
        text_overlays: &[(&TextClip, f64)],
    ) -> Result<usize, PreviewError> {
        if text_overlays.len() != self.text_overlay_branches.len()
            || text_overlays
                .iter()
                .zip(&self.text_overlay_branches)
                .any(|((clip, _), branch)| clip.id != branch.clip_id)
        {
            return Ok(0);
        }

        let mut updated = 0;
        for ((clip, local_time_secs), branch) in
            text_overlays.iter().zip(&mut self.text_overlay_branches)
        {
            let active_word_index =
                crate::overlay_render::active_highlight_word_index(clip, *local_time_secs);
            if active_word_index == branch.active_word_index {
                continue;
            }
            let rgba = crate::overlay_render::render_text_clip_rgba(
                clip,
                branch.canvas_width,
                branch.canvas_height,
                *local_time_secs,
            );
            push_rgba_overlay_buffer(&branch.appsrc, rgba)?;
            branch.active_word_index = active_word_index;
            updated += 1;
        }
        Ok(updated)
    }

    /// Re-rasterizes and replaces exactly one text branch's buffer, keyed by `clip.id` — unlike
    /// [`Self::update_text_overlays`], which skips a clip whose active highlighted word hasn't
    /// changed (the scrubbing/playback path), this always redraws: the caller here already
    /// knows some other property (text/font/color/background/position/highlight) changed and
    /// wants the new look reflected immediately, e.g. dragging a properties-panel slider. Still
    /// far cheaper than a full pipeline reopen — one small `appsrc` buffer push instead of
    /// tearing down and rebuilding the whole compositor graph (background decoder, every other
    /// branch) on every dragged frame. `Ok(false)` if no branch is currently open for this clip
    /// id — the caller falls back to a full reopen in that case.
    pub fn refresh_text_overlay(
        &mut self,
        clip: &TextClip,
        local_time_secs: f64,
    ) -> Result<bool, PreviewError> {
        let Some(branch) = self
            .text_overlay_branches
            .iter_mut()
            .find(|branch| branch.clip_id == clip.id)
        else {
            return Ok(false);
        };
        let rgba = crate::overlay_render::render_text_clip_rgba(
            clip,
            branch.canvas_width,
            branch.canvas_height,
            local_time_secs,
        );
        push_rgba_overlay_buffer(&branch.appsrc, rgba)?;
        branch.active_word_index =
            crate::overlay_render::active_highlight_word_index(clip, local_time_secs);
        Ok(true)
    }

    /// Re-rasterizes and replaces exactly one shape overlay branch's buffer, keyed by
    /// `clip.id` — same role [`Self::refresh_text_overlay`] has for text clips, applied to
    /// [`ShapeClip`] (kind/color/position/size/rotation/stroke edits) instead. Unlike text
    /// clips, a shape clip's rasterization has no time-dependent input (no word-highlight
    /// animation), so there's no analogue to `update_text_overlays`' per-frame scrubbing path —
    /// every call here is the "some property changed" case. `Ok(false)` if no branch is
    /// currently open for this clip id — the caller falls back to a full reopen in that case.
    pub fn refresh_shape_overlay(&mut self, clip: &ShapeClip) -> Result<bool, PreviewError> {
        let Some(branch) = self
            .shape_overlay_branches
            .iter_mut()
            .find(|branch| branch.clip_id == clip.id)
        else {
            return Ok(false);
        };
        let rgba = crate::overlay_render::render_shape_clip_rgba(
            clip,
            branch.canvas_width,
            branch.canvas_height,
        );
        push_rgba_overlay_buffer(&branch.appsrc, rgba)?;
        Ok(true)
    }

    pub fn play(&self) -> Result<(), PreviewError> {
        self.pipeline
            .set_state(gst::State::Playing)
            .map(|_| ())
            .map_err(PreviewError::StateChange)
    }

    pub fn pause(&self) -> Result<(), PreviewError> {
        self.pipeline
            .set_state(gst::State::Paused)
            .map(|_| ())
            .map_err(PreviewError::StateChange)
    }

    /// Seeks to `position_secs`, flushing buffered data for an immediate jump to the nearest
    /// key unit. Equivalent to [`Self::seek_with_rate`] at rate `1.0`.
    pub fn seek(&self, position_secs: f64) -> Result<(), PreviewError> {
        self.pipeline
            .seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::ClockTime::from_seconds_f64(position_secs.max(0.0)),
            )
            .map_err(PreviewError::Seek)
    }

    /// Seeks to `position_secs` like [`Self::seek`], and additionally sets the pipeline's
    /// playback rate to `rate` — GStreamer's own rate-seek mechanism, not a filter element, so
    /// it covers audio too and stays in effect across a later plain `play()`/`pause()` until
    /// the next seek changes it again. Used to make preview playback/scrubbing honor
    /// [`crate::timeline::ClipInstance::speed_factor`] the same way export's `setpts` scaling
    /// does — without this, a sped-up/slowed-down clip would still visually play at 1x in the
    /// preview even though `ui`'s position math already accounts for speed when converting
    /// between timeline and source time.
    pub fn seek_with_rate(&self, position_secs: f64, rate: f64) -> Result<(), PreviewError> {
        self.pipeline
            .seek(
                rate,
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT,
                gst::SeekType::Set,
                gst::ClockTime::from_seconds_f64(position_secs.max(0.0)),
                gst::SeekType::None,
                gst::ClockTime::NONE,
            )
            .map_err(PreviewError::Seek)
    }

    /// Current playhead position, if the pipeline can report one (usually only once it's
    /// reached at least `Paused` and finished prerolling).
    pub fn position_secs(&self) -> Option<f64> {
        self.pipeline
            .query_position::<gst::ClockTime>()
            .map(|t| t.seconds_f64())
    }

    /// Total duration of the loaded file, if known.
    pub fn duration_secs(&self) -> Option<f64> {
        self.pipeline
            .query_duration::<gst::ClockTime>()
            .map(|t| t.seconds_f64())
    }

    /// The most recently probed audio buffer's level — per `spec/ROADMAP.md` P4 item 30. Stays
    /// at its silent default (`AudioLevel::default()`) until playback has actually pushed at
    /// least one buffer through the audio sink (e.g. while merely paused/prerolled with no
    /// audio track, or before the first buffer after a seek).
    pub fn current_audio_level(&self) -> AudioLevel {
        self.audio_level.lock().map(|l| *l).unwrap_or_default()
    }

    /// Pushes a live brightness/contrast/effective-saturation update to `clip_id`'s already-
    /// built `videobalance` element (P1 item 3's remaining live-preview-update gap,
    /// `spec/matrix/performance.md`), if one exists in the running pipeline right now — avoids
    /// a full pipeline reopen for the single most common color-grading tweak, the one this
    /// gap's own investigation singled out as a real, confirmed hot-path violation before this
    /// method existed. `effective_saturation` is the caller's job to compute (`0.0` when
    /// `ColorFilter::BlackAndWhite` overrides it, same as [`build_video_filter_bin`] itself
    /// does) — this method is a plain property push, it doesn't re-derive that rule.
    ///
    /// Returns `false` (a no-op, not an error) if no such element exists: either
    /// [`build_video_filter_bin`] was never given this clip at all (a probe/audio-only
    /// pipeline, or an unresolvable overlay), or brightness/contrast/saturation were *all*
    /// neutral when the pipeline was last built — the element is only created once at least one
    /// of the three is non-default, and creating it now would mean restructuring the running
    /// filter graph, not just setting a property, which this method deliberately doesn't
    /// attempt. The caller's existing "next incidental reopen picks up the new value" fallback
    /// (unchanged, was already the *only* behavior before this method existed) still applies
    /// whenever this returns `false`.
    pub fn set_live_balance(
        &self,
        clip_id: u64,
        brightness: f32,
        contrast: f32,
        effective_saturation: f32,
    ) -> bool {
        let Some(bin) = self.pipeline.dynamic_cast_ref::<gst::Bin>() else {
            return false;
        };
        let Some(balance) = bin.by_name(&live_balance_element_name(clip_id)) else {
            return false;
        };
        balance.set_property("brightness", brightness as f64);
        balance.set_property("contrast", contrast as f64);
        balance.set_property("saturation", effective_saturation as f64);
        true
    }

    /// Pushes a live blur/sharpen update to `clip_id`'s already-built `gaussianblur` element
    /// (P1 item 3's remaining live-preview-update gap), if one exists in the running pipeline
    /// right now — same shape and same caveat [`Preview::set_live_balance`] has, just for the
    /// single derived `net_sigma` `gaussianblur` covers instead of three separate properties.
    /// `net_sigma` is the caller's job to compute (`blur_intensity * 4.0 - sharpen * 4.0`, same
    /// formula [`build_video_filter_bin`] itself uses) — this method is a plain property push.
    ///
    /// Returns `false` (a no-op, not an error) if no such element exists: either this clip was
    /// never given a video-filter pipeline at all, or `blur_intensity`/`sharpen` were both zero
    /// (`net_sigma == 0.0`) when the pipeline was last built — the element is only created once
    /// `net_sigma` is non-zero, and creating it now would mean restructuring the running filter
    /// graph, not just setting a property, which this method deliberately doesn't attempt. The
    /// caller's existing "next incidental reopen picks up the new value" fallback still applies
    /// whenever this returns `false`.
    pub fn set_live_blur(&self, clip_id: u64, net_sigma: f64) -> bool {
        let Some(bin) = self.pipeline.dynamic_cast_ref::<gst::Bin>() else {
            return false;
        };
        let Some(blur) = bin.by_name(&live_blur_element_name(clip_id)) else {
            return false;
        };
        blur.set_property("sigma", net_sigma);
        true
    }

    /// Pushes a live chroma-key color/tolerance update to `clip_id`'s already-built `alpha`
    /// element (P1 item 3's remaining live-preview-update gap), if one exists in the running
    /// pipeline right now — same shape [`Preview::set_live_balance`]/[`Preview::set_live_blur`]
    /// have. Unlike those two, this element is only ever built for a composited-overlay branch
    /// gated on `chroma_key_enabled` (a plain on/off toggle, not a gradually-approached
    /// intensity) — toggling chroma key on for the first time still needs the existing
    /// incidental-reopen fallback to build the element at all; this only covers color/tolerance
    /// edits made *after* that, while it's already enabled.
    ///
    /// Returns `false` (a no-op, not an error) if no such element exists: `chroma_key_enabled`
    /// was `false` (or this isn't an overlay branch at all) when the pipeline was last built.
    pub fn set_live_chroma_key(&self, clip_id: u64, color: [u8; 3], tolerance: f32) -> bool {
        let Some(bin) = self.pipeline.dynamic_cast_ref::<gst::Bin>() else {
            return false;
        };
        let Some(alpha) = bin.by_name(&live_chroma_key_element_name(clip_id)) else {
            return false;
        };
        let [r, g, b] = color;
        let sensitivity = (tolerance.clamp(0.0, 1.0) * 128.0).round() as u32;
        alpha.set_property("target-r", r as u32);
        alpha.set_property("target-g", g as u32);
        alpha.set_property("target-b", b as u32);
        alpha.set_property("black-sensitivity", sensitivity);
        alpha.set_property("white-sensitivity", sensitivity);
        true
    }

    /// Pushes a live crop-rectangle update to `clip_id`'s already-built static-crop `videocrop`
    /// element (P1 item 3's remaining live-preview-update gap), if one exists in the running
    /// pipeline right now — same shape [`Self::set_live_balance`]/[`Self::set_live_blur`] have.
    /// `crop_x`/`crop_y`/`crop_w`/`crop_h` are [`ClipInstance`]'s own normalized `0.0..=1.0`
    /// fractions — this method redoes the fraction-to-pixel conversion
    /// [`build_video_filter_bin`] itself does, using [`Self::clip_resolutions`]'s cached decoded
    /// size for `clip_id`, so the caller (`ui`'s `App`) never needs to know or track pixel
    /// dimensions.
    ///
    /// Returns `false` (a no-op, not an error) if no such element exists: `clip_id`'s resolution
    /// was never cached (no branch was built for it at all), or `ClipInstance::is_cropped()` was
    /// `false` when the pipeline was last built — the element is only created once cropping is
    /// active, and creating it now would mean restructuring the running filter graph, not just
    /// setting a property. The caller's existing "next incidental reopen picks up the new value"
    /// fallback still applies whenever this returns `false`.
    pub fn set_live_crop(
        &self,
        clip_id: u64,
        crop_x: f32,
        crop_y: f32,
        crop_w: f32,
        crop_h: f32,
    ) -> bool {
        let Some(&(width, height)) = self.clip_resolutions.get(&clip_id) else {
            return false;
        };
        let Some(bin) = self.pipeline.dynamic_cast_ref::<gst::Bin>() else {
            return false;
        };
        let Some(crop) = bin.by_name(&live_crop_element_name(clip_id)) else {
            return false;
        };
        let (width, height) = (width as f32, height as f32);
        let left = (crop_x * width).round().max(0.0) as i32;
        let top = (crop_y * height).round().max(0.0) as i32;
        let right = ((1.0 - crop_x - crop_w) * width).round().max(0.0) as i32;
        let bottom = ((1.0 - crop_y - crop_h) * height).round().max(0.0) as i32;
        crop.set_property("left", left);
        crop.set_property("top", top);
        crop.set_property("right", right);
        crop.set_property("bottom", bottom);
        true
    }

    /// Pushes a live pixelize-intensity update to `clip_id`'s already-built pixelize block
    /// (P1 item 3's remaining live-preview-update gap) by renegotiating its downscale
    /// `capsfilter`'s caps to the new block size's dimensions — the one live-updatable case
    /// among the "structural" effects this gap's own investigation flagged, since `capsfilter`
    /// caps changes propagate as a live renegotiation (unlike, say, `pixelize`'s own block-size
    /// *count* changing what elements exist at all). The upscale stage's own caps stay pinned
    /// to the branch's fixed output resolution regardless of block size, so only the downscale
    /// `capsfilter` needs touching.
    ///
    /// Returns `false` (a no-op, not an error) under the same conditions [`Self::set_live_crop`]
    /// has: no cached resolution for `clip_id`, or `pixelize_intensity` was `0.0` (no element
    /// built) when the pipeline was last built.
    pub fn set_live_pixelize(&self, clip_id: u64, pixelize_intensity: f32) -> bool {
        let Some(&(width, height)) = self.clip_resolutions.get(&clip_id) else {
            return false;
        };
        let Some(bin) = self.pipeline.dynamic_cast_ref::<gst::Bin>() else {
            return false;
        };
        let Some(small_caps) = bin.by_name(&live_pixelize_caps_name(clip_id)) else {
            return false;
        };
        // Same block-size formula build_video_filter_bin itself uses — see its own doc comment.
        let block = (2.0 + pixelize_intensity * 48.0).round().max(1.0) as u32;
        let small_w = (width / block).max(1) as i32;
        let small_h = (height / block).max(1) as i32;
        small_caps.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("width", small_w)
                .field("height", small_h)
                .build(),
        );
        true
    }

    /// Pushes a live shake-intensity update to `clip_id`'s already-built shake block (P1 item
    /// 3's remaining live-preview-update gap) by writing new pixel margins directly into its
    /// [`ShakeMarginHandle`]'s shared atomics — the buffer probe [`build_video_filter_bin`]
    /// installed reads these on every frame already, so no pipeline restructuring or even a
    /// GStreamer property push is needed, just updating the numbers.
    ///
    /// Returns `false` (a no-op, not an error) if no handle exists for `clip_id`:
    /// `shake_intensity` was `0.0` (no shake block built at all) when the pipeline was last
    /// built. The caller's existing "next incidental reopen picks up the new value" fallback
    /// still applies whenever this returns `false` — including a *first* enable, same as
    /// [`Self::set_live_chroma_key`]'s own "first enable still needs a reopen" caveat.
    pub fn set_live_shake(&self, clip_id: u64, shake_intensity: f32) -> bool {
        let Some(handle) = self.shake_handles.get(&clip_id) else {
            return false;
        };
        let margin = shake_intensity * 0.08_f32;
        let (total_trim_w, total_trim_h) =
            shake_margin_pixels(handle.resolution.0, handle.resolution.1, margin);
        handle
            .total_trim_w
            .store(total_trim_w, std::sync::atomic::Ordering::Relaxed);
        handle
            .total_trim_h
            .store(total_trim_h, std::sync::atomic::Ordering::Relaxed);
        true
    }

    /// Pushes a freshly rasterized mask buffer to `clip_id`'s already-built mask branch (P1
    /// item 3's remaining live-preview-update gap) via its replaceable
    /// `appsrc ! imagefreeze(allow-replace=true)` pair — same mechanism
    /// [`Self::refresh_text_overlay`] uses for text content edits, just always redrawing (mask
    /// shape has no keyframes/highlight-state to compare against, so there's no cheap
    /// "did anything actually change" check worth doing before the caller even gets here).
    ///
    /// Returns `false` (a no-op, not an error) if no mask branch exists for `clip_id`:
    /// `ClipInstance::is_masked()` was `false` when the pipeline was last built (a plain on/off
    /// gate, same as chroma key) — the first enable still needs the existing incidental-reopen
    /// fallback to build the branch at all; this only covers shape/corner-radius edits made
    /// *after* that, while it's already enabled. Also declines
    /// [`MaskShape::None`][crate::timeline::MaskShape::None]: turning masking back *off* isn't
    /// covered either, same "on/off toggle needs the reopen fallback" contract — pushing
    /// [`crate::overlay_render::render_mask_shape_gray8`]'s own `MaskShape::None` output live
    /// would be actively wrong here, since that function returns an all-zero (fully *masked*,
    /// not unmasked) buffer for `None`, correct only because every other caller already gates
    /// on [`crate::timeline::ClipInstance::is_masked`] first and never reaches it. Without this
    /// guard, dragging a live-previewed clip's mask shape back to "None" would make the whole
    /// overlay vanish instead of removing the mask.
    pub fn set_live_mask(
        &self,
        clip_id: u64,
        mask_shape: crate::timeline::MaskShape,
        mask_corner_radius: f32,
    ) -> Result<bool, PreviewError> {
        if mask_shape == crate::timeline::MaskShape::None {
            return Ok(false);
        }
        let Some(branch) = self
            .mask_shape_branches
            .iter()
            .find(|branch| branch.clip_id == clip_id)
        else {
            return Ok(false);
        };
        let mask_gray = crate::overlay_render::render_mask_shape_gray8(
            mask_shape,
            mask_corner_radius,
            branch.width,
            branch.height,
        );
        let mut buffer =
            gst::Buffer::with_size(mask_gray.len()).map_err(PreviewError::Compositing)?;
        {
            let buffer_mut = buffer.get_mut().expect("freshly created, uniquely owned");
            buffer_mut.set_pts(gst::ClockTime::ZERO);
            let mut map = buffer_mut
                .map_writable()
                .map_err(|_| PreviewError::PushBuffer(gst::FlowError::Error))?;
            map.copy_from_slice(&mask_gray);
        }
        branch
            .appsrc
            .push_buffer(buffer)
            .map_err(PreviewError::PushBuffer)?;
        Ok(true)
    }

    /// The most recent video frame the pipeline has decoded, as packed RGBA. `None` if
    /// nothing has arrived yet (e.g. called before the first preroll completes), the file
    /// has no video stream, or (while [`Self::play`]ing) no new sample has been decoded since
    /// the last call.
    ///
    /// Tries the prerolled frame first (what's available right after [`Self::open`] or a
    /// [`Self::seek`] while paused), then falls back to the latest live sample (what arrives
    /// during [`Self::play`]) — covers both without the caller needing to know which state
    /// produced the frame. Non-blocking (zero timeout on both pulls) so callers can poll this
    /// once per UI frame without risking a multi-frame stall waiting for a sample that isn't
    /// ready yet — safe since a decoded-but-not-yet-fetched frame just gets picked up on the
    /// next call instead.
    pub fn current_frame(&self) -> Option<VideoFrame> {
        let timeout = gst::ClockTime::ZERO;
        let sample = self
            .video_sink
            .try_pull_preroll(timeout)
            .or_else(|| self.video_sink.try_pull_sample(timeout))?;

        let buffer = sample.buffer()?;
        let caps = sample.caps()?;
        let info = gst_video::VideoInfo::from_caps(caps).ok()?;
        let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(buffer, &info).ok()?;
        let plane = frame.plane_data(0).ok()?;

        let width = info.width();
        let height = info.height();
        let mut rgba = plane.to_vec();

        // Composites every blend-mode branch's own frame on top, in ascending zorder -- see
        // `Self::blend_branches`' doc comment for the documented scope limit (always on top of
        // the whole stack, ignores position). A branch with no frame ready yet, or whose decoded
        // size doesn't match this frame's canvas (still settling right after a seek/open),
        // contributes nothing this call rather than corrupting or panicking -- same "degrade
        // rather than abort" posture the rest of this module uses.
        for (appsink, mode, _zorder) in &self.blend_branches {
            let Some(layer_sample) = appsink
                .try_pull_preroll(timeout)
                .or_else(|| appsink.try_pull_sample(timeout))
            else {
                continue;
            };
            let Some(layer_buffer) = layer_sample.buffer() else {
                continue;
            };
            let Some(layer_caps) = layer_sample.caps() else {
                continue;
            };
            let Ok(layer_info) = gst_video::VideoInfo::from_caps(layer_caps) else {
                continue;
            };
            if layer_info.width() != width || layer_info.height() != height {
                continue;
            }
            let Ok(layer_frame) =
                gst_video::VideoFrameRef::from_buffer_ref_readable(layer_buffer, &layer_info)
            else {
                continue;
            };
            let Ok(layer_plane) = layer_frame.plane_data(0) else {
                continue;
            };
            blend_rgba_frame(&mut rgba, layer_plane, *mode);
        }

        Some(VideoFrame {
            width,
            height,
            rgba,
        })
    }
}

/// Applies a blend-mode result as the top RGBA layer over an already composited bottom pixel.
/// The dedicated blend-mode appsink bypasses GStreamer's `compositor`, so its output alpha must
/// be applied here instead of being discarded. The normal composited output always has an opaque
/// background-video branch, so only the top alpha participates in this fast path.
fn blend_over_channel(mode: BlendMode, top: u8, bottom: u8, top_alpha: u8) -> u8 {
    let blended = blend_channel(mode, top, bottom) as u16;
    let alpha = top_alpha as u16;
    ((blended * alpha + bottom as u16 * (u16::from(u8::MAX) - alpha) + 127) / 255) as u8
}

/// Blends one packed RGBA frame over another. This is deliberately independent of GStreamer so
/// the pixel contract can be tested with deterministic synthetic buffers.
///
/// Returns `false` without mutating `base` when either buffer is not complete packed RGBA data
/// or their sizes differ.
fn blend_rgba_frame(base: &mut [u8], layer: &[u8], mode: BlendMode) -> bool {
    if base.len() != layer.len() || !base.len().is_multiple_of(4) {
        return false;
    }

    for (base_px, layer_px) in base
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(layer.as_chunks::<4>().0)
    {
        if layer_px[3] == 0 {
            continue;
        }
        for channel in 0..3 {
            base_px[channel] =
                blend_over_channel(mode, layer_px[channel], base_px[channel], layer_px[3]);
        }
    }
    true
}

impl Drop for Preview {
    fn drop(&mut self) {
        // Best-effort: nothing more to do if this fails, we're already tearing down.
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

#[cfg(test)]
#[path = "preview/preview_test.rs"]
mod tests;
