use std::sync::{Arc, Mutex};

use tauri::Listener;

use super::*;
use crate::testing::{harness, path_in, Harness};

fn collect_requests(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    h.app.listen(CLOSE_REQUESTED_EVENT, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });
    seen
}

fn wait_until_nothing_is_pending(h: &Harness) {
    let started = std::time::Instant::now();
    while pending_loss(&h.handle()).is_some() {
        assert!(started.elapsed() < std::time::Duration::from_secs(120));
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn a_clean_project_with_no_exports_closes_without_asking() {
    let h = harness();
    let seen = collect_requests(&h);
    h.session().dirty = false;

    assert_eq!(pending_loss(&h.handle()), None);
    assert!(!guard_close(&h.handle()));
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn unsaved_changes_hold_the_close_back_and_ask_the_webview() {
    let h = harness();
    let seen = collect_requests(&h);
    h.session().dirty = true;

    assert_eq!(
        pending_loss(&h.handle()),
        Some(CloseRequest {
            dirty: true,
            exporting: false
        })
    );
    assert!(guard_close(&h.handle()));
    let payloads = seen.lock().unwrap();
    assert_eq!(payloads.len(), 1);
    assert!(
        payloads[0].contains("\"dirty\":true"),
        "got {}",
        payloads[0]
    );
    assert!(payloads[0].contains("\"exporting\":false"));
}

#[test]
fn a_pending_export_holds_the_close_back_until_it_is_done() {
    let h = harness();
    h.add_fixture_clip();
    h.session().dirty = false;
    let output = path_in(&h.dir, "closing.mp4");
    crate::commands::export::start_export(h.handle(), output.display().to_string(), false).unwrap();

    let request = pending_loss(&h.handle()).expect("the export is still running");
    assert!(request.exporting);
    assert!(!request.dirty);

    wait_until_nothing_is_pending(&h);
    assert!(
        !guard_close(&h.handle()),
        "a finished export no longer blocks"
    );
}

#[test]
fn a_failed_export_does_not_block_closing() {
    let h = harness();
    h.add_fixture_clip();
    h.session().dirty = false;
    let blocked = path_in(&h.dir, "no-such-dir").join("out.mp4");
    crate::commands::export::start_export(h.handle(), blocked.display().to_string(), false)
        .unwrap();

    wait_until_nothing_is_pending(&h);
    let jobs = crate::commands::export::list_exports(h.state());
    assert!(matches!(
        jobs[0].status,
        avcore::ExportJobStatus::Failed { .. }
    ));
}
