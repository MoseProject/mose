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

fn solid_rgba(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut buf = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        buf.extend_from_slice(&[r, g, b, 255]);
    }
    buf
}

#[test]
fn luma_waveform_puts_a_solid_white_field_at_the_top_row() {
    let src = solid_rgba(4, 4, 255, 255, 255);
    let out = luma_waveform_rgba(&src, 4, 4, 4, 4);
    for col in 0..4usize {
        // Row 0 (brightest luma) is fully saturated for every column...
        let top = col * 4;
        assert_eq!(&out[top..top + 4], &[255, 255, 255, 255]);
        // ...and every other row in that column stays black (no samples landed there).
        for row in 1..4usize {
            let px = (row * 4 + col) * 4;
            assert_eq!(&out[px..px + 4], &[0, 0, 0, 255]);
        }
    }
}

#[test]
fn luma_waveform_puts_a_solid_black_field_at_the_bottom_row() {
    let src = solid_rgba(4, 4, 0, 0, 0);
    let out = luma_waveform_rgba(&src, 4, 4, 4, 4);
    for col in 0..4usize {
        let bottom = (3 * 4 + col) * 4;
        assert_eq!(&out[bottom..bottom + 4], &[255, 255, 255, 255]);
        for row in 0..3usize {
            let px = (row * 4 + col) * 4;
            assert_eq!(&out[px..px + 4], &[0, 0, 0, 255]);
        }
    }
}

#[test]
fn luma_waveform_is_empty_for_a_zero_sized_input_or_output() {
    assert!(luma_waveform_rgba(&[], 0, 0, 4, 4).is_empty());
    let src = solid_rgba(4, 4, 128, 128, 128);
    assert!(luma_waveform_rgba(&src, 4, 4, 0, 4).is_empty());
}

#[test]
fn scopes_reject_non_packed_rgba_buffers_without_reading_past_their_end() {
    let truncated = vec![255; 15];
    let padded = vec![255; 17];

    assert!(luma_waveform_rgba(&truncated, 2, 2, 4, 4).is_empty());
    assert!(vectorscope_rgba(&truncated, 2, 2, 5).is_empty());
    assert_eq!(
        render_scopes_rgba(&truncated, 2, 2, 4, 4, 5),
        (Vec::new(), Vec::new())
    );
    assert!(luma_waveform_rgba(&padded, 2, 2, 4, 4).is_empty());
    assert!(vectorscope_rgba(&padded, 2, 2, 5).is_empty());
}

#[test]
fn vectorscope_centers_a_neutral_gray_field() {
    let src = solid_rgba(4, 4, 128, 128, 128);
    let out = vectorscope_rgba(&src, 4, 4, 5);
    // Neutral gray (Cb=Cr=128) lands exactly on the center cell of an odd-sized grid.
    let center = (2 * 5 + 2) * 4;
    assert_eq!(&out[center..center + 4], &[255, 255, 255, 255]);
    let total: u32 = out.as_chunks::<4>().0.iter().map(|px| px[0] as u32).sum();
    assert_eq!(
        total, 255,
        "every sample should land in the single center cell, nowhere else"
    );
}

#[test]
fn vectorscope_pushes_a_saturated_color_away_from_center() {
    let neutral = vectorscope_rgba(&solid_rgba(2, 2, 128, 128, 128), 2, 2, 9);
    let saturated_red = vectorscope_rgba(&solid_rgba(2, 2, 255, 0, 0), 2, 2, 9);
    let brightest_cell = |img: &[u8]| {
        img.as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .max_by_key(|(_, px)| px[0])
            .map(|(i, _)| i)
            .unwrap()
    };
    assert_ne!(
        brightest_cell(&neutral),
        brightest_cell(&saturated_red),
        "a strongly saturated color must trace away from the neutral-gray center cell"
    );
}

#[test]
fn vectorscope_is_empty_for_a_zero_sized_input_or_output() {
    assert!(vectorscope_rgba(&[], 0, 0, 5).is_empty());
    let src = solid_rgba(4, 4, 128, 128, 128);
    assert!(vectorscope_rgba(&src, 4, 4, 0).is_empty());
}

/// A non-uniform frame (not just a solid color) so both scopes' output actually varies across
/// cells — a solid-color input's own trivial output could match by coincidence even with a real
/// bug in the combined pass's shared iteration.
fn varied_rgba(width: u32, height: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let r = ((x * 37 + y * 19) % 256) as u8;
            let g = ((x * 11 + y * 53) % 256) as u8;
            let b = ((x * 71 + y * 5) % 256) as u8;
            buf.extend_from_slice(&[r, g, b, 255]);
        }
    }
    buf
}

#[test]
fn render_scopes_matches_calling_both_standalone_functions() {
    let src = varied_rgba(37, 23); // deliberately not a power of two or square
    let (combined_waveform, combined_vectorscope) = render_scopes_rgba(&src, 37, 23, 17, 11, 13);
    let separate_waveform = luma_waveform_rgba(&src, 37, 23, 17, 11);
    let separate_vectorscope = vectorscope_rgba(&src, 37, 23, 13);
    assert_eq!(combined_waveform, separate_waveform);
    assert_eq!(combined_vectorscope, separate_vectorscope);
}

#[test]
fn render_scopes_is_empty_for_a_zero_sized_source() {
    let (waveform, vectorscope) = render_scopes_rgba(&[], 0, 0, 4, 4, 5);
    assert!(waveform.is_empty());
    assert!(vectorscope.is_empty());
}

#[test]
fn render_scopes_skips_only_the_zero_sized_output_side() {
    let src = varied_rgba(4, 4);
    let (waveform, vectorscope) = render_scopes_rgba(&src, 4, 4, 0, 4, 5);
    assert!(waveform.is_empty(), "waveform output was zero-sized");
    assert!(
        !vectorscope.is_empty(),
        "vectorscope output was requested and should still render"
    );
}
