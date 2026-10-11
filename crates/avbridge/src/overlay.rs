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
use std::path::{Path, PathBuf};

use crate::ffi::{
    avbridge_apply_privacy_blur, avbridge_apply_shape_overlays, avbridge_apply_text_overlays,
    avbridge_free_string, avbridge_privacy_blur_filter, avbridge_shape_overlay_filter,
    avbridge_text_overlay_filter, RawShapeSegment, RawTextSegment,
};

/// One pre-rasterized text overlay to composite over an exported video. The core crate writes
/// its semantic text segments to temporary full-canvas RGBA PNGs immediately before this pass,
/// keeping font/background rendering identical to preview and temporary paths out of persisted
/// export jobs.
#[derive(Debug, Clone)]
pub struct TextOverlaySegment {
    /// When the overlay becomes visible, in seconds from the start of the exported video.
    pub start_secs: f64,
    /// How long the overlay is visible, in seconds.
    pub duration_secs: f64,
    /// Full-canvas transparent PNG containing the already-rasterized text/background.
    pub overlay_path: PathBuf,
    /// Complete `geq`-expression-language fragment (built by
    /// `avcore::keyframe::text_opacity_alpha_expr`) multiplied against the PNG's own alpha
    /// channel — empty means "always fully opaque", the same visibility this overlay always had
    /// before opacity keyframes existed.
    pub opacity_keyframe_expr: String,
    /// Complete `overlay`-expression-language fragments (built by
    /// `avcore::keyframe::text_position_offset_expr`) added to the `overlay` filter's `x`/`y`
    /// position — empty means "no offset", the same `x=0:y=0` placement this overlay always had
    /// before position keyframes existed.
    pub position_keyframe_expr_x: String,
    pub position_keyframe_expr_y: String,
    /// Complete `geq`-expression-language fragments (built by
    /// `avcore::keyframe::text_scale_sample_exprs`) used as the (X,Y) coordinates a geq stage
    /// samples the raster's own pixels at, remapping it around its own baked position anchor —
    /// empty (either one) means "no remap", the same pixel-for-pixel raster this overlay always
    /// had before scale keyframes existed. An inverse-sample zoom, not a literal `scale` filter
    /// — see [`apply_text_overlays`]'s doc comment for why.
    pub scale_keyframe_expr_x: String,
    pub scale_keyframe_expr_y: String,
    /// Complete `geq`-expression-language fragments (built by
    /// `avcore::keyframe::text_rotation_sample_exprs`) used as the (X,Y) coordinates a geq stage
    /// samples the raster's own pixels at, remapping it around its own baked position anchor —
    /// empty (either one) means "no remap", the same pixel-for-pixel raster this overlay always
    /// had before rotation keyframes existed.
    pub rotation_keyframe_expr_x: String,
    pub rotation_keyframe_expr_y: String,
}

/// What [`apply_text_overlays`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum TextOverlayError {
    /// A path contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to allocate output context")]
    AllocOutput,
    #[error("failed to build the text-overlay filter graph")]
    FilterGraph,
    #[error("decode/filter/encode pipeline failed mid-stream")]
    Pipeline,
    #[error("unknown text overlay status code: {0}")]
    Unknown(c_int),
}

/// Opens `in_path` (the already-rendered export), overlays each segment's full-canvas RGBA PNG
/// during its timeline window, and writes the result to `out_path`. The caller is responsible
/// for renaming `out_path` over `in_path` when this returns `Ok(())`.
///
/// `canvas_width`/`canvas_height`/`fps_num`/`fps_den` must match the rendered file's canvas
/// dimensions and frame rate so the output encoder matches the existing render.
///
/// A no-op (returns `Ok(())` immediately without calling the C function) when `segments` is
/// empty — the caller can skip creating `out_path` in that case.
///
/// `TextOverlaySegment::scale_keyframe_expr_x`/`_y` drives an inverse-sample geq remap rather
/// than a literal `scale` filter with `eval=frame`: letting `scale` renegotiate its output
/// frame size every frame reliably corrupted the heap in a real export elsewhere in this
/// codebase (`timeline_export.c`'s zoom-transition case, see `CLAUDE.md`) — the geq approach
/// keeps the output frame size fixed and only changes what each pixel samples, avoiding that
/// failure mode entirely.
pub fn apply_text_overlays(
    in_path: &Path,
    out_path: &Path,
    segments: &[TextOverlaySegment],
    canvas_width: u32,
    canvas_height: u32,
    fps_num: u32,
    fps_den: u32,
) -> Result<(), TextOverlayError> {
    if segments.is_empty() {
        return Ok(());
    }

    let c_in = CString::new(in_path.to_string_lossy().as_bytes())
        .map_err(TextOverlayError::InvalidPath)?;
    let c_out = CString::new(out_path.to_string_lossy().as_bytes())
        .map_err(TextOverlayError::InvalidPath)?;

    let raw = RawTextSegments::new(segments)?;
    let raw_segments = &raw.raw;

    // SAFETY: all pointers (c_in, c_out, raw_segments' path/expr pointers, owned by `raw`)
    // are valid NUL-terminated C strings held alive for the full duration of this call.
    // raw_segments is a contiguous Vec<RawTextSegment> with segment_count entries, never
    // mutated during the call.
    let status = unsafe {
        avbridge_apply_text_overlays(
            c_in.as_ptr(),
            c_out.as_ptr(),
            raw_segments.as_ptr(),
            raw_segments.len() as c_int,
            canvas_width as c_int,
            canvas_height as c_int,
            fps_num as c_int,
            fps_den as c_int,
        )
    };

    apply_text_overlays_status(status)
}

