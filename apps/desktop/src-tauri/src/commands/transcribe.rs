//! Automatic speech transcription (CF-01, `spec/architecture/competitive-feature-plan.md`).
//! Runs Whisper on a background thread; on completion both creates subtitle `TextClip`s
//! (word-highlight, mirroring `ui::App::apply_transcription`) and saves a `TranscriptDocument`
//! sidecar the Transcript panel reads -- same split egui's own `App::pump_transcribe` /
//! `App::save_transcript_document_for` keep.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use avcore::timeline::{TextClip, Timeline, TrackKind, WordTiming};
use avcore::{ProjectTranscriptHit, TranscribeOutcome, TranscribeSegment, TranscriptDocument};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use super::with_session;
use crate::services::Services;
use crate::state::AppState;

pub const TRANSCRIBE_EVENT: &str = "transcribe-event";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TranscribeEventPayload {
    Started { asset_id: u64 },
    Done { asset_id: u64 },
    NoSpeech { asset_id: u64 },
    Failed { asset_id: u64, message: String },
}

/// [`ProjectTranscriptHit`] doesn't derive `Serialize` (it's an internal `core` search result,
/// not persisted), so the command surface mirrors it into its own view type.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptHitView {
    pub asset_id: u64,
    pub asset_name: String,
    pub word: avcore::TranscriptWord,
}

/// Transcribes `asset_id`'s audio on a background thread -- what the media library's "📝"
/// button does. Errors (already transcribing, asset not found, no model configured) surface
/// synchronously; once running, progress/result arrive via [`TRANSCRIBE_EVENT`].
#[tauri::command]
pub fn transcribe_asset<R: Runtime>(app: AppHandle<R>, asset_id: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut transcribing = state
            .transcribing_asset_id
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if transcribing.is_some() {
            return Err("a transcription is already running".to_string());
        }
        *transcribing = Some(asset_id);
    }
    let source_path = state
        .session()
        .project
        .media_library
        .iter()
        .find(|a| a.id == asset_id)
        .map(|a| a.source_path.clone());
    let Some(source_path) = source_path else {
        *state
            .transcribing_asset_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        return Err("asset not found".to_string());
    };
    let Some(model_path) = resolve_whisper_model_path(&state) else {
        *state
            .transcribing_asset_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        return Err("no Whisper model configured".to_string());
    };
    let worker_app = app.clone();
    let services = state.services.clone();
    std::thread::spawn(move || {
        run_transcription(worker_app, services, asset_id, source_path, model_path)
    });
    let _ = app.emit(
        TRANSCRIBE_EVENT,
        TranscribeEventPayload::Started { asset_id },
    );
    Ok(())
}

/// The configured `prefs.whisper_model_path` if it names an existing file, else the bundled
/// Whisper Base model -- same fallback `ui::App::apply_bundled_model_defaults` applies at
/// startup for the egui shell.
fn resolve_whisper_model_path(state: &AppState) -> Option<PathBuf> {
    let configured = state.prefs().view.whisper_model_path.clone();
    if !configured.trim().is_empty() && Path::new(&configured).is_file() {
        return Some(PathBuf::from(configured));
    }
    state.services.bundled_whisper_model()
}

fn run_transcription<R: Runtime>(
    app: AppHandle<R>,
    services: Arc<dyn Services>,
    asset_id: u64,
    source_path: PathBuf,
    model_path: PathBuf,
) {
    let cancel = Arc::new(AtomicBool::new(false));
    let outcome = services.transcribe(&source_path, &model_path, &cancel);
    {
        let state = app.state::<AppState>();
        *state
            .transcribing_asset_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
    }
    match outcome {
        Ok(TranscribeOutcome::Completed(segments) | TranscribeOutcome::Cancelled(segments)) => {
            if segments.is_empty() {
                let _ = app.emit(
                    TRANSCRIBE_EVENT,
                    TranscribeEventPayload::NoSpeech { asset_id },
                );
                return;
            }
            save_transcript_document(&app, asset_id, &segments);
            let _ = with_session(&app, |session| {
                let playhead_secs = session.project.timeline().playhead_secs;
                apply_transcription(session.project.timeline_mut(), playhead_secs, segments);
                session.dirty = true;
                Ok(())
            });
            let _ = app.emit(TRANSCRIBE_EVENT, TranscribeEventPayload::Done { asset_id });
        }
        Err(error) => {
            let _ = app.emit(
                TRANSCRIBE_EVENT,
                TranscribeEventPayload::Failed {
                    asset_id,
                    message: error.to_string(),
                },
            );
        }
    }
}

