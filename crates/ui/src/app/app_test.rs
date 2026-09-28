// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use super::*;
use avcore::motion_template::{
    ColorBinding, GraphicTemplate, ParameterValue, TemplateElement, TemplateParameter,
    TemplateParameterKind, TemplateShapeElement, TemplateTextElement, TextBinding,
};
use avcore::timeline::{AudioRole, ClipInstance, ShapeKind, Track, TrackKind};
use avcore::{LoudnessMetrics, MediaAsset, MediaKind, Recency, Sequence, Timeline};
use eframe::egui;
use std::collections::HashMap;

mod application_state;
mod clip_basics;
mod clip_clipboard;
mod clip_visual_effects;
mod export_queue;
mod gameplay_analysis;
mod graphics;
mod interoperability;
mod media_import;
mod media_preview;
mod motion_background;
mod open_projects;
mod project_timeline;
mod shorts_pack;
mod support;
mod timeline_edit;
mod timeline_review;
mod track_clip_operations;
mod youtube_download;

use support::*;

#[test]
fn start_watching_folder_is_a_no_op_with_no_path_set() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());

    app.start_watching_folder();

    assert!(!app.watch_folder_state.running);
}

#[test]
fn start_watching_folder_is_a_no_op_while_already_running() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    app.watch_folder_state.watch_path = Some(std::path::PathBuf::from("E:/records"));
    app.watch_folder_state.running = true;
    app.watch_folder_state
        .files
        .push(crate::app::WatchedFileRow {
            path: std::path::PathBuf::from("E:/records/a.mp4"),
            status: crate::app::WatchFolderFileStatus::Processing,
            percent: 40,
            error: None,
            before: None,
            after: None,
            added_to_project: false,
        });

    app.start_watching_folder();

    // The already-running session's file list isn't cleared by a second, ignored call.
    assert_eq!(app.watch_folder_state.files.len(), 1);
}

#[test]
fn stop_watching_folder_is_a_no_op_when_nothing_is_running() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());

    // Just needs to not panic without a live watch session.
    app.stop_watching_folder();

    assert!(!app.watch_folder_state.running);
}

#[test]
fn stop_watching_folder_clears_the_running_flag_and_signals_the_stop_flag() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    app.watch_folder_state.running = true;
    app.watch_folder_state.stop = Some(std::sync::Arc::clone(&stop));

    app.stop_watching_folder();

    assert!(!app.watch_folder_state.running);
    assert!(stop.load(std::sync::atomic::Ordering::Relaxed));
}

#[test]
fn pump_watch_folder_inserts_a_newly_detected_file_at_the_front() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let path = std::path::PathBuf::from("E:/records/newest.mp4");
    app.watch_folder_state
        .files
        .push(crate::app::WatchedFileRow {
            path: std::path::PathBuf::from("E:/records/older.mp4"),
            status: crate::app::WatchFolderFileStatus::Done,
            percent: 100,
            error: None,
            before: None,
            after: None,
            added_to_project: false,
        });
    let _ = app
        .watch_folder_state
        .tx
        .send(crate::app::WatchFolderEvent::Detected(path.clone()));

    app.pump_watch_folder();

    assert_eq!(app.watch_folder_state.files.len(), 2);
    assert_eq!(app.watch_folder_state.files[0].path, path);
    assert_eq!(
        app.watch_folder_state.files[0].status,
        crate::app::WatchFolderFileStatus::Stabilizing
    );
}

