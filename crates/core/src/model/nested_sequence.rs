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
//! on timeline/export inputs, source-file revisions, and descendant render generations) avoids
//! reusing renders after a relink or a change to a child sequence.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

use crate::media::MediaAsset;
use crate::project::{Project, Sequence, SequenceExportSettings};
use crate::render::{
    apply_export_aspect_ratio, render_export_job_multi_with_audio, resolve_audio_segments,
    resolve_privacy_blur_segments, resolve_shape_segments, resolve_text_segments,
    resolve_timeline_segments_multi, RenderError, RenderOutcome,
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

/// A rendered sequence and its inputs. Callers own the map; only this module constructs entries
/// so descendant generations and file revisions stay consistent with the rendered content.
#[derive(Debug, Clone, PartialEq)]
pub struct NestedSequenceCache {
    pub timeline: Timeline,
    pub rendered_path: PathBuf,
    inputs: NestedRenderInputs,
    generation: u64,
    output_revision: Option<FileRevision>,
}

#[derive(Debug, Clone, PartialEq)]
struct NestedRenderInputs {
    project_id: u64,
    project_path: Option<PathBuf>,
    cache_dir: PathBuf,
    export_settings: SequenceExportSettings,
    media: Vec<RenderMediaInput>,
    files: Vec<(PathBuf, Option<FileRevision>)>,
    children: Vec<(u64, u64)>,
}

/// Render-relevant asset fields, deliberately excluding waveforms and other display data.
#[derive(Debug, Clone, PartialEq)]
struct RenderMediaInput {
    id: u64,
    source_path: PathBuf,
    has_audio: bool,
    resolution: Option<(u32, u32)>,
    fps: Option<f32>,
    bitrate_mbps: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileRevision {
    size: u64,
    modified: SystemTime,
}

fn file_revision(path: &Path) -> Option<FileRevision> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    Some(FileRevision {
        size: metadata.len(),
        modified: metadata.modified().ok()?,
    })
}

fn render_inputs(
    project: &Project,
    sequence: &Sequence,
    cache_dir: &Path,
    cache: &HashMap<u64, NestedSequenceCache>,
) -> NestedRenderInputs {
    let clips: Vec<_> = sequence
        .timeline
        .tracks
        .iter()
        .flat_map(|track| &track.clips)
        .collect();
    let asset_ids: HashSet<_> = clips
        .iter()
        .filter(|clip| clip.nested_sequence_id.is_none())
        .map(|clip| clip.asset_id)
        .collect();
    let mut media: Vec<_> = project
        .media_library
        .iter()
        .filter(|asset| asset_ids.contains(&asset.id))
        .map(|asset| RenderMediaInput {
            id: asset.id,
            source_path: asset.source_path.clone(),
            has_audio: asset.has_audio,
            resolution: asset.resolution,
            fps: asset.fps,
            bitrate_mbps: asset.source_bitrate_mbps,
        })
        .collect();
    media.sort_by_key(|asset| asset.id);
    let mut paths: Vec<_> = media
        .iter()
        .map(|asset| asset.source_path.clone())
        .collect();
    for clip in &clips {
        for (path, enabled) in [
            (&clip.lut_path, true),
            (
                &clip.background_removal_mask_path,
                clip.background_removal_enabled,
            ),
            (&clip.privacy_blur_mask_path, clip.privacy_blur_enabled),
        ] {
            if enabled && !path.is_empty() {
                paths.push(PathBuf::from(path));
            }
        }
    }
    paths.sort();
    paths.dedup();
    let files = paths
        .into_iter()
        .map(|path| {
            let revision = file_revision(&path);
            (path, revision)
        })
        .collect();
    let mut children: Vec<_> = clips
        .iter()
        .filter_map(|clip| clip.nested_sequence_id)
        .filter_map(|id| cache.get(&id).map(|entry| (id, entry.generation)))
        .collect();
    children.sort_unstable();
    children.dedup();
    NestedRenderInputs {
        project_id: project.id,
        project_path: project.file_path.clone(),
        cache_dir: cache_dir.to_path_buf(),
        export_settings: sequence.export_settings,
        media,
        files,
        children,
    }
}

/// Renders every nested-sequence clip ([`crate::timeline::ClipInstance::nested_sequence_id`])
/// reachable from `timeline`, recursively (a nested sequence's own clips may themselves be
/// nested), checking descendants before reusing an entry with equal render inputs. Returns a
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
    let sequence_ids: HashSet<_> = project
        .sequences
        .iter()
        .map(|sequence| sequence.id)
        .collect();
    cache.retain(|id, entry| {
        sequence_ids.contains(id)
            && entry.inputs.project_id == project.id
            && entry.inputs.project_path == project.file_path
            && entry.inputs.cache_dir == cache_dir
    });
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

fn materialize_inner(
    project: &Project,
    timeline: &Timeline,
    cache_dir: &Path,
    cache: &mut HashMap<u64, NestedSequenceCache>,
    visiting: &mut HashSet<u64>,
    assets: &mut HashMap<u64, MediaAsset>,
) -> Result<(), RenderError> {
    let nested_ids: Vec<u64> = timeline
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .filter_map(|c| c.nested_sequence_id)
        .collect();

    for nested_id in nested_ids {
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
        // Recurse first -- the nested sequence's own nested clips need materializing before it
        // can be rendered itself.
        materialize_inner(
            project,
            &nested_sequence.timeline,
            cache_dir,
            cache,
            visiting,
            assets,
        )?;
        visiting.remove(&nested_id);

        let inputs = render_inputs(project, nested_sequence, cache_dir, cache);
        if let Some(cached) = cache.get(&nested_id) {
            if cached.timeline == nested_sequence.timeline
                && cached.inputs == inputs
                && inputs.files.iter().all(|(_, revision)| revision.is_some())
                && cached.output_revision.is_some()
                && cached.output_revision == file_revision(&cached.rendered_path)
            {
                assets.insert(
                    nested_id,
                    synthetic_asset(nested_id, &cached.rendered_path)?,
                );
                continue;
            }
        }

        let rendered_path = render_nested_sequence(project, nested_sequence, cache_dir, assets)?;
        assets.insert(nested_id, synthetic_asset(nested_id, &rendered_path)?);
        static NEXT_GENERATION: AtomicU64 = AtomicU64::new(0);
        let output_revision = file_revision(&rendered_path);
        cache.insert(
            nested_id,
            NestedSequenceCache {
                timeline: nested_sequence.timeline.clone(),
                rendered_path,
                inputs,
                generation: NEXT_GENERATION.fetch_add(1, Ordering::Relaxed),
                output_revision,
            },
        );
    }
    Ok(())
}

/// Renders `sequence` (whose own nested clips are already resolved in `already_materialized`)
/// to a fresh file under `cache_dir`, named by sequence id — a later render of the same sequence
/// overwrites it, no stale-file accumulation.
fn render_nested_sequence(
    project: &Project,
    sequence: &Sequence,
    cache_dir: &Path,
    already_materialized: &HashMap<u64, MediaAsset>,
) -> Result<PathBuf, RenderError> {
    let mut media_library = project.media_library.clone();
    media_library.extend(already_materialized.values().cloned());

    let (track_segments, canvas) = resolve_timeline_segments_multi(sequence, &media_library)?;
    let canvas = apply_export_aspect_ratio(canvas, sequence.export_settings.aspect_ratio);
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
        sequence.export_settings.target_lufs,
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
    })
}

#[cfg(test)]
#[path = "nested_sequence/nested_sequence_test.rs"]
mod tests;
