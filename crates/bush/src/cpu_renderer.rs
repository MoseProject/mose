//! Deterministic CPU reference compositor for Bush pixel tests.

use crate::{ChromaKey, LayerMask, Lut3D, PixelFormat, VideoFrame, VideoFrameError};

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
    And,
    Average,
    Burn,
    GrainExtract,
    Divide,
    Dodge,
    Exclusion,
    HardLight,
    Negation,
    Or,
    Phoenix,
    PinLight,
    Reflect,
    SoftLight,
    Subtract,
    VividLight,
    Xor,
    HardMix,
    LinearLight,
    Glow,
    GrainMerge,
    Multiply128,
    Heat,
    Freeze,
    Extremity,
    SoftDifference,
    Geometric,
    Harmonic,
    Bleach,
    Stain,
    Interpolate,
    HardOverlay,
}

impl BlendMode {
    /// Every mode, in FFmpeg order (the same 40 as the core timeline enum).
    pub const ALL: [Self; 40] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::Add,
        Self::Difference,
        Self::And,
        Self::Average,
        Self::Burn,
        Self::GrainExtract,
        Self::Divide,
        Self::Dodge,
        Self::Exclusion,
        Self::HardLight,
        Self::Negation,
        Self::Or,
        Self::Phoenix,
        Self::PinLight,
        Self::Reflect,
        Self::SoftLight,
        Self::Subtract,
        Self::VividLight,
        Self::Xor,
        Self::HardMix,
        Self::LinearLight,
        Self::Glow,
        Self::GrainMerge,
        Self::Multiply128,
        Self::Heat,
        Self::Freeze,
        Self::Extremity,
        Self::SoftDifference,
        Self::Geometric,
        Self::Harmonic,
        Self::Bleach,
        Self::Stain,
        Self::Interpolate,
        Self::HardOverlay,
    ];

    /// The name FFmpeg `blend=all_mode=` uses for this mode; `Normal` has no blend function.
    pub const fn ffmpeg_name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Darken => "darken",
            Self::Lighten => "lighten",
            Self::Add => "addition",
            Self::Difference => "difference",
            Self::And => "and",
            Self::Average => "average",
            Self::Burn => "burn",
            Self::GrainExtract => "grainextract",
            Self::Divide => "divide",
            Self::Dodge => "dodge",
            Self::Exclusion => "exclusion",
            Self::HardLight => "hardlight",
            Self::Negation => "negation",
            Self::Or => "or",
            Self::Phoenix => "phoenix",
            Self::PinLight => "pinlight",
            Self::Reflect => "reflect",
            Self::SoftLight => "softlight",
            Self::Subtract => "subtract",
            Self::VividLight => "vividlight",
            Self::Xor => "xor",
            Self::HardMix => "hardmix",
            Self::LinearLight => "linearlight",
            Self::Glow => "glow",
            Self::GrainMerge => "grainmerge",
            Self::Multiply128 => "multiply128",
            Self::Heat => "heat",
            Self::Freeze => "freeze",
            Self::Extremity => "extremity",
            Self::SoftDifference => "softdifference",
            Self::Geometric => "geometric",
            Self::Harmonic => "harmonic",
            Self::Bleach => "bleach",
            Self::Stain => "stain",
            Self::Interpolate => "interpolate",
            Self::HardOverlay => "hardoverlay",
        }
    }

    /// Blends one colour channel; both operands are in `0.0..=1.0`. Every mode except `Normal`
    /// uses FFmpeg 8-bit integer arithmetic (see `blend8.rs`), so a preview of opaque pixels
    /// equals the exported pixels.
    fn channel(self, backdrop: f32, source: f32) -> f32 {
        if crate::blend8::uses_integer_domain(self) {
            let a = (source * 255.0).round() as u8;
            let b = (backdrop * 255.0).round() as u8;
            return f32::from(crate::blend8::blend8(self, a, b)) / 255.0;
        }
        source
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
    /// Mirror the cropped layer horizontally / vertically (before rotation).
    pub flip_x: bool,
    pub flip_y: bool,
}

