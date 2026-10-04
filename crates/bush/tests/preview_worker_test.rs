use std::path::PathBuf;

use bush::{
    Generation, PreviewCommand, PreviewWorker, WorkerFailure, WorkerOperation, WorkerSubmitError,
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4")
}

#[test]
fn worker_decodes_latest_open_request_off_the_caller_thread() {
    let worker = PreviewWorker::new();
    worker
        .submit(Generation(1), PreviewCommand::Open { path: fixture() })
        .unwrap();

    let frame = worker.wait_for_frame(Generation(1)).unwrap();
    assert_eq!((frame.width(), frame.height()), (320, 240));
    worker.shutdown();
}

#[test]
fn worker_seek_produces_a_new_generation_frame() {
    let worker = PreviewWorker::new();
    worker
        .submit(Generation(1), PreviewCommand::Open { path: fixture() })
        .unwrap();
    let first = worker.wait_for_frame(Generation(1)).unwrap();
    worker
        .submit(
            Generation(2),
            PreviewCommand::Seek {
                timestamp_secs: 0.5,
            },
        )
        .unwrap();

    let sought = worker.wait_for_frame(Generation(2)).unwrap();
    assert!(sought.timestamp_secs() >= first.timestamp_secs());
    worker.shutdown();
}

#[test]
fn shutdown_rejects_new_commands() {
    let worker = PreviewWorker::new();
    worker.shutdown();

    assert!(matches!(
        worker.submit(Generation(1), PreviewCommand::Open { path: fixture() }),
        Err(WorkerSubmitError::Stopped)
    ));
}

fn failure_for(worker: &PreviewWorker, generation: Generation) -> WorkerFailure {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if let Some(failure) = worker.take_failure(generation) {
            return failure;
        }
        assert!(std::time::Instant::now() < deadline, "no worker failure");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

#[test]
fn opening_a_missing_file_and_seeking_without_a_source_are_reported() {
    let worker = PreviewWorker::new();
    worker
        .submit(
            Generation(1),
            PreviewCommand::Open {
                path: PathBuf::from("does-not-exist.mp4"),
            },
        )
        .unwrap();
    let failure = failure_for(&worker, Generation(1));
    assert_eq!(failure.operation, WorkerOperation::Open);
    assert!(!failure.message.is_empty());

    worker
        .submit(
            Generation(2),
            PreviewCommand::Seek {
                timestamp_secs: 0.5,
            },
        )
        .unwrap();
    let failure = failure_for(&worker, Generation(2));
    assert_eq!(failure.operation, WorkerOperation::Seek);
    assert_eq!(failure.message, "no source is open");
    worker.shutdown();
}

#[test]
fn failures_are_matched_to_their_generation() {
    let settle = || std::thread::sleep(std::time::Duration::from_millis(400));
    let fail_seek = |worker: &PreviewWorker, generation: u64| {
        worker
            .submit(
                Generation(generation),
                PreviewCommand::Seek {
                    timestamp_secs: 0.0,
                },
            )
            .unwrap();
        settle();
    };
    let worker = PreviewWorker::new();

    // A caller already on a newer generation discards an older failure.
    fail_seek(&worker, 5);
    assert!(worker.take_failure(Generation(6)).is_none());
    assert!(
        worker.take_failure(Generation(5)).is_none(),
        "it was dropped"
    );

    // A failure newer than the caller is kept for its own caller.
    fail_seek(&worker, 8);
    assert!(worker.take_failure(Generation(7)).is_none());
    assert_eq!(
        worker.take_failure(Generation(8)).unwrap().generation,
        Generation(8)
    );
    worker.shutdown();
}

#[test]
fn a_blocked_wait_ends_when_the_worker_shuts_down() {
    let worker = std::sync::Arc::new(PreviewWorker::default());
    assert!(worker.take_frame(Generation(1)).is_none());
    let stopper = std::sync::Arc::clone(&worker);
    let handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        stopper.shutdown();
    });
    assert!(worker.wait_for_frame(Generation(9)).is_none());
    handle.join().unwrap();
}
