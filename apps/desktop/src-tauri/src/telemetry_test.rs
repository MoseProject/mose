use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use super::*;
use crate::testing::harness;

fn sample() -> avcore::TelemetryEvent {
    avcore::ResourceSampler::new().sample()
}

#[test]
fn telemetry_lives_in_the_log_folder() {
    assert_eq!(telemetry_path().file_name().unwrap(), "telemetry.jsonl");
}

#[test]
fn an_enabled_sampler_forwards_samples_until_the_receiver_goes_away() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    spawn_sampler(
        tx,
        Arc::new(AtomicBool::new(true)),
        Duration::from_millis(5),
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
            Some(sample())
        },
    );
    assert!(rx.blocking_recv().is_some());
    drop(rx);
    std::thread::sleep(Duration::from_millis(60));
    let settled = calls.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(
        calls.load(Ordering::SeqCst),
        settled,
        "the sampler stops once nobody listens"
    );
}

#[test]
fn a_disabled_sampler_stays_quiet_and_a_sampler_without_a_reading_skips_the_tick() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let enabled = Arc::new(AtomicBool::new(false));
    let reads = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&reads);
    spawn_sampler(
        tx,
        Arc::clone(&enabled),
        Duration::from_millis(5),
        move || {
            // Every other reading is missing, like a GPU that did not answer.
            (counter.fetch_add(1, Ordering::SeqCst) % 2 == 1).then(sample)
        },
    );
    std::thread::sleep(Duration::from_millis(60));
    assert!(rx.try_recv().is_err());
    assert_eq!(reads.load(Ordering::SeqCst), 0);

    enabled.store(true, Ordering::Relaxed);
    assert!(rx.blocking_recv().is_some());
    assert!(reads.load(Ordering::SeqCst) >= 2);
}

#[test]
fn enabled_telemetry_appends_records_to_the_file_and_disabled_writes_none() {
    let h = harness();
    let on = h.dir.join("on.jsonl");
    spawn_to(
        on.clone(),
        Arc::new(AtomicBool::new(true)),
        Duration::from_millis(10),
        Some(|| Some(sample())),
    );
    let off = h.dir.join("off.jsonl");
    spawn_to(
        off.clone(),
        Arc::new(AtomicBool::new(false)),
        Duration::from_millis(10),
        None::<fn() -> Option<avcore::TelemetryEvent>>,
    );
    let started = std::time::Instant::now();
    while !on.exists() {
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "no record was written"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!std::fs::read_to_string(&on).unwrap().trim().is_empty());
    assert!(!off.exists());
}

#[test]
fn the_default_launch_does_not_write_when_telemetry_is_off() {
    spawn(Arc::new(AtomicBool::new(false)));
}

#[test]
fn a_record_that_cannot_be_written_is_dropped_without_stopping_the_sampler() {
    let h = harness();
    let blocker = h.dir.join("a-file");
    std::fs::write(&blocker, b"x").unwrap();
    spawn_to(
        blocker.join("inside").join("t.jsonl"),
        Arc::new(AtomicBool::new(true)),
        Duration::from_millis(5),
        None::<fn() -> Option<avcore::TelemetryEvent>>,
    );
    std::thread::sleep(Duration::from_millis(120));
    assert!(!blocker.join("inside").exists());
}
