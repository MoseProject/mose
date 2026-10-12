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

//! Nested sequences / compound clips (`spec/ROADMAP.md`, `matrix/competitor-parity.md`'s
//! 2026-08-27 update) — Premiere/DaVinci/FCP all let a group of clips be edited as its own
//! sub-timeline, then dropped onto a parent timeline as a single clip. oca already has
//! independently-editable [`crate::project::Sequence`]s (the Editor's tabs) — a compound clip
//! reuses that directly rather than inventing a second sub-timeline concept:
//! [`crate::timeline::ClipInstance::nested_sequence_id`] points at another `Sequence` in the
//! same [`crate::project::Project`] instead of `asset_id`.
//!
//! The render/preview pipeline needs to *recurse* into that sequence, not just carry a pointer
//! (per this feature's own scoping note in `matrix/competitor-parity.md`) — [`materialize_nested_sequences`]
//! does that by actually rendering each nested sequence to a cached temp file, then handing back
//! a synthetic [`MediaAsset`] per nested clip pointing at that file. Every existing resolution
//! function ([`crate::render::resolve_timeline_segments_multi`] and friends) then treats a
//! compound clip exactly like an ordinary asset-backed one — zero changes needed to their own
//! logic, and every per-clip effect (crop/color/keyframes/...) still applies on top of the
//! rendered nested content for free, since it's the same [`crate::timeline::ClipInstance`].
//!
//! **Known cost, deliberately not hidden**: unlike an imported asset, a nested sequence's
//! rendered file is produced by a real encode, not instant. `ui`'s callers (queueing an export,
//! opening a preview) call this synchronously and block until it's done — no background-thread/
//! progress-reporting path yet, a real UX rough edge for a long nested sequence. Caching (keyed
//! on the nested sequence's own [`crate::timeline::Timeline`] content — cheap `PartialEq`, no
//! hashing) means this cost is paid once per edit to that nested sequence, not on every call.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::media::MediaAsset;
use crate::project::{Project, Sequence};
use crate::render::{
    render_export_job_multi_with_audio, resolve_audio_segments, resolve_privacy_blur_segments,
    resolve_shape_segments, resolve_text_segments, resolve_timeline_segments_multi, RenderError,
    RenderOutcome,
};
use crate::timeline::Timeline;

/// Where a project's nested-sequence renders are cached — a hidden sibling folder next to the
/// project file, same convention [`crate::proxy::cache_dir_for_project`] uses (kept separate
/// from the proxy cache dir since these are full re-encodes, not lightweight editing proxies,
/// and clearing one cache shouldn't imply clearing the other).
pub fn cache_dir_for_project(project: &Project) -> PathBuf {
    match &project.file_path {
        Some(path) => {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("project");
            path.with_file_name(format!(".{stem}_nested"))
        }
        None => std::env::temp_dir().join("oca_unsaved_nested"),
    }
}

/// One nested sequence's rendered file, cached against the exact [`Timeline`] content it was
/// rendered from — `ui`'s `App` owns a `HashMap<u64, NestedSequenceCache>` (mirroring
/// `ExportPreviewCache`'s own "value-equality, not a version counter" pattern) so repeated calls
/// after an unrelated edit don't re-render. Public so `ui` can store/compare it without this
/// module needing to know anything about `App`.
#[derive(Debug, Clone, PartialEq)]
pub struct NestedSequenceCache {
    pub timeline: Timeline,
    pub rendered_path: PathBuf,
    /// The rendered file as an asset, probed once when it was rendered: a cache hit then costs a
    /// timeline comparison and a `stat`, not a probe of the file on every edit.
    pub asset: MediaAsset,
}

/// Renders every nested-sequence clip ([`crate::timeline::ClipInstance::nested_sequence_id`])
/// reachable from `timeline`, recursively (a nested sequence's own clips may themselves be
/// nested), skipping any whose `cache` already has a fresh (`Timeline`-equal) entry. Returns a
/// map from `nested_sequence_id` to a synthetic [`MediaAsset`] (`source_path` = the rendered
/// file) that [`crate::render::resolve_timeline_segments_multi`]/[`resolve_audio_segments`] can
/// resolve exactly like an ordinary asset-backed clip once merged into a `media_library` clone —
/// see this module's own doc comment. `cache` is updated in place with any newly-rendered
/// entries (and read for ones that are already fresh); synthetic ids are allocated past the
/// project's own highest real asset id, so they can never collide.
///
/// [`RenderError::CyclicNestedSequence`] if a sequence (transitively) nests itself.
/// [`RenderError::MissingNestedSequence`] if a `nested_sequence_id` doesn't resolve.
pub fn materialize_nested_sequences(
    project: &Project,
    timeline: &Timeline,
    cache_dir: &Path,
    cache: &mut HashMap<u64, NestedSequenceCache>,
) -> Result<HashMap<u64, MediaAsset>, RenderError> {
    let mut visiting = HashSet::new();
    let mut assets = HashMap::new();
    materialize_inner(
        project,
        timeline,
        cache_dir,
        cache,
        &mut visiting,
        &mut assets,
    )?;
    Ok(assets)
}

