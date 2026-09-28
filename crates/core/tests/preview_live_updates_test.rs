use avcore::timeline::{ClipInstance, MaskShape};
use avcore::{live_preview_updates, PreviewCpuEffects};

#[test]
fn classifies_live_properties_without_reopening() {
    let before = ClipInstance::new(1, 2, 0.0, 0.0, 3.0);
    let mut after = before.clone();
    after.brightness = 0.2;
    after.blur_intensity = 0.4;
    after.crop_x = 0.1;
    after.pixelize_intensity = 0.5;
    after.shake_intensity = 0.3;
    assert!(live_preview_updates(&before, &after).is_some());
}

#[test]
fn classifies_pipeline_shape_changes_as_structural() {
    let before = ClipInstance::new(1, 2, 0.0, 0.0, 3.0);
    let mut chroma = before.clone();
    chroma.chroma_key_enabled = true;
    assert!(live_preview_updates(&before, &chroma).is_none());
    let mut mask = before.clone();
    mask.mask_shape = MaskShape::Circle;
    assert!(live_preview_updates(&before, &mask).is_none());
    let mut trimmed = before.clone();
    trimmed.source_out_secs = 2.0;
    assert!(live_preview_updates(&before, &trimmed).is_none());
}

#[test]
fn selects_background_cpu_effect_parameters() {
    let mut clip = ClipInstance::new(9, 2, 0.0, 0.0, 3.0);
    clip.lut_path = "look.cube".into();
    clip.vignette_intensity = 0.4;
    clip.glitch_intensity = 0.3;
    clip.deflicker_enabled = true;
    let effects = PreviewCpuEffects::for_clip(Some(&clip));
    assert_eq!(effects.clip_id, Some(9));
    assert_eq!(effects.lut_path, "look.cube");
    assert!(effects.deflicker_enabled);
    assert_eq!(
        PreviewCpuEffects::for_clip(None),
        PreviewCpuEffects::default()
    );
}
