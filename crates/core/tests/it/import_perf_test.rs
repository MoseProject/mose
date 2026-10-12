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

/// Several files imported at once, the way the editor does it today (a thread per file), against
/// one after another: time until each proxy is ready, and the total.
/// `OCA_BENCH_VIDEO=clip.mp4 OCA_BENCH_FILES=4 cargo test --release -p core --test it import_concurrency -- --ignored --nocapture`
#[test]
#[ignore]
fn import_concurrency() {
    let Some(path) = std::env::var_os("OCA_BENCH_VIDEO").map(PathBuf::from) else {
        eprintln!("set OCA_BENCH_VIDEO to a clip to run this");
        return;
    };
    let files: usize = std::env::var("OCA_BENCH_FILES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4);
    let dir = std::env::temp_dir().join("avcore_import_concurrency");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // Distinct files (so each gets its own proxy), made by hard link where the file system allows.
    let copies: Vec<PathBuf> = (0..files)
        .map(|index| {
            let copy = dir.join(format!("clip{index}.mp4"));
            if std::fs::hard_link(&path, &copy).is_err() {
                std::fs::copy(&path, &copy).unwrap();
            }
            copy
        })
        .collect();
    let proxies = dir.join("proxies");
    std::fs::create_dir_all(&proxies).unwrap();
    let report = |label: &str, mut done: Vec<f64>| {
        done.sort_by(f64::total_cmp);
        eprintln!(
            "{label}: first proxy {:.1} s, median {:.1} s, last {:.1} s",
            done[0],
            done[done.len() / 2],
            done[done.len() - 1]
        );
    };

    let started = Instant::now();
    let done: Vec<f64> = std::thread::scope(|scope| {
        let handles: Vec<_> = copies
            .iter()
            .map(|copy| {
                let proxies = proxies.join("together");
                std::fs::create_dir_all(&proxies).unwrap();
                scope.spawn(move || {
                    avcore::ensure_proxy(copy, &proxies, PreviewQuality::default()).unwrap();
                    started.elapsed().as_secs_f64()
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect()
    });
    report(&format!("{files} at once"), done);

    let started = Instant::now();
    let proxies = proxies.join("one by one");
    std::fs::create_dir_all(&proxies).unwrap();
    let done: Vec<f64> = copies
        .iter()
        .map(|copy| {
            avcore::ensure_proxy(copy, &proxies, PreviewQuality::default()).unwrap();
            started.elapsed().as_secs_f64()
        })
        .collect();
    report(&format!("{files} one by one"), done);
    // A few at a time: what the editor's import gate does.
    let started = Instant::now();
    let proxies = proxies.parent().unwrap().join("two at a time");
    std::fs::create_dir_all(&proxies).unwrap();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut done: Vec<f64> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    let mut finished = Vec::new();
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        let Some(copy) = copies.get(index) else {
                            return finished;
                        };
                        avcore::ensure_proxy(copy, &proxies, PreviewQuality::default()).unwrap();
                        finished.push(started.elapsed().as_secs_f64());
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect()
    });
    done.sort_by(f64::total_cmp);
    report(&format!("{files} two at a time"), done);
    let _ = std::fs::remove_dir_all(&dir);
}