#[test]
fn pump_watch_folder_applies_progress_and_completion_to_the_matching_row() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let path = std::path::PathBuf::from("E:/records/a.mp4");
    app.watch_folder_state
        .files
        .push(crate::app::WatchedFileRow {
            path: path.clone(),
            status: crate::app::WatchFolderFileStatus::Processing,
            percent: 10,
            error: None,
            before: None,
            after: None,
            added_to_project: false,
        });
    let before = avcore::LoudnessMetrics {
        integrated_lufs: -22.0,
        true_peak_dbtp: -3.0,
        loudness_range_lu: 8.0,
    };
    let after = avcore::LoudnessMetrics {
        integrated_lufs: -16.0,
        true_peak_dbtp: -1.0,
        loudness_range_lu: 6.0,
    };
    let _ = app
        .watch_folder_state
        .tx
        .send(crate::app::WatchFolderEvent::Progress(path.clone(), 55));
    let _ = app
        .watch_folder_state
        .tx
        .send(crate::app::WatchFolderEvent::Done {
            path: path.clone(),
            before,
            after,
        });

    app.pump_watch_folder();

    let row = &app.watch_folder_state.files[0];
    assert_eq!(row.status, crate::app::WatchFolderFileStatus::Done);
    assert_eq!(row.percent, 100);
    assert_eq!(row.before.unwrap().integrated_lufs, -22.0);
    assert_eq!(row.after.unwrap().integrated_lufs, -16.0);
}

#[test]
fn pump_watch_folder_applies_a_failure_to_the_matching_row() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let path = std::path::PathBuf::from("E:/records/a.mp4");
    app.watch_folder_state
        .files
        .push(crate::app::WatchedFileRow {
            path: path.clone(),
            status: crate::app::WatchFolderFileStatus::Processing,
            percent: 10,
            error: None,
            before: None,
            after: None,
            added_to_project: false,
        });
    let _ = app
        .watch_folder_state
        .tx
        .send(crate::app::WatchFolderEvent::Failed {
            path: path.clone(),
            message: "ffmpeg exited with code 1".to_string(),
        });

    app.pump_watch_folder();

    let row = &app.watch_folder_state.files[0];
    assert_eq!(row.status, crate::app::WatchFolderFileStatus::Error);
    assert_eq!(row.error.as_deref(), Some("ffmpeg exited with code 1"));
}

#[test]
fn add_watched_file_to_project_queues_the_cleaned_up_output_for_import() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let watch_path = std::path::PathBuf::from("E:/records");
    app.watch_folder_state.watch_path = Some(watch_path.clone());
    let source = watch_path.join("a.mp4");
    app.watch_folder_state
        .files
        .push(crate::app::WatchedFileRow {
            path: source.clone(),
            status: crate::app::WatchFolderFileStatus::Done,
            percent: 100,
            error: None,
            before: None,
            after: None,
            added_to_project: false,
        });

    app.add_watched_file_to_project(source.clone());

    assert!(app.watch_folder_state.files[0].added_to_project);
    assert_eq!(app.import_state.pending_imports, 1);
}

#[test]
fn add_watched_file_to_project_toasts_without_an_open_project() {
    let mut app = test_app(Vec::new(), Vec::new());
    let watch_path = std::path::PathBuf::from("E:/records");
    app.watch_folder_state.watch_path = Some(watch_path.clone());
    let source = watch_path.join("a.mp4");
    app.watch_folder_state
        .files
        .push(crate::app::WatchedFileRow {
            path: source.clone(),
            status: crate::app::WatchFolderFileStatus::Done,
            percent: 100,
            error: None,
            before: None,
            after: None,
            added_to_project: false,
        });

    app.add_watched_file_to_project(source);

    assert!(!app.watch_folder_state.files[0].added_to_project);
    assert_eq!(app.import_state.pending_imports, 0);
    assert_eq!(app.toasts.len(), 1);
}

#[test]
fn add_watched_file_to_project_is_a_no_op_once_already_queued() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let watch_path = std::path::PathBuf::from("E:/records");
    app.watch_folder_state.watch_path = Some(watch_path.clone());
    let source = watch_path.join("a.mp4");
    app.watch_folder_state
        .files
        .push(crate::app::WatchedFileRow {
            path: source.clone(),
            status: crate::app::WatchFolderFileStatus::Done,
            percent: 100,
            error: None,
            before: None,
            after: None,
            added_to_project: true,
        });

    app.add_watched_file_to_project(source);

    assert_eq!(
        app.import_state.pending_imports, 0,
        "a row already marked added_to_project must not be re-queued"
    );
}

