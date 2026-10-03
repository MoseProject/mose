//! Deterministic CPU reference compositor for Bush pixel tests.

use crate::{PixelFormat, VideoFrame, VideoFrameError};

/// Separable W3C blend modes supported by the reference compositor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Add,
    Difference,
}

impl BlendMode {
    /// Blends one colour channel; both operands are in `0.0..=1.0`.
    fn channel(self, backdrop: f32, source: f32) -> f32 {
        match self {
            Self::Normal => source,
            Self::Multiply => backdrop * source,
            Self::Screen => backdrop + source - backdrop * source,
            Self::Overlay => {
                if backdrop <= 0.5 {
                    2.0 * backdrop * source
                } else {
                    1.0 - 2.0 * (1.0 - backdrop) * (1.0 - source)
                }
            }
            Self::Darken => backdrop.min(source),
            Self::Lighten => backdrop.max(source),
            Self::Add => (backdrop + source).min(1.0),
            Self::Difference => (backdrop - source).abs(),
        }
    }
}

/// Source-space crop rectangle as `0.0..=1.0` fractions of the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl CropRect {
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };
}

/// Geometry applied to a layer around its own centre. `rotation_deg` is clockwise.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerTransform {
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation_deg: f32,
    pub crop: CropRect,
}

impl Default for LayerTransform {
    fn default() -> Self {
        Self {
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: 0.0,
            crop: CropRect::FULL,
        }
    }
}

impl LayerTransform {
    pub(crate) fn is_valid(&self) -> bool {
        let crop = self.crop;
        self.scale_x.is_finite()
            && self.scale_y.is_finite()
            && self.scale_x > 0.0
            && self.scale_y > 0.0
            && self.rotation_deg.is_finite()
            && [crop.x, crop.y, crop.width, crop.height]
                .iter()
                .all(|value| value.is_finite())
            && crop.x >= 0.0
            && crop.y >= 0.0
            && crop.width > 0.0
            && crop.height > 0.0
            && crop.x + crop.width <= 1.0 + 1e-6
            && crop.y + crop.height <= 1.0 + 1e-6
    }
}

/// Per-layer colour effects applied to each sampled source pixel before blending. Neutral
/// values (`0.0`, `1.0`, `1.0`, `0.0`) skip the pass entirely.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerEffects {
    /// Added to every channel, `-1.0..=1.0`.
    pub brightness: f32,
    /// Scales distance from mid-grey, `0.0..=4.0`.
    pub contrast: f32,
    /// Scales distance from luma, `0.0..=4.0` (`0.0` is greyscale).
    pub saturation: f32,
    /// Darkens towards the layer corners, `0.0..=1.0`.
    pub vignette: f32,
}

impl Default for LayerEffects {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            vignette: 0.0,
        }
    }
}

impl LayerEffects {
    fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    pub(crate) fn is_valid(&self) -> bool {
        (-1.0..=1.0).contains(&self.brightness)
            && (0.0..=4.0).contains(&self.contrast)
            && (0.0..=4.0).contains(&self.saturation)
            && (0.0..=1.0).contains(&self.vignette)
    }

    /// `radius` is `0.0` at the layer centre and `1.0` at its corners.
    fn apply(&self, rgb: [f32; 3], radius: f32) -> [f32; 3] {
        let falloff = 1.0 - self.vignette * radius * radius;
        let mut out = rgb.map(|channel| {
            (((channel - 0.5) * self.contrast + 0.5) + self.brightness).clamp(0.0, 1.0)
        });
        let luma = 0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2];
        for channel in &mut out {
            *channel = ((luma + (*channel - luma) * self.saturation) * falloff).clamp(0.0, 1.0);
        }
        out
    }
}

/// One composited layer. `x`/`y` place the top-left corner of the cropped, scaled, unrotated
/// layer on the canvas.
#[derive(Clone, Copy, Debug)]
pub struct LayerFrame<'a> {
    pub frame: &'a VideoFrame,
    pub x: i32,
    pub y: i32,
    pub opacity: f32,
    pub transform: LayerTransform,
    pub blend: BlendMode,
    pub effects: LayerEffects,
}