impl Default for LayerTransform {
    fn default() -> Self {
        Self {
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_deg: 0.0,
            crop: CropRect::FULL,
            flip_x: false,
            flip_y: false,
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
    /// Sepia tone mix, `0.0..=1.0` (`0` is off).
    pub sepia: f32,
    /// Gaussian blur sigma in source pixels, `0.0..=64.0` (`0` is off), applied to the layer
    /// frame before it is composited.
    pub blur_sigma: f32,
    /// Unsharp-mask amount, `0.0..=4.0` (`0` is off), applied after the blur.
    pub sharpen: f32,
    /// Mosaic block edge in source pixels; `0` and `1` are off. At most 512.
    pub pixelize_block: u32,
    pub chroma_key: Option<ChromaKey>,
    pub mask: Option<LayerMask>,
}

impl Default for LayerEffects {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            vignette: 0.0,
            sepia: 0.0,
            blur_sigma: 0.0,
            sharpen: 0.0,
            pixelize_block: 0,
            chroma_key: None,
            mask: None,
        }
    }
}

impl LayerEffects {
    fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    /// Whether the brightness/contrast/saturation/vignette pass can be skipped.
    fn colour_is_neutral(&self) -> bool {
        let neutral = Self::default();
        self.brightness == neutral.brightness
            && self.contrast == neutral.contrast
            && self.saturation == neutral.saturation
            && self.vignette == neutral.vignette
            && self.sepia == neutral.sepia
    }

    pub(crate) fn is_valid(&self) -> bool {
        (-1.0..=1.0).contains(&self.brightness)
            && (0.0..=4.0).contains(&self.contrast)
            && (0.0..=4.0).contains(&self.saturation)
            && (0.0..=1.0).contains(&self.vignette)
            && (0.0..=1.0).contains(&self.sepia)
            && (0.0..=crate::MAX_BLUR_SIGMA).contains(&self.blur_sigma)
            && (0.0..=4.0).contains(&self.sharpen)
            && self.pixelize_block <= 512
            && self.chroma_key.is_none_or(|key| key.is_valid())
            && self.mask.is_none_or(|mask| mask.is_valid())
    }

