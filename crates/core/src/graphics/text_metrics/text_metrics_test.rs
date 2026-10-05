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
fn word_byte_ranges_follow_repeated_words_across_whitespace_and_lines() {
    assert_eq!(
        word_byte_ranges("go  go\nnow", &["go", "go", "now"]),
        vec![Some([0, 2]), Some([4, 6]), Some([7, 10])]
    );
}

#[test]
fn word_byte_ranges_leave_stale_timing_words_unmatched() {
    assert_eq!(
        word_byte_ranges("edited caption", &["edited", "old", "caption"]),
        vec![Some([0, 6]), None, Some([7, 14])]
    );
}
