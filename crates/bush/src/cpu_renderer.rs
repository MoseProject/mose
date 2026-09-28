//! Deterministic CPU reference compositor for Bush pixel tests.

use crate::{PixelFormat, VideoFrame, VideoFrameError};

/// One RGBA source positioned in canvas coordinates, in painter order.
#[derive(Clone, Copy, Debug)]
pub struct LayerFrame<'a> {
    pub frame: &'a VideoFrame,
    pub x: i32,
    pub y: i32,
    /// Straight-alpha layer opacity in the inclusive range `0.0..=1.0`.
    pub opacity: f32,
}

/// Bounded, deterministic CPU compositor used as the pixel-correctness reference.
#[derive(Clone, Copy, Debug)]
pub struct CpuRenderer {
    max_output_bytes: usize,
}

/// Invalid render input or output that would exceed the configured CPU reference budget.
#[derive(Clone, Debug, PartialEq)]
pub enum RenderError {
    ZeroCanvas,
    NonFiniteTimestamp(f64),
    InvalidOpacity(f32),
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
    /// Creates a reference renderer with a hard allocation budget for one output frame.
    pub fn new(max_output_bytes: usize) -> Result<Self, RenderError> {
        if max_output_bytes == 0 {
            return Err(RenderError::OutputExceedsBudget {
                requested_bytes: 1,
                budget_bytes: 0,
            });
        }
        Ok(Self { max_output_bytes })
    }

    /// Composites RGBA layers in input order using straight-alpha source-over blending.
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

fn blend_layer(canvas: &mut [u8], canvas_width: u32, canvas_height: u32, layer: &LayerFrame<'_>) {
    let source = layer.frame;
    let source_width = source.width() as i64;
    let source_height = source.height() as i64;
    let canvas_width = canvas_width as i64;
    let canvas_height = canvas_height as i64;
    let origin_x = i64::from(layer.x);
    let origin_y = i64::from(layer.y);
    let start_x = origin_x.max(0);
    let start_y = origin_y.max(0);
    let end_x = (origin_x + source_width).min(canvas_width);
    let end_y = (origin_y + source_height).min(canvas_height);
    if start_x >= end_x || start_y >= end_y {
        return;
    }

    for destination_y in start_y..end_y {
        let source_y = (destination_y - origin_y) as usize;
        for destination_x in start_x..end_x {
            let source_x = (destination_x - origin_x) as usize;
            let source_index = source_y * source.stride_bytes() + source_x * 4;
            let destination_index =
                (destination_y as usize * canvas_width as usize + destination_x as usize) * 4;
            blend_pixel(
                &mut canvas[destination_index..destination_index + 4],
                &source.pixels()[source_index..source_index + 4],
                layer.opacity,
            );
        }
    }
}

fn blend_pixel(destination: &mut [u8], source: &[u8], opacity: f32) {
    let source_alpha = (source[3] as f32 / 255.0) * opacity;
    if source_alpha == 0.0 {
        return;
    }
    let destination_alpha = destination[3] as f32 / 255.0;
    let output_alpha = source_alpha + destination_alpha * (1.0 - source_alpha);
    for channel in 0..3 {
        let source_color = source[channel] as f32 / 255.0;
        let destination_color = destination[channel] as f32 / 255.0;
        let output_color = (source_color * source_alpha
            + destination_color * destination_alpha * (1.0 - source_alpha))
            / output_alpha;
        destination[channel] = (output_color * 255.0).round() as u8;
    }
    destination[3] = (output_alpha * 255.0).round() as u8;
}
