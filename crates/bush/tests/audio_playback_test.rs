use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use bush::{
    AudioPlayer, AudioPlayerError, AudioRenderError, AudioRenderer, AudioSink, AudioSource,
    Contributor, ContributorContent, ContributorId, FakeSink, MediaSource, Placement,
    SceneSnapshot, SceneVersions, SinkError, SourceRange, Version,
};

const RATE: u32 = 44_100;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/audio.m4a")
}

fn clip(id: u64, audible: bool, placement: Placement) -> Contributor {
    Contributor {
        id: ContributorId(id),
        layer: id as i32,
        visible: false,
        audible,
        content: ContributorContent::Media(MediaSource {
            path: fixture(),
            range: SourceRange::new(0.0, 0.9).unwrap(),
            version: Version(1),
        }),
        placement,
    }
}

fn scene(contributors: Vec<Contributor>) -> SceneSnapshot {
    SceneSnapshot::new(SceneVersions::default(), contributors).unwrap()
}

fn renderer() -> AudioRenderer {
    AudioRenderer::new(RATE, 1, 8192, 2).unwrap()
}

fn source() -> AudioSource {
    AudioSource::open(&fixture(), RATE, 1, 8192).unwrap()
}

fn peak(samples: &[f32]) -> f32 {
    samples
        .iter()
        .fold(0.0, |peak, sample| f32::max(peak, sample.abs()))
}

#[test]
fn source_reads_are_sample_accurate_across_sequential_and_random_access() {
    let cancel = AtomicBool::new(false);
    let mut reference = source();
    let whole = reference.read(0, 4096, &cancel).unwrap().unwrap();

    // Sequential 1024-frame reads reproduce the single large read exactly.
    let mut sequential = source();
    let mut joined = Vec::new();
    for chunk in 0..4 {
        let block = sequential
            .read(chunk * 1024, 1024, &cancel)
            .unwrap()
            .unwrap();
        assert_eq!(
            block.timestamp_secs(),
            (chunk * 1024) as f64 / f64::from(RATE)
        );
        joined.extend_from_slice(block.samples());
    }
    let first_diff = joined.iter().zip(whole.samples()).position(|(a, b)| a != b);
    assert_eq!(first_diff, None, "first differing frame");
    assert_eq!(joined.len(), whole.samples().len());

    // A backwards jump then a re-read of the same range returns the same samples.
    let again = sequential.read(1024, 1024, &cancel).unwrap().unwrap();
    let max_error = again
        .samples()
        .iter()
        .zip(&whole.samples()[1024..2048])
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f32, f32::max);
    assert!(max_error < 1e-3, "re-read error {max_error}");
}

#[test]
fn source_pads_past_the_end_with_silence_and_honours_cancel() {
    let mut source = source();
    let cancel = AtomicBool::new(false);
    let block = source
        .read(u64::from(RATE) * 2, 512, &cancel)
        .unwrap()
        .unwrap();
    assert_eq!(block.frame_count(), 512);
    assert!(block.samples().iter().all(|sample| *sample == 0.0));

    assert!(source
        .read(0, 512, &AtomicBool::new(true))
        .unwrap()
        .is_none());
}

#[test]
fn source_rejects_reads_over_the_frame_budget() {
    let mut source = source();
    assert!(matches!(
        source.read(0, 8193, &AtomicBool::new(false)),
        Err(bush::AudioSourceError::ReadTooLarge { .. })
    ));
}

#[test]
fn renderer_places_a_clip_on_the_timeline_with_gain() {
    let cancel = AtomicBool::new(false);
    let mut direct = source();
    let expected = direct.read(1000, 2048, &cancel).unwrap().unwrap();

    // Clip starts at timeline frame 500; rendering timeline frames 1500.. reads source 1000...
    let start_secs = 500.0 / f64::from(RATE);
    let scene = scene(vec![clip(
        1,
        true,
        Placement {
            timeline_start_secs: start_secs,
            gain: 0.5,
            ..Placement::default()
        },
    )]);
    let mut renderer = renderer();
    let block = renderer
        .render(&scene, 1500, 2048, &cancel)
        .unwrap()
        .unwrap();

    assert_eq!(block.frame_count(), 2048);
    for (mixed, original) in block.samples().iter().zip(expected.samples()) {
        assert!((mixed - original * 0.5).abs() < 1e-3);
    }
}

#[test]
fn renderer_zero_pads_before_a_clip_starts_and_after_it_ends() {
    let cancel = AtomicBool::new(false);
    let start_secs = 1000.0 / f64::from(RATE);
    let scene = scene(vec![clip(
        1,
        true,
        Placement {
            timeline_start_secs: start_secs,
            ..Placement::default()
        },
    )]);
    let mut renderer = renderer();

    // Window straddles the clip start: first 1000 frames are silent, the rest is audio.
    let block = renderer.render(&scene, 0, 3000, &cancel).unwrap().unwrap();
    assert!(block.samples()[..1000].iter().all(|sample| *sample == 0.0));
    assert!(peak(&block.samples()[1000..]) > 0.01);

    // Window entirely after the clip (0.9s ends at timeline ~1000 + 39690 frames).
    let after = renderer
        .render(&scene, 60_000, 1024, &cancel)
        .unwrap()
        .unwrap();
    assert!(after.samples().iter().all(|sample| *sample == 0.0));
}

