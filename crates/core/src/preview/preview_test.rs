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
