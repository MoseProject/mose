use std::path::PathBuf;

use bush::{PreviewQuality, SourceVariant, SourceVariantError, SourceVariantKind, SourceVariants};

fn variant(kind: SourceVariantKind, width: u32, height: u32) -> SourceVariant {
    SourceVariant {
        kind,
        path: PathBuf::from(format!("{kind:?}.mp4")),
        width,
        height,
    }
}

#[test]
fn quality_selection_uses_the_smallest_sufficient_variant() {
    let variants = SourceVariants::new(vec![
        variant(SourceVariantKind::Original, 1920, 1080),
        variant(SourceVariantKind::Optimized, 1280, 720),
        variant(SourceVariantKind::Proxy, 640, 360),
    ])
    .unwrap();
    assert_eq!(
        variants.select(PreviewQuality::Full).kind,
        SourceVariantKind::Original
    );
    assert_eq!(
        variants.select(PreviewQuality::Half).kind,
        SourceVariantKind::Optimized
    );
    assert_eq!(
        variants.select(PreviewQuality::Quarter).kind,
        SourceVariantKind::Proxy
    );
}

#[test]
fn unavailable_quality_falls_back_to_the_best_available_source() {
    let variants =
        SourceVariants::new(vec![variant(SourceVariantKind::Original, 1920, 1080)]).unwrap();
    assert_eq!(
        variants.select(PreviewQuality::Quarter).kind,
        SourceVariantKind::Original
    );
}

#[test]
fn rejects_invalid_variant_sets_before_decode() {
    assert_eq!(
        SourceVariants::new(Vec::new()),
        Err(SourceVariantError::MissingOriginal)
    );
    assert_eq!(
        SourceVariants::new(vec![
            variant(SourceVariantKind::Original, 1920, 1080),
            variant(SourceVariantKind::Original, 640, 360),
        ]),
        Err(SourceVariantError::DuplicateKind(
            SourceVariantKind::Original
        ))
    );
}

#[test]
fn invalid_variants_are_rejected_and_the_list_is_ordered_by_kind() {
    let mut empty_path = variant(SourceVariantKind::Original, 640, 360);
    empty_path.path = PathBuf::new();
    assert_eq!(
        SourceVariants::new(vec![empty_path]).unwrap_err(),
        SourceVariantError::EmptyPath
    );
    for (width, height) in [(0, 360), (640, 0)] {
        assert_eq!(
            SourceVariants::new(vec![variant(SourceVariantKind::Original, width, height)])
                .unwrap_err(),
            SourceVariantError::EmptyDimensions
        );
    }
    let variants = SourceVariants::new(vec![
        variant(SourceVariantKind::Proxy, 320, 180),
        variant(SourceVariantKind::Original, 640, 360),
    ])
    .unwrap();
    let kinds: Vec<_> = variants.variants().iter().map(|v| v.kind).collect();
    assert_eq!(
        kinds,
        vec![SourceVariantKind::Original, SourceVariantKind::Proxy]
    );
}
