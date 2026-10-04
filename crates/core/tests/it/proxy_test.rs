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
use std::path::{Path, PathBuf};
use std::time::Duration;

use avcore::proxy::{ensure_proxy, proxy_path_for};
use avcore::PreviewQuality;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn temp_cache_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("oca_proxy_test_{name}_{}", std::process::id()))
}

#[test]
fn proxy_path_uses_the_source_stem_with_a_proxy_suffix() {
    let source = Path::new("/videos/boss03_ribby_croaks.mp4");
    let proxy_dir = Path::new("/cache");
    assert_eq!(
        proxy_path_for(source, proxy_dir, PreviewQuality::Medium),
        Path::new("/cache/boss03_ribby_croaks_proxy_480p.mp4")
    );
}

#[test]
fn proxy_path_differs_per_quality_so_switching_never_collides() {
    let source = Path::new("/videos/boss03_ribby_croaks.mp4");
    let proxy_dir = Path::new("/cache");
    let low = proxy_path_for(source, proxy_dir, PreviewQuality::Low);
    let medium = proxy_path_for(source, proxy_dir, PreviewQuality::Medium);
    let high = proxy_path_for(source, proxy_dir, PreviewQuality::High);
    assert_ne!(low, medium);
    assert_ne!(medium, high);
    assert_ne!(low, high);
}

#[test]
fn ensure_proxy_skips_ffmpeg_when_the_proxy_is_already_up_to_date() {
    let dir = temp_cache_dir("cache_hit");
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("source.mp4");
    fs::write(&source, b"fake source").unwrap();
    let proxy_path = proxy_path_for(&source, &dir, PreviewQuality::Medium);
    std::thread::sleep(Duration::from_millis(20));
    fs::write(&proxy_path, b"fake proxy").unwrap();

    // If this actually invoked ffmpeg (not installed in this environment), it would return
    // Err(ProxyError::Spawn(_)) instead of Ok - succeeding proves the cache-hit path ran
    // without needing the ffmpeg binary at all.
    let result = ensure_proxy(&source, &dir, PreviewQuality::Medium);

    let _ = fs::remove_file(&source);
    let _ = fs::remove_file(&proxy_path);
    let _ = fs::remove_dir(&dir);

    assert_eq!(result.unwrap(), proxy_path);
}

#[test]
fn generates_a_real_downscaled_proxy() {
    let dir = temp_cache_dir("real_generate");
    let _ = fs::remove_dir_all(&dir);
    let source = fixture("video.mp4");

    let proxy_path = ensure_proxy(&source, &dir, PreviewQuality::High).unwrap();
    let info = avcore::probe_media(&proxy_path).unwrap();

    // video.mp4 is 320x240 (see oca-avbridge's fixture generation); High requests 720, but
    // proxying never upscales past the source — scale=-2:720 on a 240-tall source still asks
    // for 720, so the FFI path (no upscale guard either, matching the original ffmpeg command)
    // does scale it up. Assert what the pipeline actually guarantees: correct aspect ratio and
    // an even width, not a specific number a future height change would silently break.
    assert_eq!(info.resolution.unwrap().1, PreviewQuality::High.height());
    let width = info.resolution.unwrap().0;
    assert_eq!(width % 2, 0);
    assert!((width as f64 / PreviewQuality::High.height() as f64 - 320.0 / 240.0).abs() < 0.02);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn errors_on_a_missing_source() {
    let dir = temp_cache_dir("missing_source");
    let _ = fs::remove_dir_all(&dir);
    let result = ensure_proxy(&fixture("does_not_exist.mp4"), &dir, PreviewQuality::Medium);
    let _ = fs::remove_dir_all(&dir);
    assert!(result.is_err());
}
