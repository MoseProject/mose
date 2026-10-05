use super::*;

#[test]
fn is_known_state_transition_accepts_every_allowlisted_name() {
    for &name in STATE_TRANSITIONS {
        assert!(is_known_state_transition(name));
    }
}

#[test]
fn is_known_state_transition_rejects_anything_not_allowlisted() {
    assert!(!is_known_state_transition("made_up_transition"));
    assert!(!is_known_state_transition(""));
}

#[test]
fn floor_char_boundary_is_a_no_op_when_the_limit_is_past_the_end() {
    assert_eq!(floor_char_boundary("hello", 100), 5);
    assert_eq!(floor_char_boundary("hello", 5), 5);
}

#[test]
fn floor_char_boundary_leaves_a_pure_ascii_limit_untouched() {
    assert_eq!(floor_char_boundary("hello world", 5), 5);
}

#[test]
fn floor_char_boundary_floors_down_out_of_a_multi_byte_character() {
    // "é" is 2 bytes (0xC3 0xA9); byte_limit=1 lands mid-character and must floor to 0.
    let s = "é";
    assert_eq!(s.len(), 2);
    assert_eq!(floor_char_boundary(s, 1), 0);
}

#[test]
fn floor_char_boundary_handles_a_four_byte_emoji() {
    // "🎬" is 4 bytes; a limit landing anywhere inside it must floor to its start (0).
    let s = "🎬x";
    assert_eq!(floor_char_boundary(s, 1), 0);
    assert_eq!(floor_char_boundary(s, 2), 0);
    assert_eq!(floor_char_boundary(s, 3), 0);
    // The limit exactly at the boundary after the emoji is untouched.
    assert_eq!(floor_char_boundary(s, 4), 4);
}

#[test]
fn field_nonempty_ok_rejects_empty() {
    assert!(!field_nonempty_ok(""));
}

#[test]
fn field_nonempty_ok_rejects_whitespace() {
    assert!(!field_nonempty_ok("has space"));
    assert!(!field_nonempty_ok("tab\tchar"));
}

#[test]
fn field_nonempty_ok_accepts_a_normal_short_value() {
    assert!(field_nonempty_ok("normal-value_123"));
}

#[test]
fn field_nonempty_ok_boundary_at_128_bytes() {
    let exactly_128 = "a".repeat(128);
    let over_128 = "a".repeat(129);
    assert!(field_nonempty_ok(&exactly_128));
    assert!(!field_nonempty_ok(&over_128));
}

#[test]
fn id_ok_rejects_empty() {
    assert!(!id_ok(""));
}

#[test]
fn id_ok_rejects_non_alphanumeric_characters() {
    assert!(!id_ok("has-dash"));
    assert!(!id_ok("has_underscore"));
    assert!(!id_ok("has space"));
    assert!(!id_ok("has.dot"));
}

#[test]
fn id_ok_accepts_plain_alphanumeric() {
    assert!(id_ok("abc123XYZ"));
}

#[test]
fn id_ok_boundary_at_64_bytes() {
    let exactly_64 = "a".repeat(64);
    let over_64 = "a".repeat(65);
    assert!(id_ok(&exactly_64));
    assert!(!id_ok(&over_64));
}

#[test]
fn splitmix64_is_deterministic() {
    assert_eq!(splitmix64(42), splitmix64(42));
    assert_eq!(splitmix64(0), splitmix64(0));
}

#[test]
fn splitmix64_produces_different_output_for_different_input() {
    assert_ne!(splitmix64(1), splitmix64(2));
    assert_ne!(splitmix64(0), splitmix64(1));
}

#[test]
fn splitmix64_does_not_panic_on_extreme_inputs() {
    let _ = splitmix64(0);
    let _ = splitmix64(u64::MAX);
}

#[test]
fn format_ffmpeg_version_unpacks_major_minor_micro() {
    let packed = (7u32 << 16) | (1u32 << 8) | 2u32;
    assert_eq!(format_ffmpeg_version(packed), "7.1.2");
}

#[test]
fn format_ffmpeg_version_handles_zero() {
    assert_eq!(format_ffmpeg_version(0), "0.0.0");
}

#[test]
fn format_ffmpeg_version_handles_the_maximum_byte_per_component() {
    let packed = (255u32 << 16) | (255u32 << 8) | 255u32;
    assert_eq!(format_ffmpeg_version(packed), "255.255.255");
}

#[test]
fn format_ffmpeg_version_only_masks_minor_and_micro_not_major() {
    // Only minor/micro are masked with `& 0xff`; major is `packed >> 16` unmasked. The one real
    // caller (`avbridge::version()`) never sets bits above 23, so this is documenting actual
    // behavior, not asserting a safety property.
    let packed = 0xFF_07_01_02u32;
    assert_eq!(format_ffmpeg_version(packed), "65287.1.2");
}
