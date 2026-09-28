use super::*;

#[test]
fn every_preset_has_a_distinct_non_empty_label_per_locale() {
    let presets = [
        ShapePreset::Ellipse,
        ShapePreset::Rectangle,
        ShapePreset::Triangle,
        ShapePreset::Trapezoid,
        ShapePreset::Arrow,
        ShapePreset::Custom,
    ];
    for locale in [crate::i18n::Locale::PtBr, crate::i18n::Locale::En] {
        let mut labels: Vec<String> = presets
            .iter()
            .map(|&p| shape_preset_label(p, locale))
            .collect();
        for label in &labels {
            assert!(!label.is_empty());
        }
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), presets.len());
    }
}
