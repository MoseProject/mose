//! D2 (`spec/architecture/differentiators.md`): highlight-candidate detection over the active
//! sequence's whole timeline -- mirrors `ui::App::detect_highlights` exactly. Synchronous (only
//! needs already-cached `MediaAsset::waveform_peaks`, no fresh decode), no core changes needed.

use avcore::timeline::{AudioRole, MarkerKind};

use super::undoable;

/// Scans every `GameAudio`-tagged track against every `Mic`-tagged track for simultaneous
/// amplitude spikes and adds a numbered `MarkerKind::Highlight` marker at each candidate's
/// start -- what the timeline toolbar's "Detect Highlights" button does. No undo step pushed if
/// no track has both roles tagged, or nothing was found.
#[tauri::command]
pub fn detect_highlights<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<(), String> {
    undoable(&app, |project| {
        let mut game_audio_samples = Vec::new();
        let mut mic_samples = Vec::new();
        for track in &project.timeline().tracks {
            let bucket = match track.audio_role {
                AudioRole::GameAudio => &mut game_audio_samples,
                AudioRole::Mic => &mut mic_samples,
                AudioRole::Unspecified | AudioRole::Music => continue,
            };
            for clip in &track.clips {
                let Some(asset) = project.media_library.iter().find(|a| a.id == clip.asset_id)
                else {
                    continue;
                };
                let Some(peaks) = &asset.waveform_peaks else {
                    continue;
                };
                bucket.extend(avcore::clip_amplitude_samples(
                    peaks,
                    asset.duration_secs,
                    clip,
                ));
            }
        }
        if game_audio_samples.is_empty() || mic_samples.is_empty() {
            return false;
        }
        let candidates = avcore::detect_highlight_candidates(
            &game_audio_samples,
            &mic_samples,
            avcore::DEFAULT_HIGHLIGHT_THRESHOLD_LINEAR,
            avcore::DEFAULT_HIGHLIGHT_GRID_SECS,
            avcore::DEFAULT_HIGHLIGHT_MIN_DURATION_SECS,
        );
        if candidates.is_empty() {
            return false;
        }
        let existing_highlights = project
            .timeline()
            .markers
            .iter()
            .filter(|m| m.kind == MarkerKind::Highlight)
            .count();
        let timeline = project.timeline_mut();
        for (i, candidate) in candidates.iter().enumerate() {
            let marker_id = timeline.add_marker(candidate.start_secs, MarkerKind::Highlight);
            if let Some(marker) = timeline.marker_mut(marker_id) {
                marker.label = format!("Highlight {}", existing_highlights + i + 1);
            }
        }
        true
    })
}

#[cfg(test)]
#[path = "highlight_detection_test.rs"]
mod tests;