fn nested_ids_of(timeline: &Timeline) -> Vec<u64> {
    timeline
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .filter_map(|c| c.nested_sequence_id)
        .collect()
}

/// Whether the cache holds a render of every compound clip under `timeline` that still matches
/// what is in the project: each nested sequence unchanged since its render, the file there, and
/// nothing nested inside it changed either. Cheap: comparisons and `stat`s, no rendering.
pub fn nested_sequences_are_current(
    project: &Project,
    timeline: &Timeline,
    cache: &HashMap<u64, NestedSequenceCache>,
) -> bool {
    fn current(
        project: &Project,
        timeline: &Timeline,
        cache: &HashMap<u64, NestedSequenceCache>,
        visiting: &mut HashSet<u64>,
    ) -> bool {
        nested_ids_of(timeline).into_iter().all(|id| {
            let Some(sequence) = project.sequences.iter().find(|s| s.id == id) else {
                return false;
            };
            let Some(cached) = cache.get(&id) else {
                return false;
            };
            if !visiting.insert(id) {
                return false;
            }
            let ok = cached.timeline == sequence.timeline
                && cached.rendered_path.exists()
                && current(project, &sequence.timeline, cache, visiting);
            visiting.remove(&id);
            ok
        })
    }
    current(project, timeline, cache, &mut HashSet::new())
}

/// The assets of the renders the cache holds for the compound clips under `timeline`, current or
/// not (a render whose file is gone is left out): what a preview shows while a fresh render is
/// being made in the background.
pub fn cached_nested_assets(
    project: &Project,
    timeline: &Timeline,
    cache: &HashMap<u64, NestedSequenceCache>,
) -> HashMap<u64, MediaAsset> {
    fn collect(
        project: &Project,
        timeline: &Timeline,
        cache: &HashMap<u64, NestedSequenceCache>,
        assets: &mut HashMap<u64, MediaAsset>,
        seen: &mut HashSet<u64>,
    ) {
        for id in nested_ids_of(timeline) {
            // Once per sequence: also what stops a nesting cycle, which is only reported later,
            // when a render is attempted.
            if !seen.insert(id) {
                continue;
            }
            if let Some(cached) = cache.get(&id).filter(|c| c.rendered_path.exists()) {
                assets.insert(id, cached.asset.clone());
            }
            if let Some(sequence) = project.sequences.iter().find(|s| s.id == id) {
                collect(project, &sequence.timeline, cache, assets, seen);
            }
        }
    }
    let mut assets = HashMap::new();
    collect(project, timeline, cache, &mut assets, &mut HashSet::new());
    assets
}

/// Renders what is stale under `timeline`; true when anything at or below this level had to be
/// rendered again (so whatever is rendered from it is stale too).
fn materialize_inner(
    project: &Project,
    timeline: &Timeline,
    cache_dir: &Path,
    cache: &mut HashMap<u64, NestedSequenceCache>,
    visiting: &mut HashSet<u64>,
    assets: &mut HashMap<u64, MediaAsset>,
) -> Result<bool, RenderError> {
    let mut rendered_any = false;
    for nested_id in nested_ids_of(timeline) {
        if assets.contains_key(&nested_id) {
            continue;
        }
        let nested_sequence = project
            .sequences
            .iter()
            .find(|s| s.id == nested_id)
            .ok_or(RenderError::MissingNestedSequence)?;

        if !visiting.insert(nested_id) {
            return Err(RenderError::CyclicNestedSequence);
        }
        // Whatever is nested inside it comes first: it has to be there to render this one, and a
        // change down there makes this render stale even when this sequence itself is untouched.
        let below_changed = materialize_inner(
            project,
            &nested_sequence.timeline,
            cache_dir,
            cache,
            visiting,
            assets,
        )?;
        visiting.remove(&nested_id);

        if !below_changed {
            if let Some(cached) = cache.get(&nested_id) {
                if cached.timeline == nested_sequence.timeline && cached.rendered_path.exists() {
                    assets.insert(nested_id, cached.asset.clone());
                    continue;
                }
            }
        }

        let rendered_path = render_nested_sequence(project, nested_sequence, cache_dir, assets)?;
        let asset = synthetic_asset(nested_id, &rendered_path)?;
        assets.insert(nested_id, asset.clone());
        cache.insert(
            nested_id,
            NestedSequenceCache {
                timeline: nested_sequence.timeline.clone(),
                rendered_path,
                asset,
            },
        );
        rendered_any = true;
    }
    Ok(rendered_any)
}

