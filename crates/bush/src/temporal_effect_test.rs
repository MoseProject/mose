use super::TemporalEffect;
use crate::{Deflicker, PixelFormat, Stabilizer, VideoFrame};

fn frame(grey: u8, timestamp: f64) -> VideoFrame {
    VideoFrame::new(
        PixelFormat::Rgba8,
        4,
        4,
        16,
        timestamp,
        [grey, grey, grey, 255].repeat(16),
    )
    .unwrap()
}

#[test]
fn unchanged_deflicker_smoothing_preserves_history_but_a_change_resets_it() {
    let mut effect = Deflicker::new(0.2).unwrap();
    effect.process(&frame(100, 0.0));
    let input = frame(160, 0.04);
    let corrected = effect.process(&input);
    assert_ne!(corrected, input);
    effect.set_smoothing(0.2);
    assert_eq!(effect.process(&input), corrected);
    effect.set_smoothing(0.4);
    assert_eq!(effect.process(&input), input);
}

#[test]
fn both_effects_apply_smoothing_through_the_shared_contract() {
    let mut stabilizer = Stabilizer::new(0.2).unwrap();
    let mut deflicker = Deflicker::new(0.2).unwrap();
    for effect in [&mut stabilizer as &mut dyn TemporalEffect, &mut deflicker] {
        effect.set_smoothing(0.7);
        assert_eq!(*effect.smoothing_mut(), 0.7);
        effect.set_smoothing(0.7);
        assert_eq!(*effect.smoothing_mut(), 0.7);
    }
}
