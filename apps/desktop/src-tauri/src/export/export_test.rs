use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use avcore::{Canvas, ExportJob, ExportJobStatus, GpuEncoderPreference, RenderControl};
use tauri::Listener;

use super::*;
use crate::testing::{harness, path_in, Harness};

fn job(id: u64, status: ExportJobStatus) -> ExportJob {
    ExportJob {
        id,
        title: format!("job {id}"),
        segments: Vec::new(),
        text_segments: Vec::new(),
        shape_segments: Vec::new(),
        privacy_blur_segments: Vec::new(),
        track_segments: Vec::new(),
        audio_segments: Vec::new(),
        canvas: Canvas {
            width: 320,
            height: 180,
            fps_num: 30,
            fps_den: 1,
            bit_rate_bps: 1_000_000,
        },
        target_lufs: -14.0,
        allow_copy: true,
        output_path: format!("out{id}.mp4"),
        status,
    }
}

fn queue(h: &Harness) -> Arc<ExportQueue> {
    h.state().exports.clone()
}

fn put(queue: &ExportQueue, jobs: Vec<ExportJob>) {
    *queue.jobs.lock().unwrap() = jobs;
}

fn status_of(queue: &ExportQueue, id: u64) -> Option<ExportJobStatus> {
    queue
        .jobs
        .lock()
        .unwrap()
        .iter()
        .find(|j| j.id == id)
        .map(|j| j.status.clone())
}

/// Waits until no render worker is running.
fn wait_idle(queue: &ExportQueue) {
    let started = Instant::now();
    while !queue.controls.lock().unwrap().is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "render never finished"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn settings_survive_a_reload_and_the_worker_count_is_clamped() {
    let h = harness();
    let dir = h.dir.join("settings");
    let first = ExportQueue::load(dir.clone());
    first
        .set_gpu_encoder_preference(GpuEncoderPreference::Cpu)
        .unwrap();
    first.set_worker_count(99).unwrap();
    assert_eq!(first.settings.lock().unwrap().worker_count, 8);
    first.set_worker_count(0).unwrap();
    assert_eq!(first.settings.lock().unwrap().worker_count, 1);
    first.set_worker_count(3).unwrap();

    let second = ExportQueue::load(dir);
    assert_eq!(second.gpu_encoder_preference(), GpuEncoderPreference::Cpu);
    assert_eq!(second.settings.lock().unwrap().worker_count, 3);
}

#[test]
fn a_corrupt_settings_file_or_queue_falls_back_to_the_defaults() {
    let h = harness();
    let dir = h.dir.join("corrupt");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("export-settings.json"), b"{not json").unwrap();
    std::fs::write(avcore::export_queue_path(&dir), b"garbage").unwrap();
    let queue = ExportQueue::load(dir);
    assert_eq!(queue.gpu_encoder_preference(), GpuEncoderPreference::Auto);
    assert_eq!(queue.settings.lock().unwrap().worker_count, 1);
    assert!(queue.snapshot().is_empty());
}

#[test]
fn saving_settings_to_an_unwritable_place_reports_the_error() {
    let h = harness();
    let blocker = h.dir.join("a-file");
    std::fs::write(&blocker, b"x").unwrap();
    let queue = ExportQueue::load(blocker.join("inside"));
    assert!(queue.set_worker_count(2).is_err());
}

#[test]
fn the_snapshot_reports_progress_only_for_running_jobs_and_elapsed_time_once_started() {
    let h = harness();
    let queue = queue(&h);
    put(
        &queue,
        vec![
            job(1, ExportJobStatus::Rendering { percent: 40 }),
            job(2, ExportJobStatus::Paused { percent: 10 }),
            job(3, ExportJobStatus::Queued),
            job(4, ExportJobStatus::Done),
        ],
    );
    queue.started.lock().unwrap().insert(1, Instant::now());
    let views = queue.snapshot();
    let percents: Vec<u8> = views.iter().map(|v| v.percent).collect();
    assert_eq!(percents, vec![40, 10, 0, 0]);
    assert!(views[0].elapsed_secs.is_some());
    assert!(views[2].elapsed_secs.is_none());
    assert_eq!(views[0].title, "job 1");
    assert_eq!(views[0].output_path, "out1.mp4");
}

