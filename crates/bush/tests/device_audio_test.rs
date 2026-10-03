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
fn recover_reopens_the_stream_and_keeps_the_clock_counting() {
    let Some(mut sink) = open(48_000) else {
        return;
    };
    assert!(!sink.is_failed());
    sink.write(&silence(&sink, 4_000)).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    let consumed_before = sink.consumed_frames();
    assert!(consumed_before > 0);

    match sink.recover() {
        Ok(()) => {}
        // The default device changed format between open and recover; nothing to assert.
        Err(DeviceSinkError::FormatChanged { .. }) => return,
        Err(error) => panic!("recover failed: {error:?}"),
    }
    assert_eq!(sink.recoveries(), 1);
    assert!(!sink.is_failed());
    assert_eq!(sink.buffered_frames(), 0, "recovery must empty the queue");

    sink.write(&silence(&sink, 4_000)).unwrap();
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        sink.consumed_frames() > consumed_before,
        "the reopened stream never pulled audio"
    );
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

#[test]
fn video_follows_the_device_audio_clock_without_drifting_ahead() {
    use bush::{DecodePolicy, DecodeSource, Engine, EngineConfig, Player, RendererChoice};

    let Some(mut sink) = open(48_000 * 2) else {
        return;
    };
    let video =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4");
    let info = DecodeSource::open(&video).unwrap().info();
    let scene = |audible: bool| {
        SceneSnapshot::new(
            SceneVersions::default(),
            vec![Contributor {
                id: ContributorId(1),
                layer: 0,
                visible: true,
                audible,
                content: ContributorContent::Media(MediaSource {
                    path: video.clone(),
                    range: SourceRange::new(0.0, 0.9).unwrap(),
                    version: Version(1),
                }),
                placement: Placement::default(),
            }],
        )
        .unwrap()
    };
    let engine = Engine::new(EngineConfig {
        canvas_width: info.width,
        canvas_height: info.height,
        max_open_sources: 1,
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap();
    let mut player = Player::new(engine, scene(false), 3.0).unwrap();
    // Silent audio scene: the device still runs its real clock while we feed it zeros.
    let renderer = AudioRenderer::new(sink.sample_rate(), sink.channels(), 4096, 1).unwrap();
    let mut audio = AudioPlayer::new(
        renderer,
        scene(false),
        1024,
        sink.sample_rate() as usize / 4,
    );

    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    player.play(started.elapsed()).unwrap();
    let mut worst_drift: f64 = 0.0;
    while started.elapsed() < Duration::from_millis(1_500) {
        audio.pump(&mut sink, &cancel).unwrap();
        let audio_position = sink.consumed_frames() as f64 / f64::from(sink.sample_rate());
        let _ = player
            .tick_synced(started.elapsed(), audio_position)
            .unwrap();
        worst_drift = worst_drift.max(player.drift_against(audio_position).abs());
        std::thread::sleep(Duration::from_millis(8));
    }
    let audio_position = sink.consumed_frames() as f64 / f64::from(sink.sample_rate());
    let wall = started.elapsed().as_secs_f64();
    eprintln!(
        "A/V sync: audio clock {audio_position:.3} s, video {:.3} s, wall {wall:.3} s, worst drift {:.1} ms, underflows {}",
        player.position_secs(),
        worst_drift * 1000.0,
        sink.underflow_callbacks()
    );
    // Video tracks the audio clock exactly (it is the master), and the audio clock keeps pace
    // with the wall clock.
    assert!(worst_drift < 0.001, "video led audio by {worst_drift} s");
    assert!(audio_position > wall * 0.7 && audio_position < wall * 1.05);
}

/// Long-run A/V sync soak against the real device. Ignored by default (it runs for
/// `BUSH_SOAK_SECS`, default 60): `cargo test -p bush --features cpal --test device_audio_test
/// long_run -- --ignored --nocapture`.
#[test]
#[ignore = "long-running device soak"]
fn long_run_audio_clock_tracks_wall_time_and_video_never_leads() {
    use bush::{DecodePolicy, DecodeSource, Engine, EngineConfig, Player, RendererChoice};

    let Some(mut sink) = open(48_000 * 2) else {
        return;
    };
    let seconds: u64 = std::env::var("BUSH_SOAK_SECS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);
    let video =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4");
    let info = DecodeSource::open(&video).unwrap().info();
    let scene = SceneSnapshot::new(
        SceneVersions::default(),
        vec![Contributor {
            id: ContributorId(1),
            layer: 0,
            visible: true,
            audible: false,
            content: ContributorContent::Media(MediaSource {
                path: video,
                range: SourceRange::new(0.0, 0.9).unwrap(),
                version: Version(1),
            }),
            placement: Placement::default(),
        }],
    )
    .unwrap();
    let engine = Engine::new(EngineConfig {
        canvas_width: info.width,
        canvas_height: info.height,
        max_open_sources: 1,
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap();
    let mut player = Player::new(engine, scene.clone(), seconds as f64 + 10.0).unwrap();
    let renderer = AudioRenderer::new(sink.sample_rate(), sink.channels(), 4096, 1).unwrap();
    let mut audio = AudioPlayer::new(renderer, scene, 1024, sink.sample_rate() as usize / 4);

    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    player.play(started.elapsed()).unwrap();
    let mut worst_lead: f64 = 0.0;
    let mut next_report = 10.0;
    while started.elapsed() < Duration::from_secs(seconds) {
        audio.pump(&mut sink, &cancel).unwrap();
        let audio_position = sink.consumed_frames() as f64 / f64::from(sink.sample_rate());
        let _ = player
            .tick_synced(started.elapsed(), audio_position)
            .unwrap();
        worst_lead = worst_lead.max(player.drift_against(audio_position));
        let wall = started.elapsed().as_secs_f64();
        if wall >= next_report {
            eprintln!(
                "t={wall:6.1}s audio {audio_position:8.3}s offset {:+6.1} ms underflows {}",
                (audio_position - wall) * 1000.0,
                sink.underflow_callbacks()
            );
            next_report += 10.0;
        }
        std::thread::sleep(Duration::from_millis(8));
    }
    let wall = started.elapsed().as_secs_f64();
    let audio_position = sink.consumed_frames() as f64 / f64::from(sink.sample_rate());
    let ppm = (audio_position - wall) / wall * 1.0e6;
    eprintln!(
        "soak {wall:.1}s: audio clock {audio_position:.3}s, relative rate error {ppm:+.0} ppm, \
         worst video lead {:.2} ms, underflow callbacks {}",
        worst_lead * 1000.0,
        sink.underflow_callbacks()
    );
    assert!(
        worst_lead < 0.001,
        "video led the audio clock by {worst_lead} s"
    );
    // The audio clock may sit slightly behind wall time (startup latency) but must not run
    // away: within 0.5 s absolute over the soak.
    assert!(
        (audio_position - wall).abs() < 0.5,
        "audio clock offset {} s",
        audio_position - wall
    );
}
