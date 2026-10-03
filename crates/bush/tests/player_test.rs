use std::path::PathBuf;
use std::time::Duration;

use bush::{
    Contributor, ContributorContent, ContributorId, DecodePolicy, DecodeSource, Engine,
    EngineConfig, MediaSource, Placement, PlaybackState, Player, PlayerError, PlayerEvent,
    RendererChoice, SceneSnapshot, SceneVersions, SourceRange, Version,
};

const WAIT: Duration = Duration::from_secs(20);

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4")
}

fn scene(opacity: f32) -> SceneSnapshot {
    SceneSnapshot::new(
        SceneVersions::default(),
        vec![Contributor {
            id: ContributorId(1),
            layer: 0,
            visible: true,
            audible: false,
            content: ContributorContent::Media(MediaSource {
                path: fixture(),
                range: SourceRange::new(0.0, 0.9).unwrap(),
                version: Version(1),
            }),
            placement: Placement {
                opacity,
                ..Placement::default()
            },
        }],
    )
    .unwrap()
}

fn player() -> Player {
    let info = DecodeSource::open(&fixture()).unwrap().info();
    let engine = Engine::new(EngineConfig {
        canvas_width: info.width,
        canvas_height: info.height,
        max_open_sources: 1,
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap();
    Player::new(engine, scene(1.0), 0.9).unwrap()
}

fn next_frame(player: &mut Player) -> (f64, bush::VideoFrame) {
    match player.wait(WAIT).expect("no player event") {
        PlayerEvent::Frame {
            position_secs,
            frame,
            ..
        } => (position_secs, frame),
        other => panic!("expected a frame, got {other:?}"),
    }
}

#[test]
fn paused_player_ignores_ticks() {
    let mut player = player();
    assert_eq!(player.tick(Duration::from_secs(5)).unwrap(), None);
    assert_eq!(player.position_secs(), 0.0);
    assert_eq!(player.state(), PlaybackState::Paused);
}

#[test]
fn ticks_advance_the_playhead_by_elapsed_time_times_rate() {
    let mut player = player();
    player.play(Duration::from_secs(100)).unwrap();
    next_frame(&mut player);

    player.tick(Duration::from_millis(100_200)).unwrap();
    assert!((player.position_secs() - 0.2).abs() < 1e-9);
    let (position, frame) = next_frame(&mut player);
    assert!((position - 0.2).abs() < 1e-9);
    assert!((frame.timestamp_secs() - 0.2).abs() < 1e-9);

    player.set_rate(2.0).unwrap();
    player.tick(Duration::from_millis(100_400)).unwrap();
    assert!((player.position_secs() - 0.6).abs() < 1e-9);
    let (position, _) = next_frame(&mut player);
    assert!((position - 0.6).abs() < 1e-9);
}

#[test]
fn zero_elapsed_tick_submits_nothing() {
    let mut player = player();
    player.play(Duration::from_secs(1)).unwrap();
    assert_eq!(player.tick(Duration::from_secs(1)).unwrap(), None);
}

#[test]
fn reaching_the_end_pauses_and_reports_ended_after_the_final_frame() {
    let mut player = player();
    player.play(Duration::ZERO).unwrap();
    next_frame(&mut player);

    player.tick(Duration::from_secs(30)).unwrap();

    assert!(player.has_ended());
    assert_eq!(player.state(), PlaybackState::Paused);
    assert_eq!(player.position_secs(), 0.9);
    let (position, _) = next_frame(&mut player);
    assert_eq!(position, 0.9);
    assert_eq!(
        player.wait(WAIT),
        Some(PlayerEvent::Ended { position_secs: 0.9 })
    );
}

#[test]
fn play_at_the_end_restarts_from_the_beginning() {
    let mut player = player();
    player.seek(0.9).unwrap();
    next_frame(&mut player);
    player.play(Duration::ZERO).unwrap();
    assert_eq!(player.position_secs(), 0.0);
    assert!(!player.has_ended());
}

#[test]
fn seek_clamps_and_renders_while_paused() {
    let mut player = player();
    player.seek(0.5).unwrap();
    let (position, frame) = next_frame(&mut player);
    assert_eq!(position, 0.5);
    assert_eq!(frame.timestamp_secs(), 0.5);

    player.seek(99.0).unwrap();
    assert_eq!(player.position_secs(), 0.9);
}

#[test]
fn live_scene_edit_rerenders_the_current_position() {
    let mut player = player();
    player.seek(0.4).unwrap();
    let (_, opaque) = next_frame(&mut player);

    player.set_scene(scene(0.5), Vec::new(), 0.9).unwrap();
    let (position, faded) = next_frame(&mut player);

    assert_eq!(position, 0.4);
    assert_ne!(opaque.pixels(), faded.pixels());
    assert_eq!(player.engine().stats().cache_hits, 1);
}

#[test]
fn burst_of_ticks_delivers_the_latest_position() {
    let mut player = player();
    player.play(Duration::ZERO).unwrap();
    for step in 1..=30 {
        player.tick(Duration::from_millis(step * 20)).unwrap();
    }
    let (position, _) = next_frame(&mut player);
    assert!((position - 0.6).abs() < 1e-6, "position {position}");
}

#[test]
fn rejects_non_positive_durations() {
    let info = DecodeSource::open(&fixture()).unwrap().info();
    let engine = Engine::new(EngineConfig {
        canvas_width: info.width,
        canvas_height: info.height,
        max_open_sources: 1,
        frame_cache_bytes: 1 << 20,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap();
    assert_eq!(
        Player::new(engine, scene(1.0), 0.0).unwrap_err(),
        PlayerError::InvalidDuration(0.0)
    );
}

#[test]
fn audio_clock_drives_the_playhead_and_never_rewinds_it() {
    let mut player = player();
    player.play(Duration::from_secs(1)).unwrap();
    next_frame(&mut player);

    // Wall clock says 5 s passed, but the audio device only played 0.2 s: follow the audio.
    player
        .tick_synced(Duration::from_secs(6), 0.2)
        .unwrap()
        .expect("audio advanced");
    assert!((player.position_secs() - 0.2).abs() < 1e-9);
    let (position, _) = next_frame(&mut player);
    assert!((position - 0.2).abs() < 1e-9);

    // A stalled audio clock holds the picture; a glitch backwards does not rewind it.
    assert_eq!(
        player.tick_synced(Duration::from_secs(7), 0.2).unwrap(),
        None
    );
    assert_eq!(
        player.tick_synced(Duration::from_secs(8), 0.1).unwrap(),
        None
    );
    assert!((player.position_secs() - 0.2).abs() < 1e-9);
    assert_eq!(
        player
            .tick_synced(Duration::from_secs(9), f64::NAN)
            .unwrap(),
        None
    );

    assert!((player.drift_against(0.15) - 0.05).abs() < 1e-9);
    assert!((player.drift_against(0.25) + 0.05).abs() < 1e-9);
}

#[test]
fn audio_clock_reaching_the_end_pauses_and_reports_ended() {
    let mut player = player();
    player.play(Duration::ZERO).unwrap();
    next_frame(&mut player);
    player.tick_synced(Duration::from_secs(1), 30.0).unwrap();
    assert!(player.has_ended());
    assert_eq!(player.state(), PlaybackState::Paused);
    assert_eq!(player.position_secs(), 0.9);
    next_frame(&mut player);
    assert_eq!(
        player.wait(WAIT),
        Some(PlayerEvent::Ended { position_secs: 0.9 })
    );
}

#[test]
fn synced_ticks_are_ignored_while_paused() {
    let mut player = player();
    assert_eq!(
        player.tick_synced(Duration::from_secs(1), 0.5).unwrap(),
        None
    );
    assert_eq!(player.position_secs(), 0.0);
}