#[test]
fn pausing_moves_queued_and_rendering_jobs_to_paused_and_leaves_others() {
    let h = harness();
    let queue = queue(&h);
    put(
        &queue,
        vec![
            job(1, ExportJobStatus::Queued),
            job(2, ExportJobStatus::Rendering { percent: 55 }),
            job(3, ExportJobStatus::Done),
        ],
    );
    for id in [1, 2, 3, 99] {
        queue.pause(h.handle(), id);
    }
    assert_eq!(
        status_of(&queue, 1),
        Some(ExportJobStatus::Paused { percent: 0 })
    );
    assert_eq!(
        status_of(&queue, 2),
        Some(ExportJobStatus::Paused { percent: 55 })
    );
    assert_eq!(status_of(&queue, 3), Some(ExportJobStatus::Done));
}

#[test]
fn pausing_a_running_job_pauses_its_render_and_resuming_wakes_it() {
    let h = harness();
    let queue = queue(&h);
    put(
        &queue,
        vec![job(1, ExportJobStatus::Rendering { percent: 30 })],
    );
    let control = Arc::new(RenderControl::new());
    queue
        .controls
        .lock()
        .unwrap()
        .insert(1, Arc::clone(&control));

    queue.pause(h.handle(), 1);
    assert!(control.is_paused());
    assert_eq!(
        status_of(&queue, 1),
        Some(ExportJobStatus::Paused { percent: 30 })
    );

    queue.resume(h.handle(), 1);
    assert!(!control.is_paused());
    assert_eq!(
        status_of(&queue, 1),
        Some(ExportJobStatus::Rendering { percent: 30 })
    );
}

#[test]
fn resuming_a_paused_job_with_no_worker_requeues_it_and_a_job_with_nothing_to_render_fails() {
    let h = harness();
    let queue = queue(&h);
    put(&queue, vec![job(1, ExportJobStatus::Paused { percent: 0 })]);
    queue.resume(h.handle(), 1);
    wait_idle(&queue);
    assert!(matches!(
        status_of(&queue, 1),
        Some(ExportJobStatus::Failed { .. })
    ));
}

#[test]
fn resuming_a_job_that_is_not_paused_changes_nothing() {
    let h = harness();
    let queue = queue(&h);
    put(&queue, vec![job(1, ExportJobStatus::Done)]);
    queue.resume(h.handle(), 1);
    queue.resume(h.handle(), 99);
    assert_eq!(status_of(&queue, 1), Some(ExportJobStatus::Done));
}

#[test]
fn cancelling_a_waiting_job_removes_it_and_a_running_one_is_told_to_stop() {
    let h = harness();
    let queue = queue(&h);
    put(
        &queue,
        vec![
            job(1, ExportJobStatus::Queued),
            job(2, ExportJobStatus::Rendering { percent: 5 }),
        ],
    );
    let control = Arc::new(RenderControl::new());
    queue
        .controls
        .lock()
        .unwrap()
        .insert(2, Arc::clone(&control));
    queue.cancel(h.handle(), 1);
    assert_eq!(status_of(&queue, 1), None);
    queue.cancel(h.handle(), 2);
    assert!(control.is_cancelled());
    assert!(
        status_of(&queue, 2).is_some(),
        "the worker removes it when it stops"
    );
}

#[test]
fn removing_drops_idle_jobs_but_never_a_running_one() {
    let h = harness();
    let queue = queue(&h);
    put(
        &queue,
        vec![
            job(1, ExportJobStatus::Done),
            job(2, ExportJobStatus::Rendering { percent: 5 }),
        ],
    );
    queue
        .controls
        .lock()
        .unwrap()
        .insert(2, Arc::new(RenderControl::new()));
    queue.remove(h.handle(), 1);
    queue.remove(h.handle(), 2);
    assert_eq!(status_of(&queue, 1), None);
    assert!(status_of(&queue, 2).is_some());
}

#[test]
fn matching_loudness_retargets_only_queued_jobs() {
    let h = harness();
    let queue = queue(&h);
    put(
        &queue,
        vec![
            job(1, ExportJobStatus::Queued),
            job(2, ExportJobStatus::Done),
            job(3, ExportJobStatus::Paused { percent: 1 }),
        ],
    );
    queue.match_loudness(&h.handle(), -23.0);
    let targets: Vec<f32> = queue
        .jobs
        .lock()
        .unwrap()
        .iter()
        .map(|j| j.target_lufs)
        .collect();
    assert_eq!(targets, vec![-23.0, -14.0, -14.0]);
}