#[test]
fn renderer_mixes_overlapping_clips_with_a_hard_limiter() {
    let cancel = AtomicBool::new(false);
    let scene = scene(vec![
        clip(1, true, Placement::default()),
        clip(
            2,
            true,
            Placement {
                gain: 4.0,
                ..Placement::default()
            },
        ),
    ]);
    let mut renderer = renderer();
    let block = renderer
        .render(&scene, 2000, 2048, &cancel)
        .unwrap()
        .unwrap();
    assert!(block
        .samples()
        .iter()
        .all(|sample| (-1.0..=1.0).contains(sample)));

    let mut direct = source();
    let single = direct.read(2000, 2048, &cancel).unwrap().unwrap();
    // Five times the original (1 + 4), limited to unity.
    for (mixed, original) in block.samples().iter().zip(single.samples()) {
        assert!((mixed - (original * 5.0).clamp(-1.0, 1.0)).abs() < 5e-3);
    }
}

#[test]
fn inaudible_contributors_are_silent_and_unsupported_speed_is_explicit() {
    let cancel = AtomicBool::new(false);
    let mut renderer = renderer();

    let muted = scene(vec![clip(1, false, Placement::default())]);
    let block = renderer.render(&muted, 0, 1024, &cancel).unwrap().unwrap();
    assert!(block.samples().iter().all(|sample| *sample == 0.0));

    let fast = scene(vec![clip(
        7,
        true,
        Placement {
            speed: 8.0,
            ..Placement::default()
        },
    )]);
    assert!(matches!(
        renderer.render(&fast, 0, 1024, &cancel),
        Err(AudioRenderError::UnsupportedSpeed(ContributorId(7)))
    ));
}

#[test]
fn player_fills_the_sink_to_its_target_and_continues_without_gaps() {
    let cancel = AtomicBool::new(false);
    let scene = scene(vec![clip(1, true, Placement::default())]);
    let mut player = AudioPlayer::new(renderer(), scene, 1024, 4096);
    let mut sink = FakeSink::new(8192, 1);

    let written = player.pump(&mut sink, &cancel).unwrap();
    assert_eq!(written, 4096);
    assert_eq!(sink.buffered_frames(), 4096);
    assert_eq!(player.position_frame(), 4096);

    // The device drains 2048 frames; the next pump tops the buffer back up.
    assert_eq!(sink.consume(2048), 2048);
    assert_eq!(player.pump(&mut sink, &cancel).unwrap(), 2048);
    assert_eq!(sink.underflows(), 0);

    // Everything played so far equals one contiguous read of the source.
    let mut direct = source();
    let expected = direct.read(0, 2048, &cancel).unwrap().unwrap();
    assert_eq!(sink.played(), expected.samples());
}

#[test]
fn seek_and_scene_edit_discard_queued_audio() {
    let cancel = AtomicBool::new(false);
    let loud = scene(vec![clip(1, true, Placement::default())]);
    let mut player = AudioPlayer::new(renderer(), loud, 1024, 2048);
    let mut sink = FakeSink::new(8192, 1);
    player.pump(&mut sink, &cancel).unwrap();

    player.seek(10_000, &mut sink);
    assert_eq!(sink.buffered_frames(), 0);
    assert_eq!(player.position_frame(), 10_000);
    player.pump(&mut sink, &cancel).unwrap();
    assert_eq!(player.position_frame(), 12_048);

    // A live edit rewinds to the device position and re-renders the queued span.
    let quiet = scene(vec![clip(
        1,
        true,
        Placement {
            gain: 0.0,
            ..Placement::default()
        },
    )]);
    player.set_scene(quiet, &mut sink);
    assert_eq!(player.position_frame(), 10_000);
    player.pump(&mut sink, &cancel).unwrap();
    assert_eq!(sink.consume(2048), 2048);
    assert!(sink.played().iter().all(|sample| *sample == 0.0));
}