/// Captures everything [`App::report_error`] hands to its reporter, for asserting on the
/// delivered reports in tests.
#[derive(Default)]
struct CapturingReporter {
    reports: std::sync::Mutex<Vec<avcore::ErrorReport>>,
}

impl avcore::ErrorReporter for CapturingReporter {
    fn report(&self, report: avcore::ErrorReport) {
        self.reports.lock().unwrap().push(report);
    }
}

#[test]
fn report_error_is_a_noop_without_a_reporter() {
    let app = test_app(Vec::new(), Vec::new());
    assert!(!app.report_error(
        avcore::ErrorCode::Import,
        avcore::ErrorSeverity::Error,
        avcore::Operation::Import,
        avcore::RecoveryOutcome::RequiresUserAction,
        false,
    ));
}

#[test]
fn report_error_delivers_a_valid_sanitized_report() {
    let reporter = std::sync::Arc::new(CapturingReporter::default());
    let mut app = test_app(Vec::new(), Vec::new());
    app.error_reporter =
        Some(std::sync::Arc::clone(&reporter) as std::sync::Arc<dyn avcore::ErrorReporter>);

    assert!(app.report_error(
        avcore::ErrorCode::ExportEncode,
        avcore::ErrorSeverity::Error,
        avcore::Operation::Export,
        avcore::RecoveryOutcome::Aborted,
        false,
    ));
    assert!(app.report_error(
        avcore::ErrorCode::Import,
        avcore::ErrorSeverity::Error,
        avcore::Operation::Import,
        avcore::RecoveryOutcome::RequiresUserAction,
        true,
    ));

    let reports = reporter.reports.lock().unwrap();
    assert_eq!(reports.len(), 2);
    let encoded = &reports[0];
    let imported = &reports[1];
    assert_eq!(encoded.error_code, avcore::ErrorCode::ExportEncode);
    assert_eq!(encoded.severity, avcore::ErrorSeverity::Error);
    assert_eq!(encoded.operation, avcore::Operation::Export);
    assert_eq!(encoded.recovery_outcome, avcore::RecoveryOutcome::Aborted);
    assert!(!encoded.retried);
    assert_eq!(imported.error_code, avcore::ErrorCode::Import);
    assert!(imported.retried);
    assert_eq!(
        encoded.session_id, imported.session_id,
        "session id is stable across the launch"
    );
    assert_ne!(
        encoded.event_id, imported.event_id,
        "event id is unique per report"
    );
    assert!(avcore::validate_report(encoded).is_ok());
    for report in reports.iter() {
        let stack = report
            .sanitized_stack_trace
            .as_deref()
            .expect("a stack trace is captured for every report");
        assert!(!stack.is_empty());
        assert!(
            !avcore::contains_forbidden_content(stack),
            "the captured backtrace must not survive sanitization with forbidden content"
        );
    }
}

#[test]
fn seed_error_reporting_seeds_launch_identity_and_breadcrumb() {
    let reporter = crate::app::error_reporting::seed_error_reporting(
        crate::i18n::Locale::En,
        crate::app::error_reporting::ErrorReportingConsent::Disabled,
    );
    assert!(reporter.is_none(), "Disabled consent must hold no reporter");

    let capturer = std::sync::Arc::new(CapturingReporter::default());
    let mut app = test_app(Vec::new(), Vec::new());
    let capturer_for_app = std::sync::Arc::clone(&capturer);
    app.error_reporter = Some(capturer_for_app as std::sync::Arc<dyn avcore::ErrorReporter>);
    assert!(app.report_error(
        avcore::ErrorCode::ProjectSave,
        avcore::ErrorSeverity::Error,
        avcore::Operation::ProjectSave,
        avcore::RecoveryOutcome::RequiresUserAction,
        false,
    ));

    let report = capturer.reports.lock().unwrap().pop().unwrap();
    assert_eq!(report.locale, "en");
    assert!(
        report.breadcrumbs.iter().any(
            |b| matches!(b, avcore::Breadcrumb::StateTransition { state } if state == "app_started")
        ),
        "seed_error_reporting must record the launch breadcrumb"
    );
    assert!(report.release.starts_with("oca-"));
}