fn save_transcript_document<R: Runtime>(
    app: &AppHandle<R>,
    asset_id: u64,
    segments: &[TranscribeSegment],
) {
    let document = TranscriptDocument::from_transcribe_segments(asset_id, None, segments);
    let dir = {
        let state = app.state::<AppState>();
        let session = state.session();
        avcore::transcript_cache_dir_for_project(&session.project)
    };
    if let Err(error) = avcore::save_transcript_document(&dir, &document) {
        tracing::warn!(asset_id, %error, "failed to save transcript document");
    }
}

/// Inserts one word-highlight `TextClip` per segment onto the first `Text` track (created if
/// none exists), anchored at `playhead_secs` -- mirrors `ui::App::apply_transcription` exactly,
/// except the track is resolved via [`Timeline::resolve_or_create_track`] (desktop's own
/// auto-naming convention, same as `commands::overlays::insert_text`) rather than egui's
/// hardcoded "Legendas" track name.
fn apply_transcription(
    timeline: &mut Timeline,
    playhead_secs: f64,
    segments: Vec<TranscribeSegment>,
) {
    let index = timeline.resolve_or_create_track(TrackKind::Text, None);
    for segment in segments {
        let id = timeline.next_clip_id();
        let words: Vec<WordTiming> = segment
            .words
            .into_iter()
            .map(|w| WordTiming {
                text: w.text,
                start_secs: w.start_secs - segment.start_secs,
                end_secs: w.end_secs - segment.start_secs,
            })
            .collect();
        let highlight_enabled = !words.is_empty();
        let mut clip = TextClip::new(id, playhead_secs + segment.start_secs, segment.text);
        clip.duration_secs = (segment.end_secs - segment.start_secs).max(0.3);
        clip.words = words;
        clip.highlight_enabled = highlight_enabled;
        timeline.tracks[index].text_clips.push(clip);
    }
}

/// The previously saved transcript for `asset_id`, if any -- what the Transcript panel loads
/// for its per-clip mode.
#[tauri::command]
pub fn get_transcript(
    state: State<'_, AppState>,
    asset_id: u64,
) -> Result<Option<TranscriptDocument>, String> {
    let dir = avcore::transcript_cache_dir_for_project(&state.session().project);
    avcore::load_transcript_document(&dir, asset_id).map_err(|e| e.to_string())
}

/// Case-insensitive substring search across every asset with a saved transcript -- the
/// Transcript panel's project-wide search mode.
#[tauri::command]
pub fn search_transcripts(state: State<'_, AppState>, query: String) -> Vec<TranscriptHitView> {
    let session = state.session();
    let dir = avcore::transcript_cache_dir_for_project(&session.project);
    avcore::search_transcripts_in_project(&dir, &session.project.media_library, &query)
        .into_iter()
        .map(|hit: ProjectTranscriptHit| TranscriptHitView {
            asset_id: hit.asset_id,
            asset_name: hit.asset_name,
            word: hit.word,
        })
        .collect()
}

/// Seeks the timeline/preview to `source_secs` of `asset_id`, mapped through the first timeline
/// clip that uses that asset -- what clicking a transcript word does. `Ok(false)` (not an
/// error) if no clip on the timeline uses the asset; the panel row stays informational.
#[tauri::command]
pub fn seek_to_transcript_word<R: Runtime>(
    app: AppHandle<R>,
    asset_id: u64,
    source_secs: f64,
) -> Result<bool, String> {
    let state = app.state::<AppState>();
    let timeline_secs = {
        let session = state.session();
        map_asset_source_to_timeline(session.project.timeline(), asset_id, source_secs)
    };
    let Some(timeline_secs) = timeline_secs else {
        return Ok(false);
    };
    with_session(&app, |session| {
        session.project.timeline_mut().playhead_secs = timeline_secs;
        Ok(())
    })?;
    state.preview.seek(timeline_secs);
    Ok(true)
}

fn map_asset_source_to_timeline(
    timeline: &Timeline,
    asset_id: u64,
    source_secs: f64,
) -> Option<f64> {
    for track in &timeline.tracks {
        if let Some(clip) = track.clips.iter().find(|c| c.asset_id == asset_id) {
            return Some(
                clip.start_secs
                    + (source_secs - clip.source_in_secs) / clip.speed_factor.max(0.01) as f64,
            );
        }
    }
    None
}

#[cfg(test)]
#[path = "transcribe_test.rs"]
mod tests;
