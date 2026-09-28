use std::path::PathBuf;

use bush::{Generation, PreviewCommand, PreviewWorker, WorkerSubmitError};

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
