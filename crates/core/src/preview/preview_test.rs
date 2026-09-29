use super::*;

#[test]
fn gain_db_to_linear_zero_db_is_unity_gain() {
    assert!((gain_db_to_linear(0.0) - 1.0).abs() < 1e-9);
}

#[test]
fn gain_db_to_linear_plus_20db_is_ten_times_amplitude() {
    assert!((gain_db_to_linear(20.0) - 10.0).abs() < 1e-9);
}

#[test]
fn gain_db_to_linear_minus_20db_is_one_tenth_amplitude() {
    assert!((gain_db_to_linear(-20.0) - 0.1).abs() < 1e-9);
}

#[test]
fn gain_db_to_linear_is_monotonically_increasing_with_db() {
    assert!(gain_db_to_linear(-6.0) < gain_db_to_linear(0.0));
    assert!(gain_db_to_linear(0.0) < gain_db_to_linear(6.0));
}

#[test]
fn shake_margin_pixels_is_zero_for_zero_intensity() {
    assert_eq!(shake_margin_pixels(1920, 1080, 0.0), (0, 0));
}

#[test]
fn shake_margin_pixels_scales_with_dimensions_and_margin() {
    let (w, h) = shake_margin_pixels(1000, 500, 0.08);
    // total_trim = round(dim * 2 * margin) = round(dim * 0.16)
    assert_eq!(w, 160);
    assert_eq!(h, 80);
}

#[test]
fn shake_margin_pixels_never_exceeds_the_dimension_minus_two() {
    // An extreme margin must clamp rather than trim more than the frame itself allows.
    let (w, h) = shake_margin_pixels(100, 50, 5.0);
    assert_eq!(w, 98);
    assert_eq!(h, 48);
}

#[test]
fn shake_margin_pixels_never_goes_negative_for_a_tiny_dimension() {
    let (w, h) = shake_margin_pixels(1, 1, 1.0);
    assert_eq!(w, 0);
    assert_eq!(h, 0);
}

#[test]
fn live_element_names_are_distinct_per_role_for_the_same_clip() {
    let clip_id = 42;
    let names = [
        live_balance_element_name(clip_id),
        live_blur_element_name(clip_id),
        live_crop_element_name(clip_id),
        live_pixelize_caps_name(clip_id),
        live_chroma_key_element_name(clip_id),
    ];
    let mut deduped = names.to_vec();
    deduped.sort();
    deduped.dedup();
    assert_eq!(deduped.len(), names.len());
}

#[test]
fn live_element_names_are_distinct_per_clip_for_the_same_role() {
    assert_ne!(live_balance_element_name(1), live_balance_element_name(2));
}

#[test]
fn blend_over_channel_respects_the_overlay_alpha() {
    use crate::timeline::BlendMode;

    // Addition of 100 over 100 is 200; at 50% alpha it must be halfway between that and the
    // original bottom value, rather than replacing it outright as the old CPU path did.
    assert_eq!(blend_over_channel(BlendMode::Addition, 100, 100, 128), 150);
    assert_eq!(blend_over_channel(BlendMode::Addition, 100, 100, 0), 100);
    assert_eq!(blend_over_channel(BlendMode::Addition, 100, 100, 255), 200);
}

#[test]
fn blend_rgba_frame_applies_each_channel_and_preserves_base_alpha() {
    let mut base = [100, 50, 10, 77];
    let layer = [100, 20, 250, 128];

    assert!(blend_rgba_frame(&mut base, &layer, BlendMode::Addition));
    assert_eq!(base, [150, 60, 133, 77]);
}

#[test]
fn blend_rgba_frame_leaves_a_transparent_layer_unchanged() {
    let mut base = [12, 34, 56, 78];
    let layer = [255, 255, 255, 0];

    assert!(blend_rgba_frame(&mut base, &layer, BlendMode::Addition));
    assert_eq!(base, [12, 34, 56, 78]);
}

#[test]
fn blend_rgba_frame_applies_layers_in_stack_order() {
    let mut forward = [100, 100, 100, 255];
    let mut reverse = forward;
    let high = [200, 200, 200, 255];
    let low = [50, 50, 50, 255];

    assert!(blend_rgba_frame(&mut forward, &high, BlendMode::Subtract));
    assert!(blend_rgba_frame(&mut forward, &low, BlendMode::Subtract));
    assert!(blend_rgba_frame(&mut reverse, &low, BlendMode::Subtract));
    assert!(blend_rgba_frame(&mut reverse, &high, BlendMode::Subtract));

    assert_eq!(forward, [0, 0, 0, 255]);
    assert_eq!(reverse, [200, 200, 200, 255]);
}

#[test]
fn blend_rgba_frame_rejects_mismatched_or_incomplete_buffers_without_mutation() {
    let mut base = [1, 2, 3, 4];
    assert!(!blend_rgba_frame(
        &mut base,
        &[5, 6, 7],
        BlendMode::Addition
    ));
    assert_eq!(base, [1, 2, 3, 4]);

    assert!(!blend_rgba_frame(
        &mut base,
        &[5, 6, 7, 8, 9, 10, 11, 12],
        BlendMode::Addition
    ));
    assert_eq!(base, [1, 2, 3, 4]);
}