fn sample_pending_crash(timestamp: u64) -> crate::app::crash_review::PendingCrashReview {
    crate::app::crash_review::PendingCrashReview::new(
        timestamp,
        "1.4.2".to_owned(),
        "crates/ui/src/app/mod.rs:1:1".to_owned(),
        "index out of bounds".to_owned(),
        "0: oca::main".to_owned(),
    )
}

#[test]
fn crash_review_payload_preview_is_none_without_a_pending_crash() {
    let app = test_app(Vec::new(), Vec::new());
    assert!(app.crash_review_payload_preview().is_none());
}

#[test]
fn crash_review_payload_preview_shows_the_exact_post_sanitization_payload() {
    let mut app = test_app(Vec::new(), Vec::new());
    app.pending_crash_review = Some(sample_pending_crash(1_700_000_000));

    let preview = app
        .crash_review_payload_preview()
        .expect("a pending crash must produce a payload preview");
    let value: serde_json::Value = serde_json::from_str(&preview).unwrap();
    assert_eq!(value["tags"]["error_code"], "panic");
    assert_eq!(value["tags"]["operation"], "app");
    assert_eq!(value["release"], "oca-1.4.2");
    let stack = value["extra"]["stack_trace"].as_str().unwrap();
    assert!(stack.contains("index out of bounds"));
    assert!(stack.contains("crates/ui/src/app/mod.rs:1:1"));
}

#[test]
fn crash_review_payload_preview_keeps_event_identity_stable_across_frames() {
    let mut app = test_app(Vec::new(), Vec::new());
    app.pending_crash_review = Some(sample_pending_crash(1_700_000_000));

    let first: serde_json::Value =
        serde_json::from_str(&app.crash_review_payload_preview().unwrap()).unwrap();
    let second: serde_json::Value =
        serde_json::from_str(&app.crash_review_payload_preview().unwrap()).unwrap();

    assert_eq!(first["event_id"], second["event_id"]);
    assert_eq!(
        first["extra"]["oca_event_id"],
        second["extra"]["oca_event_id"]
    );
    assert_eq!(
        first["extra"]["oca_event_id"],
        app.pending_crash_review.unwrap().report.event_id
    );
}

#[test]
fn dismiss_pending_crash_review_clears_state_without_sending_anything() {
    let mut app = test_app(Vec::new(), Vec::new());
    let capturer = std::sync::Arc::new(CapturingReporter::default());
    app.error_reporter =
        Some(std::sync::Arc::clone(&capturer) as std::sync::Arc<dyn avcore::ErrorReporter>);
    app.pending_crash_review = Some(sample_pending_crash(1_700_000_042));

    app.dismiss_pending_crash_review();

    assert!(app.pending_crash_review.is_none());
    assert_eq!(app.prefs.last_reviewed_crash_unix, 1_700_000_042);
    assert!(
        capturer.reports.lock().unwrap().is_empty(),
        "Do not send must never report anything"
    );
    assert_eq!(
        app.prefs.error_reporting_consent,
        crate::app::error_reporting::ErrorReportingConsent::Disabled,
        "dismissing must not change the steady-state consent preference"
    );
}

#[test]
fn always_send_pending_crash_opts_in_and_reports_through_the_new_consent() {
    let mut app = test_app(Vec::new(), Vec::new());
    app.pending_crash_review = Some(sample_pending_crash(1_700_000_100));

    app.always_send_pending_crash();

    assert!(app.pending_crash_review.is_none());
    assert_eq!(app.prefs.last_reviewed_crash_unix, 1_700_000_100);
    assert_eq!(
        app.prefs.error_reporting_consent,
        crate::app::error_reporting::ErrorReportingConsent::AlwaysSend
    );
    assert!(
        app.error_reporter.is_some(),
        "opting in must leave a live reporter wired for future reports"
    );
}