/// The C view of some [`TextOverlaySegment`]s, with the strings it points into kept alive beside
/// it.
struct RawTextSegments {
    raw: Vec<RawTextSegment>,
    _paths: Vec<CString>,
    _exprs: Vec<(
        CString,
        CString,
        CString,
        CString,
        CString,
        CString,
        CString,
    )>,
}

impl RawTextSegments {
    fn new(segments: &[TextOverlaySegment]) -> Result<Self, TextOverlayError> {
        let mut c_paths: Vec<CString> = Vec::with_capacity(segments.len());
        for seg in segments {
            c_paths.push(
                CString::new(seg.overlay_path.to_string_lossy().as_bytes())
                    .map_err(TextOverlayError::InvalidPath)?,
            );
        }
        // One CString each for opacity/position-x/position-y/scale-x/scale-y/rotation-x/rotation-y
        // per segment, held in one Vec of tuples (rather than seven parallel Vecs zipped together)
        // so building raw_segments below stays readable as the field count grows.
        let exprs: Vec<(
            CString,
            CString,
            CString,
            CString,
            CString,
            CString,
            CString,
        )> = segments
            .iter()
            .map(|seg| {
                Ok::<_, TextOverlayError>((
                    CString::new(seg.opacity_keyframe_expr.as_bytes())
                        .map_err(TextOverlayError::InvalidPath)?,
                    CString::new(seg.position_keyframe_expr_x.as_bytes())
                        .map_err(TextOverlayError::InvalidPath)?,
                    CString::new(seg.position_keyframe_expr_y.as_bytes())
                        .map_err(TextOverlayError::InvalidPath)?,
                    CString::new(seg.scale_keyframe_expr_x.as_bytes())
                        .map_err(TextOverlayError::InvalidPath)?,
                    CString::new(seg.scale_keyframe_expr_y.as_bytes())
                        .map_err(TextOverlayError::InvalidPath)?,
                    CString::new(seg.rotation_keyframe_expr_x.as_bytes())
                        .map_err(TextOverlayError::InvalidPath)?,
                    CString::new(seg.rotation_keyframe_expr_y.as_bytes())
                        .map_err(TextOverlayError::InvalidPath)?,
                ))
            })
            .collect::<Result<_, _>>()?;

        let raw: Vec<RawTextSegment> = segments
            .iter()
            .zip(c_paths.iter())
            .zip(exprs.iter())
            .map(
                |((seg, path), (opacity_expr, pos_x, pos_y, scale_x, scale_y, rot_x, rot_y))| {
                    RawTextSegment {
                        start_secs: seg.start_secs,
                        duration_secs: seg.duration_secs,
                        overlay_path: path.as_ptr(),
                        opacity_keyframe_expr: opacity_expr.as_ptr(),
                        position_keyframe_expr_x: pos_x.as_ptr(),
                        position_keyframe_expr_y: pos_y.as_ptr(),
                        scale_keyframe_expr_x: scale_x.as_ptr(),
                        scale_keyframe_expr_y: scale_y.as_ptr(),
                        rotation_keyframe_expr_x: rot_x.as_ptr(),
                        rotation_keyframe_expr_y: rot_y.as_ptr(),
                    }
                },
            )
            .collect();
        Ok(Self {
            raw,
            _paths: c_paths,
            _exprs: exprs,
        })
    }
}

/// One geometric shape to composite over an exported video via `avbridge_apply_shape_overlays`.
/// `filter_desc` is a complete, ready-to-chain `geq=...` avfilter node description — see
/// `avcore::shape_render::build_shape_filter_desc`, which builds it (the actual shape geometry/
/// color/rotation math lives there, not here or in the C function this crosses into).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShapeSegment {
    pub filter_desc: String,
}