#[test]
fn fake_sink_reports_underflow_overrun_and_device_loss() {
    let cancel = AtomicBool::new(false);
    let scene = scene(vec![clip(1, true, Placement::default())]);
    let mut renderer = renderer();
    let block = renderer.render(&scene, 0, 1024, &cancel).unwrap().unwrap();

    let mut sink = FakeSink::new(1500, 1);
    sink.write(&block).unwrap();
    assert_eq!(
        sink.write(&block),
        Err(SinkError::Overrun {
            capacity_frames: 1500,
            requested_frames: 1024
        })
    );

    assert_eq!(sink.consume(2000), 1024);
    assert_eq!(sink.underflows(), 1);

    sink.lose_device();
    assert_eq!(sink.write(&block), Err(SinkError::Closed));

    let mut player = AudioPlayer::new(renderer, scene, 1024, 2048);
    assert!(matches!(
        player.pump(&mut sink, &cancel),
        Err(AudioPlayerError::Sink(SinkError::Closed))
    ));
}

#[test]
fn sped_up_clips_are_stretched_continuously_and_end_early() {
    let cancel = AtomicBool::new(false);
    // 0.9 s of source at 2x occupies 0.45 s of timeline.
    let scene = scene(vec![clip(
        1,
        true,
        Placement {
            speed: 2.0,
            ..Placement::default()
        },
    )]);

    let mut whole = renderer();
    let single = whole.render(&scene, 0, 8192, &cancel).unwrap().unwrap();
    assert!(peak(single.samples()) > 0.01, "stretched audio is silent");

    // Two consecutive blocks join exactly like one block of the same length.
    let mut chunked = renderer();
    let first = chunked.render(&scene, 0, 4096, &cancel).unwrap().unwrap();
    let second = chunked
        .render(&scene, 4096, 4096, &cancel)
        .unwrap()
        .unwrap();
    let mut joined = first.samples().to_vec();
    joined.extend_from_slice(second.samples());
    assert_eq!(joined, single.samples());

    // The clip ends at 0.45 s (19845 frames): afterwards the output is silent.
    let after = chunked
        .render(&scene, 30_000, 2048, &cancel)
        .unwrap()
        .unwrap();
    assert!(after.samples().iter().all(|sample| *sample == 0.0));

    // A jump (seek) restarts the stretch at the matching source position without error.
    let jumped = chunked
        .render(&scene, 10_000, 2048, &cancel)
        .unwrap()
        .unwrap();
    assert!(peak(jumped.samples()) > 0.001);
}

#[test]
fn slowed_clips_run_longer_than_their_source() {
    let cancel = AtomicBool::new(false);
    let scene = scene(vec![clip(
        1,
        true,
        Placement {
            speed: 0.5,
            ..Placement::default()
        },
    )]);
    let mut renderer = renderer();
    // 0.9 s of source at half speed lasts 1.8 s: still audible at 1.5 s, silent at 2 s.
    let at_one_and_a_half = renderer
        .render(&scene, 66_150, 2048, &cancel)
        .unwrap()
        .unwrap();
    assert!(peak(at_one_and_a_half.samples()) > 0.001);
    let past_end = renderer
        .render(&scene, 88_200 + 2048, 2048, &cancel)
        .unwrap()
        .unwrap();
    assert!(past_end.samples().iter().all(|sample| *sample == 0.0));
}

#[test]
fn speed_ramped_clips_follow_the_integrated_duration() {
    let cancel = AtomicBool::new(false);
    // 0.9 s of source ramping 1x -> 2x: average 1.5x, so 0.6 s on the timeline (26460 frames).
    let scene = scene(vec![clip(
        1,
        true,
        Placement {
            speed: 1.0,
            speed_ramp_end: Some(2.0),
            ..Placement::default()
        },
    )]);
    let mut renderer = renderer();
    let early = renderer.render(&scene, 0, 4096, &cancel).unwrap().unwrap();
    assert!(peak(early.samples()) > 0.01);
    let late = renderer
        .render(&scene, 24_000, 2048, &cancel)
        .unwrap()
        .unwrap();
    assert!(peak(late.samples()) > 0.001, "ramped clip ended too early");
    let after = renderer
        .render(&scene, 30_000, 2048, &cancel)
        .unwrap()
        .unwrap();
    assert!(after.samples().iter().all(|sample| *sample == 0.0));

    // Sequential blocks stay continuous through the changing speed.
    let mut chunked = self::renderer();
    let mut whole = self::renderer();
    let single = whole.render(&scene, 0, 4096, &cancel).unwrap().unwrap();
    let first = chunked.render(&scene, 0, 2048, &cancel).unwrap().unwrap();
    let second = chunked
        .render(&scene, 2048, 2048, &cancel)
        .unwrap()
        .unwrap();
    assert_eq!(
        first.samples().len() + second.samples().len(),
        single.samples().len()
    );
    assert_eq!(first.samples(), &single.samples()[..2048]);

    // A ramp outside the supported speed range is an explicit error.
    let too_fast = self::scene(vec![clip(
        2,
        true,
        Placement {
            speed: 1.0,
            speed_ramp_end: Some(9.0),
            ..Placement::default()
        },
    )]);
    assert!(matches!(
        renderer.render(&too_fast, 0, 1024, &cancel),
        Err(AudioRenderError::UnsupportedSpeed(_))
    ));
}
