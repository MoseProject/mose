//! Harness for tests that run commands against Tauri's mock runtime: a real [`AppState`] around
//! a blank project, with the preview engine detached (its commands are collected on a channel)
//! and every user directory pointed at a scratch folder, so no test touches real preferences.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, MutexGuard, Once};

use tauri::test::{mock_app, MockRuntime};
use tauri::{App, AppHandle, Manager, State};

use crate::export::ExportQueue;
use crate::prefs::PrefsDocument;
use crate::preview_engine::{Command, PreviewEngine};
use crate::state::{AppState, Session};

static ISOLATE: Once = Once::new();
static NEXT_DIR: AtomicU32 = AtomicU32::new(0);

/// Scratch root shared by every test of this run.
fn scratch_root() -> PathBuf {
    std::env::temp_dir().join(format!("oca-desktop-test-{}", std::process::id()))
}

/// Points the per-user config locations (`prefs.oc`) and the error queue at the scratch root.
fn isolate_user_dirs() {
    ISOLATE.call_once(|| {
        let root = scratch_root();
        std::fs::create_dir_all(&root).expect("creating the scratch root");
        std::env::set_var("APPDATA", &root);
        std::env::set_var("HOME", &root);
        std::env::set_var("OCA_ERROR_QUEUE_DIR", root.join("error-queue"));
        std::env::remove_var("OCA_SENTRY_DSN");
    });
}

pub(crate) struct Harness {
    pub app: App<MockRuntime>,
    /// Every command the code under test sent to the preview engine, oldest first.
    pub preview: Receiver<Command>,
    /// A fresh directory private to this harness.
    pub dir: PathBuf,
}

impl Harness {
    pub fn handle(&self) -> AppHandle<MockRuntime> {
        self.app.handle().clone()
    }

    pub fn state(&self) -> State<'_, AppState> {
        self.app.state::<AppState>()
    }

    /// The live session, locked. `State::inner` ties the guard to the app, not to a temporary.
    pub fn session(&self) -> MutexGuard<'_, Session> {
        self.state().inner().session()
    }

    pub fn prefs(&self) -> MutexGuard<'_, PrefsDocument> {
        self.state().inner().prefs()
    }

    /// The preview commands sent so far, draining the channel.
    pub fn drain_preview(&self) -> Vec<Command> {
        self.preview.try_iter().collect()
    }
}

pub(crate) fn harness() -> Harness {
    isolate_user_dirs();
    let dir = scratch_root().join(format!("h{}", NEXT_DIR.fetch_add(1, Ordering::SeqCst)));
    std::fs::create_dir_all(&dir).expect("creating the harness directory");
    let app = mock_app();
    let (engine, preview) = PreviewEngine::detached();
    app.manage(AppState::new(
        PrefsDocument::default(),
        engine,
        Arc::new(ExportQueue::load(dir.join("config"))),
        None,
        None,
        Arc::new(AtomicBool::new(true)),
    ));
    Harness { app, preview, dir }
}

/// `dir/name`, for tests that need a path next to the harness's other files.
pub(crate) fn path_in(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}
