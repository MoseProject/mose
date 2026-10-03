//! Failure containment and soak behaviour: hostile or damaged media must produce errors, never
//! panics or hangs, and the engine must stay usable afterwards.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use bush::{
    AudioSource, Contributor, ContributorContent, ContributorId, DecodePolicy, DecodeSource,
    Engine, EngineConfig, EngineError, EngineEvent, EngineRequest, MediaSource, Placement,
    SceneSnapshot, SceneVersions, SourceRange, Version, MAX_CANVAS_PIXELS,
};

const WAIT: Duration = Duration::from_secs(30);

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures")
}

fn video() -> PathBuf {
    fixtures().join("video.mp4")
}

fn scratch(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("bush-reliability");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn config() -> EngineConfig {
    let info = DecodeSource::open(&video()).unwrap().info();
    EngineConfig {
        canvas_width: info.width,
        canvas_height: info.height,
        max_open_sources: 2,
        frame_cache_bytes: 16 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
    }
}

fn request(path: &Path, playhead_secs: f64) -> EngineRequest {
    let contributor = Contributor {
        id: ContributorId(1),
        layer: 0,
        visible: true,
        audible: false,
        content: ContributorContent::Media(MediaSource {
            path: path.to_path_buf(),
            range: SourceRange::new(0.0, 0.9).unwrap(),
            version: Version(1),
        }),
        placement: Placement::default(),
    };
    EngineRequest {
        scene: SceneSnapshot::new(SceneVersions::default(), vec![contributor]).unwrap(),
        playhead_secs,
        overlays: Vec::new(),
    }
}

/// Submits and waits for the outcome; either a frame or a failure is acceptable, a timeout is
/// not.
fn settle(engine: &Engine, path: &Path, playhead_secs: f64) -> EngineEvent {
    engine.submit(request(path, playhead_secs)).unwrap();
    engine
        .wait(WAIT)
        .unwrap_or_else(|| panic!("engine hung on {}", path.display()))
}

fn assert_engine_still_serves_good_media(engine: &Engine) {
    match settle(engine, &video(), 0.3) {
        EngineEvent::Frame { frame, .. } => assert_eq!(frame.timestamp_secs(), 0.3),
        EngineEvent::Failed { message, .. } => panic!("engine did not recover: {message}"),
    }
}

#[test]
fn oversized_canvas_is_rejected_before_any_allocation() {
    let mut huge = config();
    huge.canvas_width = 16_384;
    huge.canvas_height = 16_384;
    assert!(huge.canvas_width as usize * huge.canvas_height as usize > MAX_CANVAS_PIXELS);
    assert_eq!(Engine::new(huge).unwrap_err(), EngineError::InvalidConfig);

    let mut overflow = config();
    overflow.canvas_width = u32::MAX;
    overflow.canvas_height = u32::MAX;
    assert_eq!(
        Engine::new(overflow).unwrap_err(),
        EngineError::InvalidConfig
    );
}

#[test]
fn latency_percentiles_are_recorded_and_ordered() {
    let engine = Engine::new(config()).unwrap();
    assert_eq!(engine.latency().samples, 0);

    for step in 0..20 {
        match settle(&engine, &video(), f64::from(step) / 25.0) {
            EngineEvent::Frame { .. } => {}
            EngineEvent::Failed { message, .. } => panic!("{message}"),
        }
    }

    let latency = engine.latency();
    assert_eq!(latency.samples, 20);
    assert!(latency.p50_micros <= latency.p95_micros);
    assert!(latency.p95_micros <= latency.p99_micros);
    assert!(latency.p99_micros <= latency.max_micros);
    assert!(latency.max_micros > 0);
}

#[test]
fn truncated_media_fails_or_degrades_without_hanging() {
    let bytes = std::fs::read(video()).unwrap();
    let engine = Engine::new(config()).unwrap();

    for (index, fraction) in [0.02, 0.1, 0.3, 0.6, 0.9].into_iter().enumerate() {
        let path = scratch(&format!("truncated-{index}.mp4"));
        std::fs::write(&path, &bytes[..(bytes.len() as f64 * fraction) as usize]).unwrap();
        // Any outcome is fine as long as it is delivered.
        let _ = settle(&engine, &path, 0.5);
        assert_engine_still_serves_good_media(&engine);
    }
}

#[test]
fn corrupted_media_is_contained() {
    let original = std::fs::read(video()).unwrap();
    let engine = Engine::new(config()).unwrap();
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;

    for round in 0..12 {
        let mut damaged = original.clone();
        for _ in 0..24 {
            // xorshift64: deterministic damage positions per round.
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let position = (state as usize) % damaged.len();
            damaged[position] ^= (state >> 32) as u8 | 1;
        }
        let path = scratch(&format!("corrupt-{round}.mp4"));
        std::fs::write(&path, &damaged).unwrap();
        let _ = settle(&engine, &path, 0.4);
    }
    assert_engine_still_serves_good_media(&engine);
}

#[test]
fn damaged_audio_does_not_panic_or_spin() {
    let bytes = std::fs::read(fixtures().join("audio.m4a")).unwrap();
    let cancel = AtomicBool::new(false);
    let mut state = 0xD1B5_4A32_D192_ED03_u64;

    for round in 0..10 {
        let mut damaged = bytes.clone();
        if round % 2 == 0 {
            damaged.truncate(bytes.len() * (round + 1) / 12);
        } else {
            for _ in 0..16 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let position = (state as usize) % damaged.len();
                damaged[position] ^= (state >> 24) as u8 | 1;
            }
        }
        let path = scratch(&format!("audio-{round}.m4a"));
        std::fs::write(&path, &damaged).unwrap();

        let started = Instant::now();
        if let Ok(mut source) = AudioSource::open(&path, 44_100, 1, 4096) {
            for start in [0_u64, 20_000, 5_000] {
                let _ = source.read(start, 2048, &cancel);
            }
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "audio read took {:?}",
            started.elapsed()
        );
    }
}

#[test]
fn repeated_engine_lifecycles_shut_down_cleanly() {
    let started = Instant::now();
    for round in 0..30 {
        let engine = Engine::new(config()).unwrap();
        for step in 0..3 {
            engine
                .submit(request(&video(), f64::from(step + round % 5) / 10.0))
                .unwrap();
        }
        // Half the rounds wait for the frame, half drop with work in flight.
        if round % 2 == 0 {
            assert!(engine.wait(WAIT).is_some());
        }
        drop(engine);
    }
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "lifecycle soak took {:?}",
        started.elapsed()
    );
}

#[test]
fn long_scrub_soak_stays_responsive_with_a_tiny_cache() {
    let info = DecodeSource::open(&video()).unwrap().info();
    let tiny_cache = info.width as usize * info.height as usize * 4 * 3;
    let engine = Engine::new(EngineConfig {
        frame_cache_bytes: tiny_cache,
        max_open_sources: 1,
        ..config()
    })
    .unwrap();

    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    for _ in 0..150 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let playhead = (state % 90) as f64 / 100.0;
        match settle(&engine, &video(), playhead) {
            EngineEvent::Frame { frame, .. } => assert_eq!(frame.timestamp_secs(), playhead),
            EngineEvent::Failed { message, .. } => panic!("{message}"),
        }
    }
    let latency = engine.latency();
    assert_eq!(latency.samples, 150);
    assert_eq!(engine.stats().failures, 0);
}
