//! How long importing a clip takes, step by step. Ignored by default: it needs a real clip
//! (`scripts/make-bench-video.sh`) and prints timings instead of asserting them.
//! `OCA_BENCH_VIDEO=clip.mp4 cargo test --release -p core --test it import_perf -- --ignored --nocapture`

use std::path::PathBuf;
use std::time::Instant;

use avcore::PreviewQuality;

fn timed<T>(label: &str, step: impl FnOnce() -> T) -> T {
    let started = Instant::now();
    let value = step();
    eprintln!("{label}: {:.2} s", started.elapsed().as_secs_f64());
    value
}

#[test]
#[ignore]
fn import_perf() {
    let Some(path) = std::env::var_os("OCA_BENCH_VIDEO").map(PathBuf::from) else {
        eprintln!("set OCA_BENCH_VIDEO to a clip to run this");
        return;
    };
    let dir = std::env::temp_dir().join("avcore_import_perf");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    timed("probe", || avcore::probe_media(&path).unwrap());
    timed("waveform", || avcore::generate_waveform(&path).unwrap());
    timed("loudness", || avcore::measure_loudness(&path).unwrap());
    timed("proxy", || {
        avcore::ensure_proxy(&path, &dir, PreviewQuality::default()).unwrap()
    });
    let _ = std::fs::remove_dir_all(&dir);
}