#[test]
fn an_empty_timeline_cannot_be_prepared_for_export() {
    let h = harness();
    let session = h.session();
    assert!(prepare_export(&session.project).is_err());
}

#[test]
fn a_real_clip_renders_to_a_file_and_announces_every_change() {
    let h = harness();
    h.add_fixture_clip();
    let prepared = prepare_export(&h.session().project).unwrap();
    assert_eq!(prepared.target_lufs, -14.0);
    let queue = queue(&h);
    let events = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = Arc::clone(&events);
    h.app.listen(EXPORT_JOBS, move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });

    let output = path_in(&h.dir, "out.mp4");
    let id = queue.enqueue(h.handle(), "Show".into(), output.clone(), prepared);
    wait_idle(&queue);

    assert_eq!(status_of(&queue, id), Some(ExportJobStatus::Done));
    assert!(std::fs::metadata(&output).unwrap().len() > 0);
    assert!(events.load(Ordering::SeqCst) >= 2);
    assert!(queue.snapshot()[0].elapsed_secs.is_none());
    assert!(avcore::export_queue_path(&h.dir.join("config")).exists());
}

#[test]
fn a_render_that_cannot_write_its_output_fails_the_job() {
    let h = harness();
    h.add_fixture_clip();
    let prepared = prepare_export(&h.session().project).unwrap();
    let queue = queue(&h);
    let output = h.dir.join("missing-folder").join("out.mp4");
    let id = queue.enqueue(h.handle(), "Show".into(), output, prepared);
    wait_idle(&queue);
    assert!(matches!(
        status_of(&queue, id),
        Some(ExportJobStatus::Failed { .. })
    ));
}

#[test]
fn a_second_job_waits_for_a_free_worker_and_cancelling_the_first_stops_its_render() {
    let h = harness();
    h.add_fixture_clip();
    let queue = queue(&h);
    let first = queue.enqueue(
        h.handle(),
        "One".into(),
        path_in(&h.dir, "one.mp4"),
        prepare_export(&h.session().project).unwrap(),
    );
    let second = queue.enqueue(
        h.handle(),
        "Two".into(),
        path_in(&h.dir, "two.mp4"),
        prepare_export(&h.session().project).unwrap(),
    );
    queue.pause(h.handle(), first);
    queue.cancel(h.handle(), first);
    // `wait_idle` alone could return in the instant between one worker finishing and the next
    // job being picked up, so wait for the second job itself.
    let started = Instant::now();
    while status_of(&queue, second) != Some(ExportJobStatus::Done) {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "the second job never finished"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    wait_idle(&queue);
    assert!(matches!(
        status_of(&queue, first),
        None | Some(ExportJobStatus::Done)
    ));
    assert_eq!(status_of(&queue, second), Some(ExportJobStatus::Done));
}

#[test]
fn a_settings_file_missing_fields_falls_back_to_the_defaults() {
    let h = harness();
    let dir = h.dir.join("partial");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("export-settings.json"), b"{}").unwrap();
    let queue = ExportQueue::load(dir);
    assert_eq!(queue.settings.lock().unwrap().worker_count, 1);
    assert_eq!(queue.gpu_encoder_preference(), GpuEncoderPreference::Auto);
}

#[test]
fn a_queue_that_cannot_be_saved_still_works_in_memory() {
    let h = harness();
    let blocker = h.dir.join("a-file");
    std::fs::write(&blocker, b"x").unwrap();
    let queue = ExportQueue::load(blocker.join("inside"));
    put(&queue, vec![job(1, ExportJobStatus::Queued)]);
    queue.match_loudness(&h.handle(), -23.0);
    assert_eq!(queue.jobs.lock().unwrap()[0].target_lufs, -23.0);
}

#[test]
fn progress_only_moves_running_or_paused_jobs() {
    let h = harness();
    let queue = queue(&h);
    put(
        &queue,
        vec![
            job(1, ExportJobStatus::Rendering { percent: 1 }),
            job(2, ExportJobStatus::Paused { percent: 1 }),
            job(3, ExportJobStatus::Done),
        ],
    );
    for id in [1, 2, 3, 99] {
        queue.progress(&h.handle(), id, 50);
    }
    assert_eq!(
        status_of(&queue, 1),
        Some(ExportJobStatus::Rendering { percent: 50 })
    );
    assert_eq!(
        status_of(&queue, 2),
        Some(ExportJobStatus::Paused { percent: 50 })
    );
    assert_eq!(status_of(&queue, 3), Some(ExportJobStatus::Done));
}

