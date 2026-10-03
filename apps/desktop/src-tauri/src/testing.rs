//! Harness for tests that run commands against Tauri's mock runtime: a real [`AppState`] around
//! a blank project, with the preview engine detached (its commands are collected on a channel)
//! and every user directory pointed at a scratch folder, so no test touches real preferences.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, MutexGuard, Once};

use tauri::test::{mock_app, MockRuntime};
use tauri::{App, AppHandle, Listener, Manager, State};

use crate::export::ExportQueue;
use crate::prefs::PrefsDocument;
use crate::preview_engine::{Command, PreviewEngine};
use crate::services::Services;
use crate::state::{AppState, Session, SESSION_CHANGED};

/// How long a test waits for a background worker before giving up.
const WAIT: std::time::Duration = std::time::Duration::from_secs(120);

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
        std::env::remove_var("OCA_SENTRY_DSN");
    });
}

/// Stands in for [`crate::services::SystemServices`]: each call returns what the test queued
/// for it, or a failure when nothing was queued.
#[derive(Default)]
pub(crate) struct FakeServices {
    pub release: Mutex<Option<Result<avcore::LatestRelease, avcore::UpdateCheckError>>>,
    pub update: Mutex<Option<Result<avcore::ApplyUpdateOutcome, avcore::ApplyUpdateError>>>,
    pub restart: Mutex<Option<Result<(), avcore::ApplyUpdateError>>>,
    pub restarts: AtomicUsize,
    pub exits: Mutex<Vec<i32>>,
    pub speech: Mutex<Option<Result<Vec<f32>, avcore::TtsError>>>,
}

impl Services for FakeServices {
    fn fetch_latest_release(&self) -> Result<avcore::LatestRelease, avcore::UpdateCheckError> {
        self.release
            .lock()
            .unwrap()
            .take()
            .unwrap_or_else(|| Err(avcore::UpdateCheckError::Request("offline".into())))
    }

    fn apply_update(
        &self,
        _version: &str,
    ) -> Result<avcore::ApplyUpdateOutcome, avcore::ApplyUpdateError> {
        self.update
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Err(avcore::ApplyUpdateError::UnsupportedPlatform))
    }

    fn restart_application(&self) -> Result<(), avcore::ApplyUpdateError> {
        self.restarts.fetch_add(1, Ordering::SeqCst);
        self.restart.lock().unwrap().take().unwrap_or(Ok(()))
    }

    fn exit(&self, code: i32) {
        self.exits.lock().unwrap().push(code);
    }

    fn synthesize(
        &self,
        _model_path: &Path,
        _config: &avcore::PiperVoiceConfig,
        _text: &str,
    ) -> Result<Vec<f32>, avcore::TtsError> {
        self.speech
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Ok(vec![0.0; 1600]))
    }
}

pub(crate) struct Harness {
    pub app: App<MockRuntime>,
    /// What the fake services answer; queue results here before running a command.
    pub services: Arc<FakeServices>,
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

    /// Counts the `session-changed` events published from now on: one per command that
    /// changed the session, none for a rejected or no-op edit.
    pub fn count_session_events(&self) -> Arc<AtomicUsize> {
        let seen = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&seen);
        self.app.listen(SESSION_CHANGED, move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        seen
    }

    /// Moves the preview engine's live playhead, the one edits such as split-at-playhead read.
    pub fn set_playhead(&self, secs: f64) {
        self.state().preview.set_playhead(secs);
    }

    /// Puts `video.mp4` in the library and one clip of it on the timeline. Returns the asset
    /// and clip ids.
    pub fn add_fixture_clip(&self) -> (u64, u64) {
        let path = fixture("video.mp4");
        let probed = avcore::probe_media(&path).expect("probing the shipped fixture");
        let asset_id = {
            let mut session = self.session();
            let id = session
                .project
                .media_library
                .iter()
                .map(|a| a.id)
                .max()
                .unwrap_or(0)
                + 1;
            let asset = probed.into_media_asset(id, "video.mp4".into(), path);
            session.project.media_library.push(asset);
            id
        };
        crate::commands::timeline::add_asset_to_timeline(self.handle(), asset_id, None, None)
            .expect("adding the fixture clip");
        let clip_id = self
            .session()
            .project
            .timeline()
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .map(|c| c.id)
            .max()
            .expect("the clip was added");
        (asset_id, clip_id)
    }

    /// Polls until `done` holds for the session, for work that finishes on a background thread.
    /// Panics with `what` after `WAIT` so a stuck worker fails the test instead of hanging it.
    pub fn wait_until(&self, what: &str, done: impl Fn(&Session) -> bool) {
        let started = std::time::Instant::now();
        while !done(&self.session()) {
            assert!(started.elapsed() < WAIT, "timed out waiting for {what}");
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
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
    let services = Arc::new(FakeServices::default());
    let (engine, preview) = PreviewEngine::detached();
    app.manage(AppState::new(
        PrefsDocument::default(),
        engine,
        Arc::new(ExportQueue::load(dir.join("config"))),
        None,
        None,
        Arc::new(AtomicBool::new(true)),
        Arc::clone(&services) as Arc<dyn Services>,
    ));
    Harness {
        app,
        services,
        preview,
        dir,
    }
}

/// A video asset that exists only in the library (nothing on disk behind `source_path`).
pub(crate) fn video_asset(id: u64, duration_secs: f64) -> avcore::MediaAsset {
    avcore::MediaAsset {
        id,
        file_name: format!("asset-{id}.mp4"),
        source_path: PathBuf::from(format!("asset-{id}.mp4")),
        kind: avcore::MediaKind::Video,
        has_audio: true,
        duration_secs,
        codec: "h264".into(),
        source_bitrate_mbps: 8.0,
        resolution: None,
        fps: None,
        sample_rate_khz: None,
        loudness: None,
        proxy_path: None,
        waveform_peaks: None,
        favorited: false,
    }
}

/// A media file shipped with `avcore`'s own tests (`video.mp4`, `video_silent.mp4`, `audio.m4a`).
pub(crate) fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../crates/core/tests/fixtures")
        .join(name)
}

/// `dir/name`, for tests that need a path next to the harness's other files.
pub(crate) fn path_in(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}
