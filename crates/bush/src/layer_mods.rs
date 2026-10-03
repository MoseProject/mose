//! Per-layer pixel operations beyond plain colour grading: chroma key, shape masks and 3D LUTs.
//!
//! All operate in the layer's own cropped space so they follow its transform. They are
//! deterministic reference implementations; a GPU renderer must match them within rounding.

/// Keys out pixels near `color`. `tolerance` and `softness` are distances in normalised RGB
/// space (`0.0` identical colour, `1.0` the black-to-white diagonal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChromaKey {
    pub color: [u8; 3],
    /// Pixels within this distance become fully transparent.
    pub tolerance: f32,
    /// Width of the ramp from transparent back to opaque beyond `tolerance`.
    pub softness: f32,
}

impl ChromaKey {
    pub(crate) fn is_valid(&self) -> bool {
        (0.0..=1.0).contains(&self.tolerance) && (0.0..=1.0).contains(&self.softness)
    }

    /// Alpha multiplier for a source pixel, `0.0..=1.0`.
    pub(crate) fn alpha_factor(&self, rgb: [u8; 3]) -> f32 {
        let distance = (0..3)
            .map(|channel| {
                let difference = (f32::from(rgb[channel]) - f32::from(self.color[channel])) / 255.0;
                difference * difference
            })
            .sum::<f32>()
            .sqrt()
            / 3.0_f32.sqrt();
        if distance <= self.tolerance {
            0.0
        } else if self.softness > 0.0 && distance < self.tolerance + self.softness {
            (distance - self.tolerance) / self.softness
        } else {
            1.0
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaskShape {
    Rectangle,
    Ellipse,
}

/// A shape mask over the layer's cropped area: pixels inside the shape stay, outside become
/// transparent (or the reverse when `invert`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerMask {
    pub shape: MaskShape,
    /// Rectangle corner rounding as a fraction of the half-size, `0.0..=1.0`.
    pub corner_radius: f32,
    /// Soft edge width in normalised half-size units; `0.0` is a hard edge.
    pub feather: f32,
    pub invert: bool,
}

impl LayerMask {
    pub(crate) fn is_valid(&self) -> bool {
        (0.0..=1.0).contains(&self.corner_radius) && (0.0..=1.0).contains(&self.feather)
    }

    /// Alpha multiplier at normalised layer coordinates (`-1.0..=1.0` on both axes).
    pub(crate) fn alpha_factor(&self, nx: f32, ny: f32) -> f32 {
        let distance = match self.shape {
            MaskShape::Ellipse => (nx * nx + ny * ny).sqrt() - 1.0,
            MaskShape::Rectangle => {
                let radius = self.corner_radius;
                let qx = nx.abs() - (1.0 - radius);
                let qy = ny.abs() - (1.0 - radius);
                let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
                outside + qx.max(qy).min(0.0) - radius
            }
        };
        let inside = if self.feather > 0.0 {
            (0.5 - distance / self.feather).clamp(0.0, 1.0)
        } else if distance <= 0.0 {
            1.0
        } else {
            0.0
        };
        if self.invert {
            1.0 - inside
        } else {
            inside
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LutError {
    /// Edge length must be `2..=65`.
    InvalidSize(usize),
    /// Entry count must be `size^3`.
    WrongEntryCount {
        expected: usize,
        actual: usize,
    },
    NonFiniteEntry,
}

/// A 3D colour lookup table with trilinear interpolation. Entries are ordered with red
/// varying fastest, then green, then blue (the `.cube` convention).
#[derive(Clone, Debug, PartialEq)]
pub struct Lut3D {
    size: usize,
    entries: Vec<[f32; 3]>,
}

impl Lut3D {
    pub fn new(size: usize, entries: Vec<[f32; 3]>) -> Result<Self, LutError> {
        if !(2..=65).contains(&size) {
            return Err(LutError::InvalidSize(size));
        }
        let expected = size * size * size;
        if entries.len() != expected {
            return Err(LutError::WrongEntryCount {
                expected,
                actual: entries.len(),
            });
        }
        if entries
            .iter()
            .any(|entry| entry.iter().any(|value| !value.is_finite()))
        {
            return Err(LutError::NonFiniteEntry);
        }
        Ok(Self { size, entries })
    }

    /// The identity table of edge length `size`.
    pub fn identity(size: usize) -> Result<Self, LutError> {
        if !(2..=65).contains(&size) {
            return Err(LutError::InvalidSize(size));
        }
        let scale = (size - 1) as f32;
        let mut entries = Vec::with_capacity(size * size * size);
        for blue in 0..size {
            for green in 0..size {
                for red in 0..size {
                    entries.push([
                        red as f32 / scale,
                        green as f32 / scale,
                        blue as f32 / scale,
                    ]);
                }
            }
        }
        Self::new(size, entries)
    }

    pub const fn size(&self) -> usize {
        self.size
    }

    pub fn retained_bytes(&self) -> usize {
        self.entries.len() * std::mem::size_of::<[f32; 3]>()
    }

    /// Trilinear lookup of `rgb` (each `0.0..=1.0`); the result is clamped to `0.0..=1.0`.
    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let max_index = (self.size - 1) as f32;
        let position = rgb.map(|channel| channel.clamp(0.0, 1.0) * max_index);
        let base = position.map(|value| (value.floor() as usize).min(self.size - 2));
        let fraction = [
            position[0] - base[0] as f32,
            position[1] - base[1] as f32,
            position[2] - base[2] as f32,
        ];
        let at = |r: usize, g: usize, b: usize| self.entries[(b * self.size + g) * self.size + r];
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let mut output = [0.0_f32; 3];
        for (channel, slot) in output.iter_mut().enumerate() {
            let c00 = lerp(
                at(base[0], base[1], base[2])[channel],
                at(base[0] + 1, base[1], base[2])[channel],
                fraction[0],
            );
            let c10 = lerp(
                at(base[0], base[1] + 1, base[2])[channel],
                at(base[0] + 1, base[1] + 1, base[2])[channel],
                fraction[0],
            );
            let c01 = lerp(
                at(base[0], base[1], base[2] + 1)[channel],
                at(base[0] + 1, base[1], base[2] + 1)[channel],
                fraction[0],
            );
            let c11 = lerp(
                at(base[0], base[1] + 1, base[2] + 1)[channel],
                at(base[0] + 1, base[1] + 1, base[2] + 1)[channel],
                fraction[0],
            );
            let c0 = lerp(c00, c10, fraction[1]);
            let c1 = lerp(c01, c11, fraction[1]);
            *slot = lerp(c0, c1, fraction[2]).clamp(0.0, 1.0);
        }
        output
    }
}
