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

// `extract_first_json_object` is a private helper, so unlike the rest of this module's tests
// (see `core/tests/loudness_test.rs` for those — they only exercise the public API), this
// one has to stay a unit test: an integration test in `tests/` can't see non-`pub` items.
#[test]
fn extract_first_json_object_ignores_unbalanced_braces_before_it() {
    let text = "log } line\n{\"a\":1}\nmore text";
    assert_eq!(extract_first_json_object(text), Some("{\"a\":1}"));
}
