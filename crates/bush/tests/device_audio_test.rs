//! Real-device validation. Requires `--features cpal` and an output device; without one each test
//! reports that it was skipped instead of failing, so headless CI stays green.
#![cfg(feature = "cpal")]

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use bush::{
    AudioBlock, AudioPlayer, AudioRenderer, AudioSink, Contributor, ContributorContent,
    ContributorId, DeviceSink, DeviceSinkError, MediaSource, Placement, SceneSnapshot,
    SceneVersions, SinkError, SourceRange, Version,
};

fn open(capacity_frames: usize) -> Option<DeviceSink> {
    match DeviceSink::open_default(capacity_frames) {
        Ok(sink) => Some(sink),
        Err(DeviceSinkError::NoDevice) => {
            eprintln!("skipped: no default audio output device");
            None
        }
        Err(error) => {
            eprintln!("skipped: output device unavailable: {error:?}");
            None
        }
    }
}

fn silence(sink: &DeviceSink, frames: usize) -> AudioBlock {
    AudioBlock::new(
        sink.sample_rate(),
        sink.channels(),
        0.0,
        vec![0.0; frames * usize::from(sink.channels())],
    )
    .unwrap()
}

#[test]
fn device_consumes_audio_at_its_own_clock_rate() {
    let Some(mut sink) = open(96_000 * 2) else {
        return;
    };
    let rate = sink.sample_rate() as usize;
    sink.write(&silence(&sink, rate)).unwrap();

    let started = Instant::now();
    let before = sink.consumed_frames();
    std::thread::sleep(Duration::from_millis(600));
    let elapsed = started.elapsed().as_secs_f64();
    let consumed = (sink.consumed_frames() - before) as f64;

    let expected = elapsed * rate as f64;
    let drift = (consumed - expected) / expected;
    eprintln!(
        "device clock: {} Hz, consumed {consumed:.0} of expected {expected:.0} frames ({:+.2}% drift)",
        rate,
        drift * 100.0
    );
    // Wall-clock sleep jitter and callback granularity bound this; a stalled or double-speed
    // device would be far outside it.
    assert!(drift.abs() < 0.25, "device clock drift {drift}");
}

#[test]
fn starved_device_reports_underflow_callbacks() {
    let Some(sink) = open(48_000) else {
        return;
    };
    std::thread::sleep(Duration::from_millis(250));
    assert!(sink.underflow_callbacks() > 0);
    assert_eq!(sink.consumed_frames(), 0);
}

#[test]
fn sink_bounds_its_queue_and_clears_on_request() {
    let Some(mut sink) = open(4_800) else {
        return;
    };
    // Stay well ahead of the device so nothing drains while asserting on the queue.
    let block = silence(&sink, 4_000);
    sink.write(&block).unwrap();
    assert!(matches!(sink.write(&block), Err(SinkError::Overrun { .. })));
    sink.clear();
    assert_eq!(sink.buffered_frames(), 0);
    sink.write(&block).unwrap();
}

#[test]
fn player_streams_a_real_clip_to_the_device() {
    let Some(mut sink) = open(48_000 * 2) else {
        return;
    };
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/audio.m4a");
    let scene = SceneSnapshot::new(
        SceneVersions::default(),
        vec![Contributor {
            id: ContributorId(1),
            layer: 0,
            visible: false,
            audible: true,
            content: ContributorContent::Media(MediaSource {
                path: fixture,
                range: SourceRange::new(0.0, 0.9).unwrap(),
                version: Version(1),
            }),
            placement: Placement::default(),
        }],
    )
    .unwrap();
    let renderer = AudioRenderer::new(sink.sample_rate(), sink.channels(), 4096, 2).unwrap();
    let mut player = AudioPlayer::new(renderer, scene, 1024, sink.sample_rate() as usize / 2);

    let cancel = AtomicBool::new(false);
    let written = player.pump(&mut sink, &cancel).unwrap();
    assert!(written >= sink.sample_rate() as usize / 2);

    std::thread::sleep(Duration::from_millis(300));
    assert!(sink.consumed_frames() > 0, "the device never pulled audio");
}