#[test]
fn send_pending_crash_once_marks_reviewed_without_changing_steady_state_consent() {
    let mut app = test_app(Vec::new(), Vec::new());
    app.pending_crash_review = Some(sample_pending_crash(1_700_000_200));

    app.send_pending_crash_once();

    assert!(app.pending_crash_review.is_none());
    assert_eq!(app.prefs.last_reviewed_crash_unix, 1_700_000_200);
    assert_eq!(
        app.prefs.error_reporting_consent,
        crate::app::error_reporting::ErrorReportingConsent::Disabled,
        "Send once must stay a one-off action, never flipping the steady-state preference"
    );
}

#[test]
fn crash_review_actions_are_a_noop_without_a_pending_crash() {
    let mut app = test_app(Vec::new(), Vec::new());
    app.prefs.last_reviewed_crash_unix = 5;

    app.send_pending_crash_once();
    app.always_send_pending_crash();
    app.dismiss_pending_crash_review();

    assert!(app.pending_crash_review.is_none());
    assert_eq!(
        app.prefs.last_reviewed_crash_unix, 5,
        "with nothing pending, none of the three actions should touch the reviewed marker"
    );
}

fn text_template_element(id: &str, text: TextBinding) -> TemplateElement {
    TemplateElement::Text(TemplateTextElement {
        id: id.to_string(),
        text,
        color_rgba: ColorBinding::Fixed([255, 255, 255, 255]),
        font_family: Default::default(),
        font_style: Default::default(),
        font_size: 32.0,
        pos_x: 0.2,
        pos_y: 0.8,
        timing: Default::default(),
    })
}

fn shape_template_element(id: &str) -> TemplateElement {
    TemplateElement::Shape(TemplateShapeElement {
        id: id.to_string(),
        shape_kind: ShapeKind::rectangle(),
        color_rgba: ColorBinding::Fixed([0, 0, 0, 255]),
        center_x: 0.5,
        center_y: 0.5,
        width: 0.3,
        height: 0.1,
        rotation_deg: 0.0,
        stroke_thickness_px: 0.0,
    })
}

fn minimal_graphic_template(elements: Vec<TemplateElement>) -> GraphicTemplate {
    GraphicTemplate {
        schema_version: avcore::motion_template::TEMPLATE_SCHEMA_VERSION,
        name: "Test template".to_string(),
        canvas_width: 1920,
        canvas_height: 1080,
        safe_area_margin: 0.0,
        parameters: vec![TemplateParameter {
            id: "player_name".to_string(),
            label: "Player name".to_string(),
            kind: TemplateParameterKind::Text,
        }],
        elements,
    }
}

#[test]
fn apply_graphic_template_places_a_text_element_at_the_playhead() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    app.active_project_mut().timeline_mut().playhead_secs = 5.0;
    let template = minimal_graphic_template(vec![text_template_element(
        "e1",
        TextBinding::Fixed("PacoPaçoca".to_string()),
    )]);

    app.apply_graphic_template(&template, &HashMap::new());

    let tracks = &app.active_project().timeline().tracks;
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].kind, TrackKind::Text);
    assert_eq!(tracks[0].text_clips.len(), 1);
    let clip = &tracks[0].text_clips[0];
    assert_eq!(clip.start_secs, 5.0);
    assert_eq!(clip.text, "PacoPaçoca");
    assert_eq!(clip.pos_x, 0.2);
    assert_eq!(clip.pos_y, 0.8);
}

#[test]
fn apply_graphic_template_places_a_shape_element_on_its_own_track() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let template = minimal_graphic_template(vec![shape_template_element("s1")]);

    app.apply_graphic_template(&template, &HashMap::new());

    let tracks = &app.active_project().timeline().tracks;
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].kind, TrackKind::Shape);
    assert_eq!(tracks[0].shape_clips.len(), 1);
}

#[test]
fn apply_graphic_template_places_text_and_shape_elements_on_separate_tracks() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let template = minimal_graphic_template(vec![
        text_template_element("e1", TextBinding::Fixed("Hi".to_string())),
        shape_template_element("s1"),
    ]);

    app.apply_graphic_template(&template, &HashMap::new());

    let tracks = &app.active_project().timeline().tracks;
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].kind, TrackKind::Text);
    assert_eq!(tracks[1].kind, TrackKind::Shape);
}