impl<'a> LayerFrame<'a> {
    /// An untransformed, fully opaque, normally blended layer at the canvas origin.
    pub fn new(frame: &'a VideoFrame) -> Self {
        Self {
            frame,
            x: 0,
            y: 0,
            opacity: 1.0,
            transform: LayerTransform::default(),
            blend: BlendMode::Normal,
            effects: LayerEffects::default(),
        }
    }
}

/// Bounded, allocation-validating compositor used as the correctness reference for any future
/// GPU renderer.
#[derive(Clone, Copy, Debug)]
pub struct CpuRenderer {
    max_output_bytes: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RenderError {
    ZeroCanvas,
    NonFiniteTimestamp(f64),
    InvalidOpacity(f32),
    InvalidTransform,
    InvalidEffects,
    OutputExceedsBudget {
        requested_bytes: usize,
        budget_bytes: usize,
    },
    Frame(VideoFrameError),
}

impl From<VideoFrameError> for RenderError {
    fn from(error: VideoFrameError) -> Self {
        Self::Frame(error)
    }
}

impl CpuRenderer {
    pub fn new(max_output_bytes: usize) -> Result<Self, RenderError> {
        if max_output_bytes == 0 {
            return Err(RenderError::OutputExceedsBudget {
                requested_bytes: 1,
                budget_bytes: 0,
            });
        }
        Ok(Self { max_output_bytes })
    }