/// Same shape as [`apply_text_overlays`] (same [`TextOverlayError`] variants, same "opens
/// `in_path`, writes the composited result to `out_path`" contract), but chains each segment's
/// pre-built `geq` filter node instead of building a `drawtext` chain. A no-op (`Ok(())`
/// immediately) when `segments` is empty.
pub fn apply_shape_overlays(
    in_path: &Path,
    out_path: &Path,
    segments: &[ShapeSegment],
    canvas_width: u32,
    canvas_height: u32,
    fps_num: u32,
    fps_den: u32,
) -> Result<(), TextOverlayError> {
    if segments.is_empty() {
        return Ok(());
    }

    let c_in = CString::new(in_path.to_string_lossy().as_bytes())
        .map_err(TextOverlayError::InvalidPath)?;
    let c_out = CString::new(out_path.to_string_lossy().as_bytes())
        .map_err(TextOverlayError::InvalidPath)?;

    let mut c_descs: Vec<CString> = Vec::with_capacity(segments.len());
    for seg in segments {
        c_descs
            .push(CString::new(seg.filter_desc.as_bytes()).map_err(TextOverlayError::InvalidPath)?);
    }

    let raw_segments: Vec<RawShapeSegment> = c_descs
        .iter()
        .map(|desc| RawShapeSegment {
            filter_desc: desc.as_ptr(),
        })
        .collect();

    // SAFETY: same reasoning as apply_text_overlays — c_in/c_out/raw_segments' filter_desc
    // pointers (from c_descs) are valid NUL-terminated C strings held alive for the full call.
    let status = unsafe {
        avbridge_apply_shape_overlays(
            c_in.as_ptr(),
            c_out.as_ptr(),
            raw_segments.as_ptr(),
            raw_segments.len() as c_int,
            canvas_width as c_int,
            canvas_height as c_int,
            fps_num as c_int,
            fps_den as c_int,
        )
    };

    apply_shape_overlays_status(status)
}

/// What [`apply_privacy_blur`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum PrivacyBlurError {
    /// A path contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to allocate output context")]
    AllocOutput,
    #[error("failed to build the blur/mask filter graph")]
    FilterGraph,
    #[error("decode/filter/encode pipeline failed mid-stream")]
    Pipeline,
    /// `blur_sigma` was <= 0.0 — there is no meaningful "zero blur" for `gblur`; a caller
    /// wanting the frame untouched should skip calling this function entirely.
    #[error("blur_sigma must be positive")]
    NonPositiveSigma,
    #[error("unknown privacy-blur status code: {0}")]
    Unknown(c_int),
}

/// Opens `in_path` (an already-rendered export, same "post-process pass" contract
/// [`apply_text_overlays`]/[`apply_shape_overlays`] use), blurs (Gaussian, `blur_sigma`)
/// everywhere `matte_path`'s own per-frame luma is non-zero, and writes the result to
/// `out_path`. The caller is responsible for renaming `out_path` over `in_path` when this
/// returns `Ok(())`, same as the other two overlay functions.
///
/// `matte_path` is expected to be exactly [`encode_matte_video`]'s own output shape
/// (grayscale-as-luma, neutral chroma) — CF-09's arbitrary-object privacy-blur use case builds
/// one from `avcore::mask_propagation::rasterize_to_matte_frames`, but any matte video of that
/// shape works. Its own frame rate/duration must already match `canvas_fps_num`/`_den` — this
/// function does no timing reconciliation between the main video and the matte.
///
/// Fails with [`PrivacyBlurError::NonPositiveSigma`] immediately (without touching `out_path`)
/// if `blur_sigma <= 0.0`.
#[allow(clippy::too_many_arguments)]
pub fn apply_privacy_blur(
    in_path: &Path,
    out_path: &Path,
    matte_path: &Path,
    blur_sigma: f64,
    canvas_width: u32,
    canvas_height: u32,
    fps_num: u32,
    fps_den: u32,
) -> Result<(), PrivacyBlurError> {
    if blur_sigma <= 0.0 {
        return Err(PrivacyBlurError::NonPositiveSigma);
    }

    let c_in = CString::new(in_path.to_string_lossy().as_bytes())
        .map_err(PrivacyBlurError::InvalidPath)?;
    let c_out = CString::new(out_path.to_string_lossy().as_bytes())
        .map_err(PrivacyBlurError::InvalidPath)?;
    let c_matte = CString::new(matte_path.to_string_lossy().as_bytes())
        .map_err(PrivacyBlurError::InvalidPath)?;

    // SAFETY: c_in/c_out/c_matte are valid NUL-terminated C strings held alive for the full
    // call. privacy_blur.c frees the decoder/encoder contexts, filter graph, and output I/O on
    // every exit path.
    let status = unsafe {
        avbridge_apply_privacy_blur(
            c_in.as_ptr(),
            c_out.as_ptr(),
            c_matte.as_ptr(),
            blur_sigma,
            canvas_width as c_int,
            canvas_height as c_int,
            fps_num as c_int,
            fps_den as c_int,
        )
    };

    apply_privacy_blur_status(status)
}

