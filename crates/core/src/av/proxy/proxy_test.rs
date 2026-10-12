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

#[test]
fn a_proxy_keeps_the_colour_of_hd_footage_even_though_it_is_small() {
    // A 480p proxy of BT.709 footage must still be read as BT.709, not as SD (BT.601).
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/color/hd709_untagged.mkv");
    let dir = std::env::temp_dir().join(format!("oca-proxy-color-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let proxy = ensure_proxy(&source, &dir, PreviewQuality::Medium).expect("a proxy");
    let frame = crate::FrameSampler::open(&proxy)
        .expect("opens")
        .sample(0.0, Duration::from_secs(5))
        .expect("a frame");
    let at = ((frame.height / 2 * frame.width + frame.width / 2) * 4) as usize;
    let pixel = [frame.rgba[at], frame.rgba[at + 1], frame.rgba[at + 2]];
    let _ = fs::remove_dir_all(&dir);
    for (channel, expected) in [200i32, 60, 40].into_iter().enumerate() {
        assert!(
            (i32::from(pixel[channel]) - expected).abs() <= 6,
            "the proxy shows {pixel:?}, the colour is 200, 60, 40"
        );
    }
}

fn audio_fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/audio_codecs")
        .join(name)
}

#[test]
fn a_proxy_is_made_for_flac_opus_and_pcm_audio_in_matroska_and_keeps_its_length() {
    // Matroska stamps audio in milliseconds, and PCM in it has no channel layout: each of these
    // used to fail (timestamps going backwards, or an encoder refusing "2 channels").
    for name in ["flac_in_mkv.mkv", "opus_in_mkv.mkv", "pcm_in_mkv.mkv"] {
        let source = audio_fixture(name);
        let dir =
            std::env::temp_dir().join(format!("oca-proxy-audio-{name}-{}", std::process::id()));
        let proxy = ensure_proxy(&source, &dir, PreviewQuality::Low)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        let made = crate::probe_media(&proxy).expect("the proxy probes");
        let original = crate::probe_media(&source).expect("the source probes");
        let _ = fs::remove_dir_all(&dir);
        assert!(made.has_audio, "{name}: the proxy lost its audio");
        assert!(
            (made.duration_secs - original.duration_secs).abs() < 0.15,
            "{name}: {} s became {} s",
            original.duration_secs,
            made.duration_secs
        );
    }
}

#[test]
fn a_failed_proxy_leaves_nothing_behind_to_be_taken_for_a_good_one() {
    let dir = std::env::temp_dir().join(format!("oca-proxy-failed-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let not_media = dir.join("clip.mp4");
    fs::write(&not_media, b"this is not a video").unwrap();
    let proxies = dir.join("proxies");

    assert!(ensure_proxy(&not_media, &proxies, PreviewQuality::Medium).is_err());
    let leftovers: Vec<_> = fs::read_dir(&proxies)
        .map(|entries| entries.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    let again = ensure_proxy(&not_media, &proxies, PreviewQuality::Medium);
    let _ = fs::remove_dir_all(&dir);
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    assert!(
        again.is_err(),
        "the next call took a leftover for a finished proxy"
    );
}

#[test]
fn the_proxy_of_an_hdr_clip_is_sdr_with_the_same_colour() {
    // 10-bit PQ BT.2020 in, plain Rec.709 8-bit out: scrubbing never needs the tone mapping.
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/color/hdr10_pq.mkv");
    let dir = std::env::temp_dir().join(format!("oca-proxy-hdr-{}", std::process::id()));
    let proxy = ensure_proxy(&source, &dir, PreviewQuality::Medium).expect("a proxy");
    let color = crate::probe_media(&proxy)
        .expect("probes")
        .streams
        .color
        .expect("a video proxy");
    assert!(!color.is_hdr(), "the proxy is still HDR: {color:?}");
    assert_eq!(color.bit_depth, 8);
    let frame = crate::FrameSampler::open(&proxy)
        .expect("opens")
        .sample(0.0, Duration::from_secs(5))
        .expect("a frame");
    let at = ((frame.height / 2 * frame.width + frame.width / 2) * 4) as usize;
    let pixel = [frame.rgba[at], frame.rgba[at + 1], frame.rgba[at + 2]];
    let _ = fs::remove_dir_all(&dir);
    for (channel, expected) in [200i32, 60, 40].into_iter().enumerate() {
        assert!(
            (i32::from(pixel[channel]) - expected).abs() <= 12,
            "the proxy shows {pixel:?}, the colour is 200, 60, 40"
        );
    }
}

#[test]
fn a_treat_as_sdr_proxy_is_tagged_sdr_but_keeps_the_pixels_unconverted() {
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/color/hdr10_pq.mkv");
    let dir = std::env::temp_dir().join(format!("oca-proxy-asis-{}", std::process::id()));
    let mapped = ensure_proxy_as(&source, &dir, PreviewQuality::Medium, true).expect("a proxy");
    let as_is = ensure_proxy_as(&source, &dir, PreviewQuality::Medium, false).expect("a proxy");
    assert_ne!(mapped, as_is, "the two choices must not share a file");
    assert!(as_is.display().to_string().ends_with("_asis.mp4"));

    let center = |path: &Path| {
        let frame = crate::FrameSampler::open(path)
            .expect("opens")
            .sample(0.0, Duration::from_secs(5))
            .expect("a frame");
        let at = ((frame.height / 2 * frame.width + frame.width / 2) * 4) as usize;
        [frame.rgba[at], frame.rgba[at + 1], frame.rgba[at + 2]]
    };
    let color = crate::probe_media(&as_is)
        .expect("probes")
        .streams
        .color
        .expect("a video proxy");
    assert!(!color.is_hdr(), "the proxy is tagged HDR: {color:?}");
    let converted = center(&mapped);
    let untouched = center(&as_is);
    let _ = fs::remove_dir_all(&dir);
    let distance: i32 = (0..3)
        .map(|c| (i32::from(converted[c]) - i32::from(untouched[c])).abs())
        .sum();
    assert!(
        distance > 40,
        "the unconverted proxy looks like the converted one: {converted:?} against {untouched:?}"
    );
}
