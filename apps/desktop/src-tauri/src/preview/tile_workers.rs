// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! The decoders behind timeline tiles and poster frames. The webview asks for a screenful of
//! tiles at once; one thread and one freshly opened decoder per request made 24 simultaneous
//! requests on a 2-hour file each wait 1.3 s (every decoder starting a thread per core), against
//! 0.19 s for the first tile when four workers drain the same burst. Workers also keep the
//! decoder of the file they just served, since opening one costs more than the frame (45 ms
//! against 30 ms on a 2.8 GB recording), and let it go as soon as the queue stays empty so an
//! idle app holds no handle on the user's media.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use avcore::FrameSampler;

use crate::frames::{encode_jpeg, THUMBNAIL_MAX_DIM};
use crate::state::lock_or_recover;

/// Decoders running at once. More than this only adds contention: each decoder already uses a
/// thread per core.
const WORKERS: usize = 4;

/// How long a worker keeps a decoder open with nothing to do.
const IDLE: Duration = Duration::from_millis(300);

/// A decode slower than this is given up on and reported as a miss.
const SAMPLE_DEADLINE: Duration = Duration::from_millis(800);

struct Job {
    source: PathBuf,
    at_secs: f64,
    reply: Sender<Option<Vec<u8>>>,
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}

pub struct TileWorkers {
    shared: Arc<Shared>,
}

impl Default for TileWorkers {
    fn default() -> Self {
        Self::new(WORKERS)
    }
}

impl TileWorkers {
    pub fn new(workers: usize) -> Self {
        let shared = Arc::new(Shared::default());
        for _ in 0..workers {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || work(&shared));
        }
        Self { shared }
    }

    /// The JPEG of `source` at `at_secs`, once a worker is free to decode it.
    pub fn extract(&self, source: &Path, at_secs: f64) -> Option<Vec<u8>> {
        let (reply, answer) = channel();
        lock_or_recover(&self.shared.queue).jobs.push_back(Job {
            source: source.to_path_buf(),
            at_secs,
            reply,
        });
        self.shared.wake.notify_one();
        answer.recv().ok().flatten()
    }
}

impl Drop for TileWorkers {
    fn drop(&mut self) {
        lock_or_recover(&self.shared.queue).closed = true;
        self.shared.wake.notify_all();
    }
}

fn work(shared: &Shared) {
    // The decoder of the file served last, kept for the next request on it.
    let mut open: Option<(PathBuf, FrameSampler)> = None;
    loop {
        let job = {
            let mut queue = lock_or_recover(&shared.queue);
            loop {
                if let Some(job) = queue.jobs.pop_front() {
                    break job;
                }
                if queue.closed {
                    return;
                }
                let (next, timeout) = shared
                    .wake
                    .wait_timeout(queue, IDLE)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                queue = next;
                if timeout.timed_out() {
                    open = None;
                }
            }
        };
        let jpeg = serve(&mut open, &job);
        let _ = job.reply.send(jpeg);
    }
}

fn serve(open: &mut Option<(PathBuf, FrameSampler)>, job: &Job) -> Option<Vec<u8>> {
    if !job.source.exists() {
        *open = None;
        return None;
    }
    if open.as_ref().is_none_or(|(path, _)| *path != job.source) {
        *open = FrameSampler::open(&job.source)
            .ok()
            .map(|sampler| (job.source.clone(), sampler));
    }
    let (_, sampler) = open.as_ref()?;
    let frame = sampler.sample(job.at_secs, SAMPLE_DEADLINE);
    if frame.is_none() {
        // A decoder that failed or ran late may be stuck mid-GOP; start over next time.
        *open = None;
    }
    encode_jpeg(&frame?, THUMBNAIL_MAX_DIM)
}

#[cfg(test)]
#[path = "tile_workers_test.rs"]
mod tests;