/// Maps the status code the C side returned from `apply_text_overlays` to its result.
pub(crate) fn apply_text_overlays_status(status: c_int) -> Result<(), TextOverlayError> {
    match status {
        0 => Ok(()),
        1 => Err(TextOverlayError::OpenInput),
        2 => Err(TextOverlayError::AllocOutput),
        3 => Err(TextOverlayError::FilterGraph),
        4 => Err(TextOverlayError::Pipeline),
        other => Err(TextOverlayError::Unknown(other)),
    }
}

/// Maps the status code the C side returned from `apply_shape_overlays` to its result.
pub(crate) fn apply_shape_overlays_status(status: c_int) -> Result<(), TextOverlayError> {
    match status {
        0 => Ok(()),
        1 => Err(TextOverlayError::OpenInput),
        2 => Err(TextOverlayError::AllocOutput),
        3 => Err(TextOverlayError::FilterGraph),
        4 => Err(TextOverlayError::Pipeline),
        other => Err(TextOverlayError::Unknown(other)),
    }
}

/// Maps the status code the C side returned from `apply_privacy_blur` to its result.
pub(crate) fn apply_privacy_blur_status(status: c_int) -> Result<(), PrivacyBlurError> {
    match status {
        0 => Ok(()),
        1 => Err(PrivacyBlurError::OpenInput),
        2 => Err(PrivacyBlurError::AllocOutput),
        3 => Err(PrivacyBlurError::FilterGraph),
        4 => Err(PrivacyBlurError::Pipeline),
        other => Err(PrivacyBlurError::Unknown(other)),
    }
}

/// Copies out and frees a string `avbridge` returned, or `None` for NULL.
fn take_c_string(ptr: *mut std::os::raw::c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: `ptr` is a NUL-terminated string `avbridge` allocated and has not freed; it is read
    // once here and then released with the function that matches its allocator.
    let text = unsafe { std::ffi::CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { avbridge_free_string(ptr) };
    Some(text)
}

/// The avfilter graph description (input `in`, output `out`) that composites `segments` over a
/// video: what [`apply_text_overlays`] runs as a pass over a finished file, for
/// [`crate::encode_timeline_export_with_post`] to run inside the export instead. `None` when
/// there is nothing to composite or the description does not build.
pub fn text_overlay_filter(
    segments: &[TextOverlaySegment],
) -> Result<Option<String>, TextOverlayError> {
    if segments.is_empty() {
        return Ok(None);
    }
    let raw = RawTextSegments::new(segments)?;
    // SAFETY: `raw` and the strings it points into outlive the call; the C side only reads them.
    let ptr = unsafe { avbridge_text_overlay_filter(raw.raw.as_ptr(), raw.raw.len() as c_int) };
    Ok(take_c_string(ptr))
}

/// The graph that blurs the area `matte_path` (a matte video at the canvas's size and frame rate,
/// as long as the export) marks, for [`crate::encode_timeline_export_with_post`]. `None` when the
/// path or sigma cannot be used.
pub fn privacy_blur_filter(
    matte_path: &Path,
    blur_sigma: f64,
    canvas: crate::timeline_export::Canvas,
) -> Result<Option<String>, TextOverlayError> {
    let c_path = CString::new(matte_path.to_string_lossy().as_bytes())
        .map_err(TextOverlayError::InvalidPath)?;
    // SAFETY: the path is a valid C string held for the call.
    let ptr = unsafe {
        avbridge_privacy_blur_filter(
            c_path.as_ptr(),
            blur_sigma,
            canvas.width as c_int,
            canvas.height as c_int,
        )
    };
    Ok(take_c_string(ptr))
}

/// The shape counterpart of [`text_overlay_filter`].
pub fn shape_overlay_filter(segments: &[ShapeSegment]) -> Result<Option<String>, TextOverlayError> {
    if segments.is_empty() {
        return Ok(None);
    }
    let mut c_descs: Vec<CString> = Vec::with_capacity(segments.len());
    for seg in segments {
        c_descs
            .push(CString::new(seg.filter_desc.as_bytes()).map_err(TextOverlayError::InvalidPath)?);
    }
    let raw: Vec<RawShapeSegment> = c_descs
        .iter()
        .map(|desc| RawShapeSegment {
            filter_desc: desc.as_ptr(),
        })
        .collect();
    // SAFETY: as in `text_overlay_filter`.
    let ptr = unsafe { avbridge_shape_overlay_filter(raw.as_ptr(), raw.len() as c_int) };
    Ok(take_c_string(ptr))
}
