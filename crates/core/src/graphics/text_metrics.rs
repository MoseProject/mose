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

//! Text/word byte-range mapping — [`word_byte_ranges`] locates each timed/transcribed word inside
//! a caption string, pure string search independent of whichever engine actually shapes and
//! rasterizes the glyphs ([`crate::text_layout`], see that module's own doc comment).
//!
//! TEXT-01A's `cosmic-text` swap (`spec/architecture/complex-text-shaping.md`) moved
//! [`crate::overlay_render`]'s actual glyph placement/rasterization onto [`crate::text_layout`],
//! which left this module's own per-character `fontdue`-based advance-width measurement
//! (`text_width_px`/`word_x_offsets_px`/`bundled_font`) with no caller outside its own tests —
//! removed here (TEXT-01B step 4, "replace approximate word-width helpers with cluster/run
//! geometry") along with the now-unused `fontdue` dependency, since `TextLayoutEngine::shape`'s
//! real shaped-glyph geometry is that replacement and has been the only measurement path actually
//! wired to pixel-affecting output since TEXT-01A shipped.

/// Locates each timed/transcribed word inside the exact caption string, in order, returning UTF-8
/// byte ranges — matched against [`crate::text_layout::ShapedGlyph::cluster`] by
/// `crate::overlay_render::glyph_excluded`. Searching continues after the previous match, so
/// repeated words map to their own occurrence and arbitrary whitespace/newlines in the edited
/// caption remain intact. A word that no longer appears after the previous match returns `None`
/// instead of drawing a highlight over unrelated text — useful when a user edits the caption
/// without regenerating its word timings.
pub fn word_byte_ranges(text: &str, words: &[&str]) -> Vec<Option<[usize; 2]>> {
    let mut search_from = 0usize;
    words
        .iter()
        .map(|word| {
            if word.is_empty() || search_from > text.len() {
                return None;
            }
            let relative_start = text[search_from..].find(word)?;
            let start = search_from + relative_start;
            let end = start + word.len();
            search_from = end;
            Some([start, end])
        })
        .collect()
}

#[cfg(test)]
#[path = "text_metrics/text_metrics_test.rs"]
mod tests;