    /// `radius` is `0.0` at the layer centre and `1.0` at its corners.
    fn apply(&self, rgb: [f32; 3], radius: f32) -> [f32; 3] {
        let falloff = 1.0 - self.vignette * radius * radius;
        let mut out = rgb.map(|channel| {
            (((channel - 0.5) * self.contrast + 0.5) + self.brightness).clamp(0.0, 1.0)
        });
        let luma = 0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2];
        for channel in &mut out {
            *channel = (luma + (*channel - luma) * self.saturation).clamp(0.0, 1.0);
        }
        if self.sepia > 0.0 {
            let [red, green, blue] = out;
            let toned = [
                0.393 * red + 0.769 * green + 0.189 * blue,
                0.349 * red + 0.686 * green + 0.168 * blue,
                0.272 * red + 0.534 * green + 0.131 * blue,
            ];
            for (channel, tone) in out.iter_mut().zip(toned) {
                *channel = (*channel + (tone.min(1.0) - *channel) * self.sepia).clamp(0.0, 1.0);
            }
        }
        for channel in &mut out {
            *channel = (*channel * falloff).clamp(0.0, 1.0);
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
    /// Optional 3D colour lookup applied before the colour adjustments.
    pub lut: Option<&'a Lut3D>,
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
            lut: None,
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
    /// A GPU renderer failed (adapter, device, shader, readback); the message names the stage.
    Gpu(String),
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
        let (stride, bytes) =
            validate_render_input(width, height, timestamp_secs, layers, self.max_output_bytes)?;

        // Neighbourhood effects (blur, sharpen) produce a new frame per affected layer first.
        let processed: Vec<Option<VideoFrame>> = layers
            .iter()
            .map(|layer| {
                crate::blur::apply_effects(
                    layer.frame,
                    layer.effects.blur_sigma,
                    layer.effects.sharpen,
                )
            })
            .collect();
        let adjusted: Vec<LayerFrame<'_>> = layers
            .iter()
            .zip(&processed)
            .map(|(layer, frame)| LayerFrame {
                frame: frame.as_ref().unwrap_or(layer.frame),
                ..*layer
            })
            .collect();
        let layers = adjusted.as_slice();

        let mut pixels = vec![0; bytes];
        let row_bytes = width as usize * 4;
        let workers = worker_count(width, height, layers.len());
        if workers <= 1 {
            for layer in layers {
                blend_layer(&mut pixels, 0, width, height, layer);
            }
        } else {
            // Destination pixels are independent, so horizontal bands composite in parallel
            // with output identical to the sequential order.
            let rows_per_band = (height as usize).div_ceil(workers);
            std::thread::scope(|scope| {
                for (index, band) in pixels.chunks_mut(rows_per_band * row_bytes).enumerate() {
                    let first_row = index * rows_per_band;
                    scope.spawn(move || {
                        for layer in layers {
                            blend_layer(band, first_row, width, height, layer);
                        }
                    });
                }
            });
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

/// Validates a render request and returns `(stride_bytes, total_bytes)`. Shared by every
/// renderer so they all reject exactly the same input.
pub(crate) fn validate_render_input(
    width: u32,
    height: u32,
    timestamp_secs: f64,
    layers: &[LayerFrame<'_>],
    max_output_bytes: usize,
) -> Result<(usize, usize), RenderError> {
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

    let overflow = RenderError::OutputExceedsBudget {
        requested_bytes: usize::MAX,
        budget_bytes: max_output_bytes,
    };
    let stride = usize::try_from(width)
        .ok()
        .and_then(|width| width.checked_mul(4))
        .ok_or_else(|| overflow.clone())?;
    let bytes = stride
        .checked_mul(height as usize)
        .ok_or_else(|| overflow.clone())?;
    if bytes > max_output_bytes {
        return Err(RenderError::OutputExceedsBudget {
            requested_bytes: bytes,
            budget_bytes: max_output_bytes,
        });
    }
    Ok((stride, bytes))
}

/// Frames below this many pixels composite on the calling thread: spawning costs more.
const PARALLEL_MIN_PIXELS: usize = 256 * 256;

/// Upper bound on compositing threads for one frame.
const MAX_WORKERS: usize = 8;

fn worker_count(width: u32, height: u32, layer_count: usize) -> usize {
    if layer_count == 0 || (width as usize) * (height as usize) < PARALLEL_MIN_PIXELS {
        return 1;
    }
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(MAX_WORKERS)
        .min(height as usize)
}

/// Where a layer lands on the canvas: its crop window in source pixels, the centre and rotation
/// of the transformed rectangle, and its clipped destination bounding box. Shared by the CPU and
/// GPU compositors so both walk exactly the same pixels with exactly the same arithmetic.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LayerGeometry {
    pub crop_x0: i64,
    pub crop_y0: i64,
    pub crop_w: i64,
    pub crop_h: i64,
    pub center_x: f32,
    pub center_y: f32,
    pub sin: f32,
    pub cos: f32,
    pub start_x: i64,
    pub start_y: i64,
    pub end_x: i64,
    pub end_y: i64,
}

impl LayerGeometry {
    /// `None` when the transformed layer does not intersect the canvas.
    pub(crate) fn new(
        canvas_width: u32,
        canvas_height: u32,
        layer: &LayerFrame<'_>,
    ) -> Option<Self> {
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
            return None;
        }
        Some(Self {
            crop_x0,
            crop_y0,
            crop_w,
            crop_h,
            center_x,
            center_y,
            sin,
            cos,
            start_x,
            start_y,
            end_x,
            end_y,
        })
    }
}

/// Anything that can composite layers into a frame: the CPU reference or a GPU implementation.
pub trait Renderer: Send {
    /// Composites `layers` bottom-to-top onto a transparent canvas.
    fn render(
        &mut self,
        width: u32,
        height: u32,
        timestamp_secs: f64,
        layers: &[LayerFrame<'_>],
    ) -> Result<VideoFrame, RenderError>;

    /// A short stable name for diagnostics ("cpu", or the GPU adapter).
    fn name(&self) -> String;
}

impl Renderer for CpuRenderer {
    fn render(
        &mut self,
        width: u32,
        height: u32,
        timestamp_secs: f64,
        layers: &[LayerFrame<'_>],
    ) -> Result<VideoFrame, RenderError> {
        CpuRenderer::render(self, width, height, timestamp_secs, layers)
    }

    fn name(&self) -> String {
        "cpu".to_string()
    }
}

/// Inverse-maps each destination pixel centre into the source (nearest sample), so scale,
/// rotation and crop share one deterministic path. Only the transformed bounding box is visited.
/// `canvas` holds whole rows starting at canvas row `first_row`; rows outside it are skipped.
fn blend_layer(
    canvas: &mut [u8],
    first_row: usize,
    canvas_width: u32,
    canvas_height: u32,
    layer: &LayerFrame<'_>,
) {
    let source = layer.frame;
    let transform = layer.transform;
    let Some(geometry) = LayerGeometry::new(canvas_width, canvas_height, layer) else {
        return;
    };
    let LayerGeometry {
        crop_x0,
        crop_y0,
        crop_w,
        crop_h,
        center_x,
        center_y,
        sin,
        cos,
        start_x,
        start_y,
        end_x,
        end_y,
    } = geometry;
    let neutral_effects = layer.effects.is_neutral() && layer.lut.is_none();
    let canvas_width = canvas_width as usize;
    let band_rows = (canvas.len() / (canvas_width * 4)) as i64;
    let band_start = first_row as i64;
    let start_y = start_y.max(band_start);
    let end_y = end_y.min(band_start + band_rows);
    let identity = !transform.flip_x
        && !transform.flip_y
        && transform.scale_x == 1.0
        && transform.scale_y == 1.0
        && sin == 0.0
        && cos == 1.0;
    for destination_y in start_y..end_y {
        if identity && neutral_effects {
            // Unscaled, unrotated, uncoloured: the source maps 1:1 onto destination pixels, so
            // skip the per-pixel float mapping (exactly equal to the general path below).
            let source_y = (crop_y0 + destination_y - layer.y as i64) as usize;
            if destination_y < layer.y as i64 || source_y >= (crop_y0 + crop_h) as usize {
                continue;
            }
            let row_start = ((destination_y - band_start) as usize) * canvas_width * 4;
            for destination_x in start_x..end_x {
                let source_x = crop_x0 + destination_x - layer.x as i64;
                if source_x < crop_x0 || source_x >= crop_x0 + crop_w {
                    continue;
                }
                let source_index = source_y * source.stride_bytes() + source_x as usize * 4;
                let destination_index = row_start + destination_x as usize * 4;
                blend_pixel(
                    &mut canvas[destination_index..destination_index + 4],
                    &source.pixels()[source_index..source_index + 4],
                    layer.opacity,
                    layer.blend,
                );
            }
            continue;
        }
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
            let (mut local_x, mut local_y) = (u as i64, v as i64);
            let block = i64::from(layer.effects.pixelize_block);
            if block > 1 {
                // Mosaic: every pixel in a block samples the block centre.
                local_x = ((local_x / block) * block + block / 2).min(crop_w - 1);
                local_y = ((local_y / block) * block + block / 2).min(crop_h - 1);
            }
            if transform.flip_x {
                local_x = crop_w - 1 - local_x;
            }
            if transform.flip_y {
                local_y = crop_h - 1 - local_y;
            }
            let source_x = (crop_x0 + local_x) as usize;
            let source_y = (crop_y0 + local_y) as usize;
            let source_index = source_y * source.stride_bytes() + source_x * 4;
            let destination_index =
                ((destination_y - band_start) as usize * canvas_width + destination_x as usize) * 4;
            let mut pixel: [u8; 4] = source.pixels()[source_index..source_index + 4]
                .try_into()
                .expect("slice of four bytes");
            if !neutral_effects {
                let nx = (u / crop_w as f32 - 0.5) * 2.0;
                let ny = (v / crop_h as f32 - 0.5) * 2.0;
                if let Some(key) = layer.effects.chroma_key {
                    let factor = key.alpha_factor([pixel[0], pixel[1], pixel[2]]);
                    pixel[3] = (f32::from(pixel[3]) * factor).round() as u8;
                }
                if layer.lut.is_some() || !layer.effects.colour_is_neutral() {
                    let mut rgb = [
                        pixel[0] as f32 / 255.0,
                        pixel[1] as f32 / 255.0,
                        pixel[2] as f32 / 255.0,
                    ];
                    if let Some(lut) = layer.lut {
                        rgb = lut.apply(rgb);
                    }
                    if !layer.effects.colour_is_neutral() {
                        let radius = ((nx * nx + ny * ny) / 2.0).sqrt();
                        rgb = layer.effects.apply(rgb, radius);
                    }
                    for channel in 0..3 {
                        pixel[channel] = (rgb[channel] * 255.0).round() as u8;
                    }
                }
                if let Some(mask) = layer.effects.mask {
                    let factor = mask.alpha_factor(nx, ny);
                    pixel[3] = (f32::from(pixel[3]) * factor).round() as u8;
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
    // Exact fast paths: an opaque normal-blend source replaces the backdrop, and over a
    // transparent backdrop the blend function cancels out, leaving the source colour.
    if mode == BlendMode::Normal && source[3] == 255 && opacity >= 1.0 {
        destination.copy_from_slice(&source[..4]);
        return;
    }
    let source_alpha = (source[3] as f32 / 255.0) * opacity;
    if source_alpha == 0.0 {
        return;
    }
    if destination[3] == 0 {
        destination[..3].copy_from_slice(&source[..3]);
        destination[3] = (source_alpha * 255.0).round() as u8;
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
