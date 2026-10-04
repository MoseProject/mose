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

use std::path::{Path, PathBuf};
use std::time::Duration;

use avcore::FrameSampler;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn sample_decodes_a_frame_at_the_requested_time() {
    let sampler = FrameSampler::open(&fixture("video.mp4")).unwrap();
    let frame = sampler
        .sample(0.0, Duration::from_millis(2000))
        .expect("a real fixture should decode within the deadline");
    assert!(frame.width > 0 && frame.height > 0);
    assert_eq!(frame.rgba.len(), (frame.width * frame.height * 4) as usize);
}

#[test]
fn sample_can_be_called_repeatedly_against_the_same_open_pipeline() {
    let sampler = FrameSampler::open(&fixture("video.mp4")).unwrap();
    let first = sampler
        .sample(0.0, Duration::from_millis(2000))
        .expect("first sample should decode");
    let second = sampler
        .sample(0.2, Duration::from_millis(2000))
        .expect("second sample, same sampler, should also decode");
    assert_eq!(first.width, second.width);
    assert_eq!(first.height, second.height);
}

#[test]
fn open_fails_for_a_nonexistent_source() {
    assert!(FrameSampler::open(&fixture("does_not_exist.mp4")).is_err());
}

#[test]
fn different_times_give_different_pictures_and_match_a_direct_decode() {
    let sampler = FrameSampler::open(&fixture("video.mp4")).unwrap();
    let early = sampler.sample(0.1, Duration::from_secs(10)).unwrap();
    let late = sampler.sample(0.8, Duration::from_secs(10)).unwrap();
    assert_ne!(
        early.rgba, late.rgba,
        "the fixture moves, so 0.1 s and 0.8 s differ"
    );

    // The sampler returns exactly the frame the preview's decoder produces for that time.
    let mut direct = bush::DecodeSource::open(&fixture("video.mp4")).unwrap();
    let never_cancelled = std::sync::atomic::AtomicBool::new(false);
    let expected = direct.frame_at(0.8, &never_cancelled).unwrap().unwrap();
    assert_eq!(late.rgba.as_slice(), &expected.pixels()[..late.rgba.len()]);
}

#[test]
fn sampling_backwards_and_past_the_end_does_not_panic() {
    let sampler = FrameSampler::open(&fixture("video.mp4")).unwrap();
    let late = sampler.sample(0.9, Duration::from_secs(10)).unwrap();
    let back = sampler.sample(0.1, Duration::from_secs(10)).unwrap();
    assert_ne!(
        late.rgba, back.rgba,
        "seeking backwards must change the picture"
    );
    // Far past the end of a one second clip: the last frame or nothing, never a panic.
    if let Some(last) = sampler.sample(1_000.0, Duration::from_secs(10)) {
        assert_eq!(last.width, late.width);
    }
    // Negative times clamp to the start.
    assert!(sampler.sample(-5.0, Duration::from_secs(10)).is_some());
}

#[test]
fn an_expired_deadline_skips_the_sample() {
    let sampler = FrameSampler::open(&fixture("video.mp4")).unwrap();
    assert!(sampler.sample(0.5, Duration::ZERO).is_none());
}
