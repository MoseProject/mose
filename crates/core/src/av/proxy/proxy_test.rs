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

use std::fs;
use std::time::Duration;

use super::*;

// `is_up_to_date` is a private helper, so unlike the rest of this module's tests (see
// `core/tests/proxy_test.rs` for those — they only exercise the public API), these have to
// stay unit tests: an integration test in `tests/` can't see non-`pub` items.

#[test]
fn missing_proxy_is_not_up_to_date() {
    let dir = std::env::temp_dir().join(format!(
        "oca_proxy_unit_test_missing_{}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.mp4");
    fs::write(&source, b"src").unwrap();
    let proxy_path = dir.join("does_not_exist_proxy.mp4");

    let result = is_up_to_date(&source, &proxy_path);

    let _ = fs::remove_file(&source);
    let _ = fs::remove_dir(&dir);
    assert!(!result);
}

#[test]
fn a_proxy_older_than_its_source_is_not_up_to_date() {
    let dir =
        std::env::temp_dir().join(format!("oca_proxy_unit_test_stale_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let proxy_path = dir.join("proxy.mp4");
    fs::write(&proxy_path, b"old proxy").unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let source = dir.join("source.mp4");
    fs::write(&source, b"newer source").unwrap();

    let result = is_up_to_date(&source, &proxy_path);

    let _ = fs::remove_file(&source);
    let _ = fs::remove_file(&proxy_path);
    let _ = fs::remove_dir(&dir);
    assert!(!result);
}

#[test]
fn a_proxy_newer_than_its_source_is_up_to_date() {
    let dir =
        std::env::temp_dir().join(format!("oca_proxy_unit_test_fresh_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.mp4");
    fs::write(&source, b"source").unwrap();
    std::thread::sleep(Duration::from_millis(20));
    let proxy_path = dir.join("proxy.mp4");
    fs::write(&proxy_path, b"fresh proxy").unwrap();

    let result = is_up_to_date(&source, &proxy_path);

    let _ = fs::remove_file(&source);
    let _ = fs::remove_file(&proxy_path);
    let _ = fs::remove_dir(&dir);
    assert!(result);
}
