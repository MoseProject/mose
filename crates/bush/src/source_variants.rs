//! Explicit original/proxy/optimized source selection for interactive preview.

use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SourceVariantKind {
    Original,
    Optimized,
    Proxy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewQuality {
    Full,
    Half,
    Quarter,
}

impl PreviewQuality {
    const fn divisor(self) -> u32 {
        match self {
            Self::Full => 1,
            Self::Half => 2,
            Self::Quarter => 4,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceVariant {
    pub kind: SourceVariantKind,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceVariantError {
    EmptyPath,
    EmptyDimensions,
    MissingOriginal,
    DuplicateKind(SourceVariantKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceVariants {
    variants: Vec<SourceVariant>,
    original_width: u32,
    original_height: u32,
}

impl SourceVariants {
    /// Validates variants prepared by Mose before any decoder is opened.
    pub fn new(mut variants: Vec<SourceVariant>) -> Result<Self, SourceVariantError> {
        let mut kinds = HashSet::with_capacity(variants.len());
        for variant in &variants {
            if variant.path.as_os_str().is_empty() {
                return Err(SourceVariantError::EmptyPath);
            }
            if variant.width == 0 || variant.height == 0 {
                return Err(SourceVariantError::EmptyDimensions);
            }
            if !kinds.insert(variant.kind) {
                return Err(SourceVariantError::DuplicateKind(variant.kind));
            }
        }
        let original = variants
            .iter()
            .find(|variant| variant.kind == SourceVariantKind::Original)
            .ok_or(SourceVariantError::MissingOriginal)?;
        let (original_width, original_height) = (original.width, original.height);
        variants.sort_unstable_by_key(|variant| variant.kind);
        Ok(Self {
            variants,
            original_width,
            original_height,
        })
    }

    pub fn variants(&self) -> &[SourceVariant] {
        &self.variants
    }

    /// Picks the lowest-resolution source that satisfies the requested quality, else the best available.
    pub fn select(&self, quality: PreviewQuality) -> &SourceVariant {
        let divisor = quality.divisor();
        let minimum_width = self.original_width.div_ceil(divisor);
        let minimum_height = self.original_height.div_ceil(divisor);
        self.variants
            .iter()
            .filter(|variant| variant.width >= minimum_width && variant.height >= minimum_height)
            .min_by_key(|variant| {
                (
                    u64::from(variant.width) * u64::from(variant.height),
                    variant.kind,
                )
            })
            .unwrap_or_else(|| {
                self.variants
                    .iter()
                    .max_by_key(|variant| {
                        (
                            u64::from(variant.width) * u64::from(variant.height),
                            variant.kind,
                        )
                    })
                    .expect("validated source variants contain an original")
            })
    }
}