#[test]
fn finishing_updates_the_job_it_belongs_to_and_ignores_one_that_is_gone() {
    let report = || ExportReport {
        encoder: Some("libopenh264".to_string()),
        took_secs: 2.0,
        video_secs: 4.0,
    };
    let h = harness();
    let queue = queue(&h);
    put(
        &queue,
        vec![
            job(1, ExportJobStatus::Rendering { percent: 99 }),
            job(2, ExportJobStatus::Rendering { percent: 10 }),
            job(3, ExportJobStatus::Rendering { percent: 10 }),
        ],
    );
    queue.finish(
        h.handle(),
        1,
        Ok(avcore::RenderOutcome::Completed),
        report(),
    );
    queue.finish(
        h.handle(),
        2,
        Err(avcore::RenderError::EmptyTimeline),
        report(),
    );
    queue.finish(
        h.handle(),
        3,
        Ok(avcore::RenderOutcome::Cancelled),
        report(),
    );
    assert_eq!(status_of(&queue, 1), Some(ExportJobStatus::Done));
    assert!(matches!(
        status_of(&queue, 2),
        Some(ExportJobStatus::Failed { .. })
    ));
    assert_eq!(status_of(&queue, 3), None);
    // Only a finished export has a report to show; a failed or cancelled one has none.
    let reported = |id: u64| {
        queue
            .snapshot()
            .into_iter()
            .find(|job| job.id == id)
            .and_then(|job| job.report)
    };
    assert_eq!(
        reported(1).and_then(|r| r.encoder).as_deref(),
        Some("libopenh264")
    );
    assert!(reported(2).is_none());

    queue.finish(
        h.handle(),
        99,
        Ok(avcore::RenderOutcome::Completed),
        report(),
    );
    queue.finish(
        h.handle(),
        99,
        Err(avcore::RenderError::EmptyTimeline),
        report(),
    );
}

fn canvas_1080p() -> Canvas {
    Canvas {
        width: 1920,
        height: 1080,
        fps_num: 30,
        fps_den: 1,
        bit_rate_bps: 16_000_000,
    }
}

#[test]
fn no_options_leave_the_canvas_alone() {
    let canvas = canvas_1080p();
    let applied = ExportOptions::default().apply(canvas);
    assert_eq!(
        (applied.width, applied.height, applied.bit_rate_bps),
        (1920, 1080, 16_000_000)
    );
}

#[test]
fn a_lower_height_scales_width_and_bitrate_with_the_pixels() {
    let applied = ExportOptions {
        bitrate_mbps: None,
        max_height: Some(720),
    }
    .apply(canvas_1080p());
    assert_eq!((applied.width, applied.height), (1280, 720));
    assert_eq!(applied.bit_rate_bps, 7_111_111);
}

#[test]
fn a_height_above_the_canvas_never_upscales() {
    let applied = ExportOptions {
        bitrate_mbps: None,
        max_height: Some(2160),
    }
    .apply(canvas_1080p());
    assert_eq!(
        (applied.width, applied.height, applied.bit_rate_bps),
        (1920, 1080, 16_000_000)
    );
}

#[test]
fn an_explicit_bitrate_wins_over_the_scaled_one() {
    let applied = ExportOptions {
        bitrate_mbps: Some(5.5),
        max_height: Some(720),
    }
    .apply(canvas_1080p());
    assert_eq!(applied.bit_rate_bps, 5_500_000);
    assert_eq!(applied.height, 720);
}

#[test]
fn bitrates_and_heights_outside_what_the_encoder_takes_are_refused() {
    let bad = |bitrate_mbps, max_height| {
        ExportOptions {
            bitrate_mbps,
            max_height,
        }
        .validate()
    };
    assert!(bad(Some(0.1), None).is_err());
    assert!(bad(Some(f32::INFINITY), None).is_err());
    assert!(bad(None, Some(100)).is_err());
    assert!(bad(Some(8.0), Some(720)).is_ok());
}