    /// Composites `layers` bottom-to-top onto a transparent canvas.
    pub fn render(
        &self,
        width: u32,
        height: u32,
        timestamp_secs: f64,
        layers: &[LayerFrame<'_>],
    ) -> Result<VideoFrame, RenderError> {
        if width == 0 || height == 0 {
            return Err(RenderError::ZeroCanvas);
        }
        if !timestamp_secs.is_finite() || timestamp_secs < 0.0 {
            return Err(RenderError::NonFiniteTimestamp(timestamp_secs));
        }
        for layer in layers {
            if !layer.opacity.is_finite() || !(0.0..=1.0).contains(&layer.opacity) {
                return Err(RenderError::InvalidOpacity(layer.opacity));
            }
            if !layer.transform.is_valid() {
                return Err(RenderError::InvalidTransform);
            }
            if !layer.effects.is_valid() {
                return Err(RenderError::InvalidEffects);
            }
        }

        let stride = usize::try_from(width)
            .ok()
            .and_then(|width| width.checked_mul(4))
            .ok_or(RenderError::OutputExceedsBudget {
                requested_bytes: usize::MAX,
                budget_bytes: self.max_output_bytes,
            })?;
        let bytes =
            stride
                .checked_mul(height as usize)
                .ok_or(RenderError::OutputExceedsBudget {
                    requested_bytes: usize::MAX,
                    budget_bytes: self.max_output_bytes,
                })?;
        if bytes > self.max_output_bytes {
            return Err(RenderError::OutputExceedsBudget {
                requested_bytes: bytes,
                budget_bytes: self.max_output_bytes,
            });
        }

        let mut pixels = vec![0; bytes];
        for layer in layers {
            blend_layer(&mut pixels, width, height, layer);
        }
        VideoFrame::new(
            PixelFormat::Rgba8,
            width,
            height,
            stride,
            timestamp_secs,
            pixels,
        )
        .map_err(Into::into)
    }
}

/// Inverse-maps each destination pixel centre into the source (nearest sample), so scale,
/// rotation and crop share one deterministic path. Only the transformed bounding box is visited.
fn blend_layer(canvas: &mut [u8], canvas_width: u32, canvas_height: u32, layer: &LayerFrame<'_>) {
    let source = layer.frame;
    let transform = layer.transform;
    let crop_x0 = (transform.crop.x * source.width() as f32).round() as i64;
    let crop_y0 = (transform.crop.y * source.height() as f32).round() as i64;
    let crop_w = ((transform.crop.width * source.width() as f32).round() as i64)
        .clamp(1, (i64::from(source.width()) - crop_x0).max(1));
    let crop_h = ((transform.crop.height * source.height() as f32).round() as i64)
        .clamp(1, (i64::from(source.height()) - crop_y0).max(1));
    let scaled_w = crop_w as f32 * transform.scale_x;
    let scaled_h = crop_h as f32 * transform.scale_y;
    let center_x = layer.x as f32 + scaled_w / 2.0;
    let center_y = layer.y as f32 + scaled_h / 2.0;
    let (sin, cos) = transform.rotation_deg.to_radians().sin_cos();
    let half_w = (scaled_w * cos.abs() + scaled_h * sin.abs()) / 2.0;
    let half_h = (scaled_w * sin.abs() + scaled_h * cos.abs()) / 2.0;
    let start_x = ((center_x - half_w).floor() as i64).max(0);
    let start_y = ((center_y - half_h).floor() as i64).max(0);
    let end_x = ((center_x + half_w).ceil() as i64).min(i64::from(canvas_width));
    let end_y = ((center_y + half_h).ceil() as i64).min(i64::from(canvas_height));
    if start_x >= end_x || start_y >= end_y {
        return;
    }

    let neutral_effects = layer.effects.is_neutral();
    let canvas_width = canvas_width as usize;
    for destination_y in start_y..end_y {
        for destination_x in start_x..end_x {
            let dx = destination_x as f32 + 0.5 - center_x;
            let dy = destination_y as f32 + 0.5 - center_y;
            // Inverse of a clockwise rotation in y-down space.
            let rx = dx * cos + dy * sin;
            let ry = -dx * sin + dy * cos;
            let u = rx / transform.scale_x + crop_w as f32 / 2.0;
            let v = ry / transform.scale_y + crop_h as f32 / 2.0;
            if u < 0.0 || v < 0.0 || u >= crop_w as f32 || v >= crop_h as f32 {
                continue;
            }
            let source_x = (crop_x0 + u as i64) as usize;
            let source_y = (crop_y0 + v as i64) as usize;
            let source_index = source_y * source.stride_bytes() + source_x * 4;
            let destination_index =
                (destination_y as usize * canvas_width + destination_x as usize) * 4;
            let mut pixel: [u8; 4] = source.pixels()[source_index..source_index + 4]
                .try_into()
                .expect("slice of four bytes");
            if !neutral_effects {
                let nx = (u / crop_w as f32 - 0.5) * 2.0;
                let ny = (v / crop_h as f32 - 0.5) * 2.0;
                let radius = ((nx * nx + ny * ny) / 2.0).sqrt();
                let rgb = layer.effects.apply(
                    [
                        pixel[0] as f32 / 255.0,
                        pixel[1] as f32 / 255.0,
                        pixel[2] as f32 / 255.0,
                    ],
                    radius,
                );
                for channel in 0..3 {
                    pixel[channel] = (rgb[channel] * 255.0).round() as u8;
                }
            }
            blend_pixel(
                &mut canvas[destination_index..destination_index + 4],
                &pixel,
                layer.opacity,
                layer.blend,
            );
        }
    }
}

/// W3C compositing: `co = as*(1-ab)*cs + as*ab*B(cb,cs) + (1-as)*ab*cb`, `ao = as + ab*(1-as)`.
fn blend_pixel(destination: &mut [u8], source: &[u8], opacity: f32, mode: BlendMode) {
    let source_alpha = (source[3] as f32 / 255.0) * opacity;
    if source_alpha == 0.0 {
        return;
    }
    let destination_alpha = destination[3] as f32 / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    for channel in 0..3 {
        let source_color = source[channel] as f32 / 255.0;
        let destination_color = destination[channel] as f32 / 255.0;
        let blended = mode.channel(destination_color, source_color);
        let premultiplied = source_alpha * (1.0 - destination_alpha) * source_color
            + source_alpha * destination_alpha * blended
            + (1.0 - source_alpha) * destination_alpha * destination_color;
        destination[channel] = ((premultiplied / output_alpha) * 255.0).round() as u8;
    }
    destination[3] = (output_alpha * 255.0).round() as u8;
}