#[test]
fn apply_graphic_template_resolves_a_parameter_bound_text_value() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let template = minimal_graphic_template(vec![text_template_element(
        "e1",
        TextBinding::Parameter("player_name".to_string()),
    )]);
    let mut values = HashMap::new();
    values.insert(
        "player_name".to_string(),
        ParameterValue::Text("Zé".to_string()),
    );

    app.apply_graphic_template(&template, &values);

    let clip = &app.active_project().timeline().tracks[0].text_clips[0];
    assert_eq!(clip.text, "Zé");
}

#[test]
fn apply_graphic_template_toasts_and_makes_no_change_on_a_missing_parameter_value() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let template = minimal_graphic_template(vec![text_template_element(
        "e1",
        TextBinding::Parameter("player_name".to_string()),
    )]);

    app.apply_graphic_template(&template, &HashMap::new());

    assert!(app.active_project().timeline().tracks.is_empty());
    assert_eq!(app.toasts.len(), 1);
}

#[test]
fn apply_graphic_template_pushes_exactly_one_undo_snapshot_for_the_whole_batch() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    app.undo_stack.clear();
    let template = minimal_graphic_template(vec![
        text_template_element("e1", TextBinding::Fixed("Hi".to_string())),
        shape_template_element("s1"),
    ]);

    app.apply_graphic_template(&template, &HashMap::new());
    assert!(app.undo_stack.can_undo());

    let sequence = app.active_project().sequences[app.active_project().active_sequence].clone();
    let restored = app
        .undo_stack
        .undo(sequence)
        .expect("one snapshot was pushed");
    assert!(
        restored.timeline.tracks.is_empty(),
        "undoing the apply should restore the pre-apply (empty) timeline"
    );
    assert!(
        !app.undo_stack.can_undo(),
        "exactly one snapshot should have been pushed for the whole batch"
    );
}

#[test]
fn load_graphic_template_from_file_applies_a_parameterless_template_immediately() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let template = minimal_graphic_template(vec![text_template_element(
        "e1",
        TextBinding::Fixed("Hi".to_string()),
    )]);
    // No parameters at all this time -- overrides the fixture's default one.
    let mut template = template;
    template.parameters = vec![];
    let json = serde_json::to_string(&template).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("template.json");
    std::fs::write(&path, json).unwrap();

    app.load_graphic_template_from_file(path);

    assert!(app.pending_graphic_template_apply.is_none());
    assert_eq!(
        app.active_project().timeline().tracks[0].text_clips.len(),
        1
    );
}

#[test]
fn load_graphic_template_from_file_stages_a_parameterized_template_instead_of_applying() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let template = minimal_graphic_template(vec![text_template_element(
        "e1",
        TextBinding::Parameter("player_name".to_string()),
    )]);
    let json = serde_json::to_string(&template).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("template.json");
    std::fs::write(&path, json).unwrap();

    app.load_graphic_template_from_file(path);

    assert!(app.active_project().timeline().tracks.is_empty());
    let pending = app
        .pending_graphic_template_apply
        .as_ref()
        .expect("a parameterized template should be staged, not applied");
    assert!(pending.text_values.contains_key("player_name"));
}

#[test]
fn load_graphic_template_from_file_toasts_on_invalid_json() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("template.json");
    std::fs::write(&path, "not json").unwrap();

    app.load_graphic_template_from_file(path);

    assert!(app.pending_graphic_template_apply.is_none());
    assert!(app.active_project().timeline().tracks.is_empty());
    assert_eq!(app.toasts.len(), 1);
}

#[test]
fn confirm_apply_graphic_template_applies_the_staged_template_with_filled_values() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let template = minimal_graphic_template(vec![text_template_element(
        "e1",
        TextBinding::Parameter("player_name".to_string()),
    )]);
    let mut text_values = HashMap::new();
    text_values.insert("player_name".to_string(), "Zé".to_string());
    app.pending_graphic_template_apply = Some(PendingGraphicTemplateApply {
        template,
        text_values,
        color_values: HashMap::new(),
    });

    app.confirm_apply_graphic_template();

    assert!(app.pending_graphic_template_apply.is_none());
    let clip = &app.active_project().timeline().tracks[0].text_clips[0];
    assert_eq!(clip.text, "Zé");
}

