// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use super::*;

#[test]
fn even_sample_times_spaces_samples_across_the_full_duration() {
    let times = FrameSampler::even_sample_times(2.0, 4.0, 2.0, 2, 60);
    // duration 2.0 * rate 2.0 = 4 samples, evenly spaced from the start up to (and including)
    // the end, with equal gaps between consecutive samples.
    assert_eq!(times.len(), 4);
    assert_eq!(*times.first().unwrap(), 2.0);
    assert_eq!(*times.last().unwrap(), 4.0);
    let gaps: Vec<f64> = times.windows(2).map(|w| w[1] - w[0]).collect();
    for gap in &gaps {
        assert!(
            (gap - 2.0 / 3.0).abs() < 1e-9,
            "expected an even ~0.667s gap, got {gap}"
        );
    }
}

#[test]
fn even_sample_times_clamps_to_min_samples_for_a_short_duration() {
    // duration 0.1 * rate 2.0 rounds to 0 samples, clamped up to the 2-sample floor every
    // existing caller (motion tracking, background removal) passes.
    let times = FrameSampler::even_sample_times(0.0, 0.1, 2.0, 2, 60);
    assert_eq!(times.len(), 2);
    assert_eq!(times[0], 0.0);
    assert_eq!(times[1], 0.1);
}

#[test]
fn even_sample_times_clamps_to_max_samples_for_a_long_duration() {
    let times = FrameSampler::even_sample_times(0.0, 100.0, 4.0, 2, 60);
    assert_eq!(times.len(), 60);
    assert_eq!(*times.first().unwrap(), 0.0);
    assert_eq!(*times.last().unwrap(), 100.0);
}

#[test]
fn even_sample_times_is_empty_for_a_zero_or_negative_duration() {
    assert!(FrameSampler::even_sample_times(5.0, 5.0, 2.0, 2, 60).is_empty());
    assert!(FrameSampler::even_sample_times(5.0, 3.0, 2.0, 2, 60).is_empty());
}

#[test]
fn even_sample_times_does_not_divide_by_zero_for_a_single_sample() {
    // Not exercised by any current caller (both pass min_samples: 2), but a public primitive
    // shouldn't panic on a min_samples of 1 — sample_count == 1 would otherwise divide by
    // (sample_count - 1) == 0.
    let times = FrameSampler::even_sample_times(1.0, 1.0 + 1e-9, 1.0, 1, 1);
    assert_eq!(times, vec![1.0]);
}

#[test]
fn even_sample_times_honors_the_maximum_when_bounds_are_reversed() {
    let times = FrameSampler::even_sample_times(0.0, 10.0, 1.0, 10, 3);
    assert_eq!(times, vec![0.0, 5.0, 10.0]);
}

fn fixture() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/video.mp4")
}

#[test]
fn a_small_sample_is_converted_at_the_size_asked_for() {
    let sampler = FrameSampler::open(&fixture()).unwrap();
    let full = sampler.sample(0.5, Duration::from_secs(5)).unwrap();
    let small = sampler
        .sample_small(0.5, Duration::from_secs(5), 64, None)
        .unwrap();
    assert!(small.width.max(small.height) <= 64);
    assert!(
        small.width.max(small.height) >= 60,
        "not much smaller than asked"
    );
    // The same picture, only smaller, with its aspect ratio kept.
    let ratio = |frame: &SampledFrame| f64::from(frame.width) / f64::from(frame.height);
    assert!((ratio(&full) - ratio(&small)).abs() < 0.05);
    assert_eq!(
        small.rgba.len(),
        small.width as usize * small.height as usize * 4
    );
    // A size larger than the source leaves it as it is.
    let large = sampler
        .sample_small(0.5, Duration::from_secs(5), 100_000, None)
        .unwrap();
    assert_eq!((large.width, large.height), (full.width, full.height));
}

#[test]
fn a_sample_with_a_spent_budget_still_comes_back_and_is_not_after_the_target() {
    let sampler = FrameSampler::open(&fixture()).unwrap();
    // A budget of nothing: the first frame decoded stands in, instead of failing or waiting.
    let rough = sampler
        .sample_small(0.9, Duration::from_secs(5), 64, Some(Duration::ZERO))
        .unwrap();
    assert!(rough.width > 0 && rough.height > 0);
    // The exact sample that follows is unaffected by it.
    let exact = sampler.sample(0.9, Duration::from_secs(5)).unwrap();
    let reference = FrameSampler::open(&fixture())
        .unwrap()
        .sample(0.9, Duration::from_secs(5))
        .unwrap();
    assert_eq!(exact, reference);
}
