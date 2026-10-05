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

fn frame(fill: u8) -> Vec<u8> {
    vec![fill; 4]
}

#[test]
fn no_padding_when_clip_spans_the_whole_canvas() {
    let clip_frames = vec![frame(255), frame(255)];
    let padded = pad_matte_frames_to_canvas_duration(&clip_frames, 4, 1, 1, 0.0, 10.0, 10.0);
    assert_eq!(padded, clip_frames);
}

#[test]
fn pads_before_when_clip_starts_partway_through() {
    // Canvas is 10s at 1fps; clip starts at 5s -> 5 black frames before.
    let clip_frames = vec![frame(255)];
    let padded = pad_matte_frames_to_canvas_duration(&clip_frames, 4, 1, 1, 5.0, 5.0, 10.0);
    assert_eq!(padded.len(), 6);
    for f in &padded[0..5] {
        assert_eq!(f, &frame(0));
    }
    assert_eq!(padded[5], frame(255));
}

#[test]
fn pads_after_when_clip_ends_before_canvas_end() {
    let clip_frames = vec![frame(255)];
    let padded = pad_matte_frames_to_canvas_duration(&clip_frames, 4, 1, 1, 0.0, 3.0, 10.0);
    assert_eq!(padded.len(), 8);
    assert_eq!(padded[0], frame(255));
    for f in &padded[1..8] {
        assert_eq!(f, &frame(0));
    }
}

#[test]
fn pads_both_sides_when_clip_is_in_the_middle() {
    let clip_frames = vec![frame(255), frame(255)];
    let padded = pad_matte_frames_to_canvas_duration(&clip_frames, 4, 1, 1, 3.0, 2.0, 10.0);
    // 3 before, 2 clip frames, 5 after = 10 total.
    assert_eq!(padded.len(), 10);
    for f in &padded[0..3] {
        assert_eq!(f, &frame(0));
    }
    assert_eq!(padded[3], frame(255));
    assert_eq!(padded[4], frame(255));
    for f in &padded[5..10] {
        assert_eq!(f, &frame(0));
    }
}

#[test]
fn clamps_a_clip_start_past_the_canvas_end_without_underflow() {
    let clip_frames = vec![frame(255)];
    // clip_start_secs (20.0) is beyond canvas_duration_secs (10.0) -- must not panic.
    let padded = pad_matte_frames_to_canvas_duration(&clip_frames, 4, 1, 1, 20.0, 5.0, 10.0);
    assert_eq!(padded.len(), 11); // 10 black frames before (clamped start) + 1 clip frame
    for f in &padded[0..10] {
        assert_eq!(f, &frame(0));
    }
}

#[test]
fn clamps_a_negative_clip_start_to_zero() {
    let clip_frames = vec![frame(255)];
    let padded = pad_matte_frames_to_canvas_duration(&clip_frames, 4, 1, 1, -5.0, 5.0, 10.0);
    assert_eq!(padded.len(), 6); // 0 before + 1 clip frame + 5 after
    assert_eq!(padded[0], frame(255));
}

#[test]
fn zero_fps_den_produces_no_padding_rather_than_dividing_by_zero() {
    let clip_frames = vec![frame(255)];
    let padded = pad_matte_frames_to_canvas_duration(&clip_frames, 4, 1, 0, 5.0, 2.0, 10.0);
    assert_eq!(padded, clip_frames);
}

#[test]
fn empty_clip_frames_produces_only_padding() {
    let padded = pad_matte_frames_to_canvas_duration(&[], 4, 1, 1, 2.0, 3.0, 10.0);
    assert_eq!(padded.len(), 7); // 2 before + 0 clip frames + 5 after
    for f in &padded {
        assert_eq!(f, &frame(0));
    }
}