#[test]
fn cancel_apply_graphic_template_discards_the_staged_template_without_applying() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    let template = minimal_graphic_template(vec![text_template_element(
        "e1",
        TextBinding::Parameter("player_name".to_string()),
    )]);
    app.pending_graphic_template_apply = Some(PendingGraphicTemplateApply {
        template,
        text_values: HashMap::new(),
        color_values: HashMap::new(),
    });

    app.cancel_apply_graphic_template();

    assert!(app.pending_graphic_template_apply.is_none());
    assert!(app.active_project().timeline().tracks.is_empty());
}

#[test]
fn apply_effect_preset_blur_sets_the_default_intensity() {
    use crate::app::effects_panel::{EffectPreset, EFFECT_DEFAULT_INTENSITY};

    let mut app = test_app(
        vec![test_project_with_tracks(
            1,
            vec![test_track(
                1,
                TrackKind::Video,
                vec![test_clip(1, 0.0, 0.0, 10.0)],
            )],
        )],
        Vec::new(),
    );
    app.selected_clip_id = Some(1);

    app.apply_effect_preset(EffectPreset::Blur);

    let clips = &app.active_project().timeline().tracks[0].clips;
    assert_eq!(clips[0].blur_intensity, EFFECT_DEFAULT_INTENSITY);
}

#[test]
fn apply_effect_preset_black_and_white_sets_the_color_filter() {
    use crate::app::effects_panel::EffectPreset;
    use avcore::timeline::ColorFilter;

    let mut app = test_app(
        vec![test_project_with_tracks(
            1,
            vec![test_track(
                1,
                TrackKind::Video,
                vec![test_clip(1, 0.0, 0.0, 10.0)],
            )],
        )],
        Vec::new(),
    );
    app.selected_clip_id = Some(1);

    app.apply_effect_preset(EffectPreset::BlackAndWhite);

    let clips = &app.active_project().timeline().tracks[0].clips;
    assert_eq!(clips[0].color_filter, ColorFilter::BlackAndWhite);
}

#[test]
fn apply_effect_preset_chroma_key_enables_it_without_touching_existing_color_or_tolerance() {
    use crate::app::effects_panel::EffectPreset;

    let mut app = test_app(
        vec![test_project_with_tracks(
            1,
            vec![test_track(
                1,
                TrackKind::Video,
                vec![test_clip(1, 0.0, 0.0, 10.0)],
            )],
        )],
        Vec::new(),
    );
    app.selected_clip_id = Some(1);
    app.set_selected_clip_chroma_key(false, [10, 20, 30], 0.42);

    app.apply_effect_preset(EffectPreset::ChromaKey);

    let clips = &app.active_project().timeline().tracks[0].clips;
    assert!(clips[0].chroma_key_enabled);
    assert_eq!(clips[0].chroma_key_color, [10, 20, 30]);
    assert_eq!(clips[0].chroma_key_tolerance, 0.42);
}

#[test]
fn apply_effect_preset_is_a_no_op_when_nothing_is_selected() {
    use crate::app::effects_panel::EffectPreset;

    let mut app = test_app(
        vec![test_project_with_tracks(
            1,
            vec![test_track(
                1,
                TrackKind::Video,
                vec![test_clip(1, 0.0, 0.0, 10.0)],
            )],
        )],
        Vec::new(),
    );

    app.apply_effect_preset(EffectPreset::Blur);

    let clips = &app.active_project().timeline().tracks[0].clips;
    assert_eq!(clips[0].blur_intensity, 0.0);
}

