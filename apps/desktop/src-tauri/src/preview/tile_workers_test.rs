use super::*;
use crate::frames::THUMBNAIL_MAX_DIM;
use std::time::Instant;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../crates/core/tests/fixtures/video.mp4")
}

#[test]
fn a_frame_comes_back_as_a_jpeg() {
    let workers = TileWorkers::default();
    let jpeg = workers
        .extract(&fixture(), 0.5, THUMBNAIL_MAX_DIM)
        .expect("a frame");
    assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
}

#[test]
fn a_missing_file_is_a_miss_and_the_worker_keeps_serving() {
    let workers = TileWorkers::new(1);
    assert!(workers
        .extract(Path::new("definitely-missing.mp4"), 0.0, THUMBNAIL_MAX_DIM)
        .is_none());
    assert!(workers
        .extract(&fixture(), 0.5, THUMBNAIL_MAX_DIM)
        .is_some());
}

#[test]
fn a_burst_larger_than_the_pool_is_answered_in_full() {
    let workers = Arc::new(TileWorkers::new(2));
    let handles: Vec<_> = (0..12)
        .map(|index| {
            let workers = Arc::clone(&workers);
            std::thread::spawn(move || {
                workers.extract(&fixture(), f64::from(index) * 0.1, THUMBNAIL_MAX_DIM)
            })
        })
        .collect();
    for handle in handles {
        assert!(handle.join().unwrap().is_some());
    }
}

#[test]
fn repeated_requests_on_one_file_reuse_its_decoder() {
    let workers = TileWorkers::new(1);
    let first = workers.extract(&fixture(), 0.2, THUMBNAIL_MAX_DIM).unwrap();
    // Backwards in time too: the kept decoder seeks, it is not stuck past the target.
    let second = workers.extract(&fixture(), 0.1, THUMBNAIL_MAX_DIM).unwrap();
    assert_eq!(&first[..2], &second[..2]);
}

#[test]
fn dropping_the_pool_stops_its_threads() {
    let shared = {
        let workers = TileWorkers::new(3);
        Arc::clone(&workers.shared)
    };
    // Each worker holds one reference until it sees `closed`.
    let deadline = Instant::now() + Duration::from_secs(5);
    while Arc::strong_count(&shared) > 1 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(Arc::strong_count(&shared), 1);
}

#[test]
fn a_strip_tile_is_a_small_jpeg_and_the_exact_extract_still_works_after_it() {
    let workers = TileWorkers::new(1);
    let tile = workers
        .extract_tile(&fixture(), 0.8, THUMBNAIL_MAX_DIM)
        .expect("a tile");
    assert_eq!(&tile[..2], &[0xFF, 0xD8]);
    let image = image::load_from_memory(&tile).unwrap();
    assert!(image.width().max(image.height()) <= THUMBNAIL_MAX_DIM);
    assert!(workers
        .extract(&fixture(), 0.8, THUMBNAIL_MAX_DIM)
        .is_some());
}
