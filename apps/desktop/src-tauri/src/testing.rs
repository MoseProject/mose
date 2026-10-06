//! Harness for tests that run commands against Tauri's mock runtime: a real [`AppState`] around
//! a blank project, with the preview engine detached (its commands are collected on a channel)
//! and every user directory pointed at a scratch folder, so no test touches real preferences.

use crate::preview::mailbox::Receiver;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Once};

use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::{App, AppHandle, Listener, Manager, State};

use crate::export::ExportQueue;
use crate::prefs::PrefsDocument;
use crate::preview_engine::{Command, PreviewEngine};
use crate::services::Services;
use crate::state::{AppState, Session, SESSION_CHANGED, SESSION_PATCH};

/// Observes asynchronous session events only after pending publications complete.
pub struct SessionEventCounter {
    seen: Arc<AtomicUsize>,
    app: AppHandle<MockRuntime>,
}
impl SessionEventCounter {
    pub fn load(&self, ordering: Ordering) -> usize {
        self.app.state::<AppState>().publication.flush();
        self.seen.load(ordering)
    }
}

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
        // Log everything: macro arguments are only evaluated for levels that are enabled, and
        // the code under test should run them the way a debug build would.
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_test_writer()
            .try_init();
        let root = scratch_root();
        std::fs::create_dir_all(&root).expect("creating the scratch root");
        std::env::set_var("APPDATA", &root);
        std::env::set_var("HOME", &root);
        std::env::remove_var("OCA_SENTRY_DSN");
    });
}

pub(crate) type WatchResult =
    Result<Option<(avcore::LoudnessMetrics, avcore::LoudnessMetrics)>, avcore::ProcessError>;

/// A scripted download: gets the URL, target, cancel flag and progress callback.
pub(crate) type YoutubeScript = Box<
    dyn FnOnce(
            &str,
            avcore::YoutubeDownloadTarget,
            &AtomicBool,
            &mut dyn FnMut(f32),
        ) -> Result<PathBuf, avcore::YoutubeDownloadError>
        + Send,
>;

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
    pub whisper_model: Mutex<Option<PathBuf>>,
    /// What the next watched-file normalization returns; absent runs the real one.
    pub watch_result: Mutex<Option<WatchResult>>,
    /// Whether the YouTube helper "is installed".
    pub youtube_available: std::sync::atomic::AtomicBool,
    /// What the next download does; absent means the helper reports the file as missing.
    pub youtube: Mutex<Option<YoutubeScript>>,
    /// Faces every detection returns; `None` makes detection fail.
    pub faces: Mutex<Option<Vec<avcore::FaceBox>>>,
    /// Alpha every segmented pixel gets; `None` makes segmentation fail.
    pub person_alpha: Mutex<Option<f32>>,
    pub transcript: Mutex<Option<Result<avcore::TranscribeOutcome, avcore::TranscribeError>>>,
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

    fn segment_person(
        &self,
        _model_path: &Path,
        _rgba: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<f32>, avcore::SegmentError> {
        match *self.person_alpha.lock().unwrap() {
            Some(alpha) => Ok(vec![alpha; (width * height) as usize]),
            None => Err(avcore::SegmentError::Onnx("no model".into())),
        }
    }

    fn detect_faces(
        &self,
        _model_path: &Path,
        _rgba: &[u8],
        _width: u32,
        _height: u32,
    ) -> Result<Vec<avcore::FaceBox>, avcore::ReframeError> {
        self.faces
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| avcore::ReframeError::Onnx("no model".into()))
    }

    fn process_watched_file(
        &self,
        source: &Path,
        output: &Path,
        target_lufs: f32,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(u8),
    ) -> Result<Option<(avcore::LoudnessMetrics, avcore::LoudnessMetrics)>, avcore::ProcessError>
    {
        match self.watch_result.lock().unwrap().take() {
            Some(result) => result,
            None => avcore::process_watched_file(source, output, target_lufs, cancel, progress),
        }
    }

    fn is_yt_dlp_available(&self) -> bool {
        self.youtube_available.load(Ordering::SeqCst)
    }

    fn download_youtube(
        &self,
        url: &str,
        target: avcore::YoutubeDownloadTarget,
        _dir: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(f32),
    ) -> Result<PathBuf, avcore::YoutubeDownloadError> {
        match self.youtube.lock().unwrap().take() {
            Some(script) => script(url, target, cancel, progress),
            None => Err(avcore::YoutubeDownloadError::ToolNotFound),
        }
    }

    fn bundled_whisper_model(&self) -> Option<PathBuf> {
        self.whisper_model.lock().unwrap().clone()
    }

    fn transcribe(
        &self,
        _source: &Path,
        _model: &Path,
        _cancel: &Arc<AtomicBool>,
    ) -> Result<avcore::TranscribeOutcome, avcore::TranscribeError> {
        self.transcript
            .lock()
            .unwrap()
            .take()
            .unwrap_or(Err(avcore::TranscribeError::NoAudio))
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
    /// A window of the mock app, the target of [`Harness::ipc`].
    window: tauri::WebviewWindow<MockRuntime>,
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

    /// Invokes a command the way the webview does: through the real handler, with the
    /// arguments as camelCase JSON. `Err` carries what the webview's promise would reject with.
    pub fn ipc(
        &self,
        command: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value, serde_json::Value> {
        let request = tauri::webview::InvokeRequest {
            cmd: command.into(),
            callback: tauri::ipc::CallbackFn(0),
            error: tauri::ipc::CallbackFn(1),
            url: "http://tauri.localhost".parse().unwrap(),
            body: tauri::ipc::InvokeBody::Json(arguments),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        };
        tauri::test::get_ipc_response(&self.window, request)
            .map(|body| body.deserialize::<serde_json::Value>().unwrap())
    }

    /// Counts completed session publications from now on. Pending snapshots may coalesce.
    pub fn count_session_events(&self) -> SessionEventCounter {
        self.state().publication.flush();
        let seen = Arc::new(AtomicUsize::new(0));
        for event in [SESSION_CHANGED, SESSION_PATCH] {
            let counter = Arc::clone(&seen);
            self.app.listen(event, move |_| {
                counter.fetch_add(1, Ordering::SeqCst);
            });
        }
        SessionEventCounter {
            seen,
            app: self.app.handle().clone(),
        }
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
    let app = mock_builder()
        .invoke_handler(crate::handler::all_commands!())
        .build(mock_context(noop_assets()))
        .expect("building the mock app");
    let window = tauri::WebviewWindowBuilder::new(&app, "main", tauri::WebviewUrl::default())
        .build()
        .expect("opening the mock window");
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
        window,
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
