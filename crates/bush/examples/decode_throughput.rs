//! Steady-state software decode throughput of one file, no composition: the time to open it, the
//! first frame, then the average over the rest. Run:
//! `cargo run --release -p bush --example decode_throughput -- clip.mp4 [frames]`.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use bush::{DecodePolicy, DecodeSource};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .expect("usage: decode_throughput clip.mp4 [frames]"),
    );
    let frames: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(90);
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let mut source = DecodeSource::open_with_policy(&path, DecodePolicy::Software).unwrap();
    let opened = started.elapsed();
    let first = Instant::now();
    let frame = source.next_frame(&cancel).unwrap().expect("a first frame");
    println!(
        "{}x{}: open {:.1} ms, first frame {:.1} ms",
        frame.width(),
        frame.height(),
        opened.as_secs_f64() * 1e3,
        first.elapsed().as_secs_f64() * 1e3
    );
    let steady = Instant::now();
    let mut count = 0;
    while count < frames {
        match source.next_frame(&cancel).unwrap() {
            Some(frame) => drop(frame),
            None => break,
        }
        count += 1;
    }
    let per_frame = steady.elapsed().as_secs_f64() * 1e3 / f64::from(count.max(1));
    println!(
        "{count} frames: {per_frame:.2} ms/frame ({:.0} fps)",
        1e3 / per_frame
    );
}