/// One compound clip render at a time in the process. The file is named by sequence id, and the
/// preview (in the background) and an export may both ask for the same one: two renders writing
/// the same path would corrupt it.
static RENDER_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Renders `sequence` (whose own nested clips are already resolved in `already_materialized`)
/// to a fresh file under `cache_dir`, named by sequence id — a later render of the same sequence
/// overwrites it, no stale-file accumulation.
fn render_nested_sequence(
    project: &Project,
    sequence: &Sequence,
    cache_dir: &Path,
    already_materialized: &HashMap<u64, MediaAsset>,
) -> Result<PathBuf, RenderError> {
    let _one_at_a_time = RENDER_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut media_library = project.media_library.clone();
    media_library.extend(already_materialized.values().cloned());

    let (track_segments, canvas) = resolve_timeline_segments_multi(sequence, &media_library)?;
    let audio_segments = resolve_audio_segments(sequence, &media_library)?;
    let text_segments = resolve_text_segments(sequence, canvas.width, canvas.height);
    let shape_segments = resolve_shape_segments(sequence, canvas.width, canvas.height);
    let privacy_blur_segments = resolve_privacy_blur_segments(sequence);

    std::fs::create_dir_all(cache_dir).map_err(RenderError::ReplaceOutput)?;
    let output = cache_dir.join(format!("sequence_{}.mp4", sequence.id));

    let outcome = render_export_job_multi_with_audio(
        &track_segments,
        &audio_segments,
        canvas,
        &output,
        // The loudness is set once, by the export or the preview that uses this file: a second
        // pass here would normalize the sound twice, and cost the time of a loudness analysis.
        crate::render::NO_NORMALIZATION,
        avbridge::GpuEncoderPreference::Auto,
        &text_segments,
        &shape_segments,
        &privacy_blur_segments,
        &AtomicBool::new(false),
        |_percent| {},
    )?;
    debug_assert_eq!(
        outcome,
        RenderOutcome::Completed,
        "cancel is never set here"
    );
    Ok(output)
}

/// The [`MediaAsset::id`] a nested sequence's synthetic asset gets, offset into a private range
/// (`u64::MAX / 2 +`) so it can never collide with a real imported asset's id. Every clip
/// lookup by asset id (`render.rs`'s resolve functions, `ui`'s preview clip resolution, the
/// desktop preview engine) must go through [`crate::timeline::ClipInstance::resolved_asset_id`]
/// rather than the raw `asset_id` field so a compound clip resolves to this same id.
pub fn synthetic_asset_id(nested_sequence_id: u64) -> u64 {
    u64::MAX / 2 + nested_sequence_id
}

/// A synthetic [`MediaAsset`] standing in for a rendered nested-sequence file — probed for real
/// (resolution/fps/duration/audio), so it resolves through the ordinary asset pipeline exactly
/// like an imported file. `id` is [`synthetic_asset_id`] of the nested sequence — this asset is
/// never inserted into `Project::media_library` itself, only into a throwaway clone passed to
/// the resolve functions, so no persistence/uniqueness contract with real asset ids is at risk.
fn synthetic_asset(
    nested_sequence_id: u64,
    rendered_path: &Path,
) -> Result<MediaAsset, RenderError> {
    let info = avbridge::probe(rendered_path).map_err(|_| RenderError::ProbeNestedSequence)?;
    Ok(MediaAsset {
        id: synthetic_asset_id(nested_sequence_id),
        file_name: rendered_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("nested_sequence.mp4")
            .to_string(),
        source_path: rendered_path.to_path_buf(),
        kind: crate::media::MediaKind::Video,
        has_audio: info.has_audio,
        duration_secs: info.duration_secs,
        codec: info.codec_name,
        source_bitrate_mbps: info
            .bit_rate
            .map(|bps| bps as f32 / 1_000_000.0)
            .unwrap_or(0.0),
        resolution: info.resolution,
        fps: info.fps,
        sample_rate_khz: None,
        loudness: None,
        proxy_path: None,
        waveform_peaks: None,
        favorited: false,
        streams: Default::default(),
        hdr_handling: Default::default(),
        audio_stream: None,
    })
}

#[cfg(test)]
#[path = "nested_sequence/nested_sequence_test.rs"]
mod tests;