#[test]
fn materialize_nested_sequences_for_active_sequence_is_a_no_op_without_any_nested_clips() {
    let mut app = test_app(
        vec![test_project_with_tracks(
            1,
            vec![test_track(
                1,
                TrackKind::Video,
                vec![test_clip(1, 0.0, 0.0, 10.0)],
            )],
        )],
        Vec::new(),
    );

    let assets = app.materialize_nested_sequences_for_active_sequence();

    assert!(assets.is_empty());
    assert!(app
        .nested_sequence_render_state
        .nested_sequence_rendering_ids
        .is_empty());
}

#[test]
fn materialize_nested_sequences_for_active_sequence_returns_cached_result_when_input_unchanged() {
    let mut clip = test_clip(1, 0.0, 0.0, 10.0);
    clip.nested_sequence_id = Some(99);
    let mut app = test_app(
        vec![test_project_with_tracks(
            1,
            vec![test_track(1, TrackKind::Video, vec![clip])],
        )],
        Vec::new(),
    );
    let sequences_input = app.active_project().sequences.clone();
    app.nested_sequence_render_state
        .nested_sequence_last_input
        .insert(1, sequences_input);
    app.nested_sequence_render_state
        .nested_sequence_last_result
        .insert(1, vec![]);

    let assets = app.materialize_nested_sequences_for_active_sequence();

    assert!(assets.is_empty());
    // Unchanged input -- must not have dispatched a fresh background render.
    assert!(app
        .nested_sequence_render_state
        .nested_sequence_rendering_ids
        .is_empty());
}

#[test]
fn materialize_nested_sequences_for_active_sequence_dispatches_a_background_render_for_a_nested_clip(
) {
    let mut clip = test_clip(1, 0.0, 0.0, 10.0);
    clip.nested_sequence_id = Some(99);
    let mut app = test_app(
        vec![test_project_with_tracks(
            1,
            vec![test_track(1, TrackKind::Video, vec![clip])],
        )],
        Vec::new(),
    );

    let assets = app.materialize_nested_sequences_for_active_sequence();

    // No prior result cached -- returns empty immediately while the background render runs.
    assert!(assets.is_empty());
    assert!(app
        .nested_sequence_render_state
        .nested_sequence_rendering_ids
        .contains(&1));
}

#[test]
fn pump_nested_sequence_renders_applies_a_ready_event() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    app.nested_sequence_render_state
        .nested_sequence_rendering_ids
        .insert(1);
    let asset = test_asset(1000);
    app.nested_sequence_render_state
        .nested_sequence_tx
        .send(crate::app::NestedSequenceEvent::Ready {
            sequence_id: 1,
            cache: std::collections::HashMap::new(),
            assets: vec![asset.clone()],
            sequences_input: app.active_project().sequences.clone(),
        })
        .unwrap();

    app.pump_nested_sequence_renders();

    assert_eq!(
        app.nested_sequence_render_state
            .nested_sequence_last_result
            .get(&1),
        Some(&vec![asset])
    );
    assert!(app
        .nested_sequence_render_state
        .nested_sequence_last_input
        .contains_key(&1));
    assert!(!app
        .nested_sequence_render_state
        .nested_sequence_rendering_ids
        .contains(&1));
}

#[test]
fn pump_nested_sequence_renders_applies_a_failed_event_and_latches_the_input() {
    let mut app = test_app(vec![test_project(1, Vec::new())], Vec::new());
    app.nested_sequence_render_state
        .nested_sequence_rendering_ids
        .insert(1);
    let sequences_input = app.active_project().sequences.clone();
    app.nested_sequence_render_state
        .nested_sequence_tx
        .send(crate::app::NestedSequenceEvent::Failed {
            sequence_id: 1,
            error: "missing nested sequence".to_string(),
            sequences_input: sequences_input.clone(),
        })
        .unwrap();

    app.pump_nested_sequence_renders();

    assert!(!app
        .nested_sequence_render_state
        .nested_sequence_last_result
        .contains_key(&1));
    assert_eq!(
        app.nested_sequence_render_state
            .nested_sequence_last_input
            .get(&1),
        Some(&sequences_input)
    );
    assert!(!app
        .nested_sequence_render_state
        .nested_sequence_rendering_ids
        .contains(&1));
}
