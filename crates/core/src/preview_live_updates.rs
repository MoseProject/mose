//! Shared classification and dispatch data for live preview property updates.
//!
//! A preview pipeline is expensive to rebuild. These are precisely the properties backed by
//! mutable GStreamer elements, so frontends can update them without reopening the decoder.

use crate::preview::Preview;
use crate::timeline::{ClipInstance, ColorFilter, MaskShape};

/// CPU-only effects applied after decode to the background preview frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreviewCpuEffects {
    pub lut_path: String,
    pub vignette_intensity: f32,
    pub glitch_intensity: f32,
    pub deflicker_enabled: bool,
    pub clip_id: Option<u64>,
}

impl PreviewCpuEffects {
    pub fn for_clip(clip: Option<&ClipInstance>) -> Self {
        let Some(clip) = clip else {
            return Self::default();
        };
        Self {
            lut_path: clip.lut_path.clone(),
            vignette_intensity: clip.vignette_intensity,
            glitch_intensity: clip.glitch_intensity,
            deflicker_enabled: clip.deflicker_enabled,
            clip_id: Some(clip.id),
        }
    }
}

/// One update accepted by an already-open [`Preview`] pipeline.
#[derive(Clone, Debug, PartialEq)]
pub enum LivePreviewUpdate {
    Balance {
        clip_id: u64,
        brightness: f32,
        contrast: f32,
        saturation: f32,
    },
    Blur {
        clip_id: u64,
        net_sigma: f64,
    },
    ChromaKey {
        clip_id: u64,
        color: [u8; 3],
        tolerance: f32,
    },
    Crop {
        clip_id: u64,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    },
    Pixelize {
        clip_id: u64,
        intensity: f32,
    },
    Shake {
        clip_id: u64,
        intensity: f32,
    },
    Mask {
        clip_id: u64,
        shape: MaskShape,
        corner_radius: f32,
    },
}

impl LivePreviewUpdate {
    pub fn for_clip(clip: &ClipInstance) -> Vec<Self> {
        let saturation = if clip.color_filter == ColorFilter::BlackAndWhite {
            0.0
        } else {
            clip.saturation
        };
        vec![
            Self::Balance {
                clip_id: clip.id,
                brightness: clip.brightness,
                contrast: clip.contrast,
                saturation,
            },
            Self::Blur {
                clip_id: clip.id,
                net_sigma: clip.blur_intensity as f64 * 4.0 - clip.sharpen as f64 * 4.0,
            },
            Self::ChromaKey {
                clip_id: clip.id,
                color: clip.chroma_key_color,
                tolerance: clip.chroma_key_tolerance,
            },
            Self::Crop {
                clip_id: clip.id,
                x: clip.crop_x,
                y: clip.crop_y,
                width: clip.crop_w,
                height: clip.crop_h,
            },
            Self::Pixelize {
                clip_id: clip.id,
                intensity: clip.pixelize_intensity,
            },
            Self::Shake {
                clip_id: clip.id,
                intensity: clip.shake_intensity,
            },
            Self::Mask {
                clip_id: clip.id,
                shape: clip.mask_shape,
                corner_radius: clip.mask_corner_radius,
            },
        ]
    }

    pub(crate) fn apply(self, preview: &mut Preview) {
        match self {
            Self::Balance {
                clip_id,
                brightness,
                contrast,
                saturation,
            } => {
                preview.set_live_balance(clip_id, brightness, contrast, saturation);
            }
            Self::Blur { clip_id, net_sigma } => {
                preview.set_live_blur(clip_id, net_sigma);
            }
            Self::ChromaKey {
                clip_id,
                color,
                tolerance,
            } => {
                preview.set_live_chroma_key(clip_id, color, tolerance);
            }
            Self::Crop {
                clip_id,
                x,
                y,
                width,
                height,
            } => {
                preview.set_live_crop(clip_id, x, y, width, height);
            }
            Self::Pixelize { clip_id, intensity } => {
                preview.set_live_pixelize(clip_id, intensity);
            }
            Self::Shake { clip_id, intensity } => {
                preview.set_live_shake(clip_id, intensity);
            }
            Self::Mask {
                clip_id,
                shape,
                corner_radius,
            } => {
                let _ = preview.set_live_mask(clip_id, shape, corner_radius);
            }
        }
    }
}

/// Returns replacement live commands when `next` differs from `previous` only in properties
/// that an already-built pipeline can change. `None` means the pipeline must be reopened.
pub fn live_preview_updates(
    previous: &ClipInstance,
    next: &ClipInstance,
) -> Option<Vec<LivePreviewUpdate>> {
    if previous.id != next.id || previous.chroma_key_enabled != next.chroma_key_enabled {
        return None;
    }
    if (previous.mask_shape == MaskShape::None) != (next.mask_shape == MaskShape::None) {
        return None;
    }
    let mut normalized = next.clone();
    normalized.brightness = previous.brightness;
    normalized.contrast = previous.contrast;
    normalized.saturation = previous.saturation;
    normalized.color_filter = previous.color_filter;
    normalized.blur_intensity = previous.blur_intensity;
    normalized.sharpen = previous.sharpen;
    normalized.chroma_key_color = previous.chroma_key_color;
    normalized.chroma_key_tolerance = previous.chroma_key_tolerance;
    normalized.crop_x = previous.crop_x;
    normalized.crop_y = previous.crop_y;
    normalized.crop_w = previous.crop_w;
    normalized.crop_h = previous.crop_h;
    normalized.pixelize_intensity = previous.pixelize_intensity;
    normalized.shake_intensity = previous.shake_intensity;
    normalized.mask_shape = previous.mask_shape;
    normalized.mask_corner_radius = previous.mask_corner_radius;
    (normalized == *previous).then(|| LivePreviewUpdate::for_clip(next))
}
