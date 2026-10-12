// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! What the webview is told after an edit. Sending the whole project as JSON on every change
//! costs time that grows with the project (15 ms at 3000 clips, serialized, then parsed again by
//! the webview), so after the first full snapshot only the difference travels: a list of
//! "set this path to this value" operations against the project JSON, with `rev` numbers so the
//! webview can tell when it missed one and ask for a full snapshot instead.
//!
//! The diff walks the project one level at a time with exhaustive destructuring (no `..`), so a
//! field added to `Project`, `Sequence`, `Timeline` or `Track` does not compile until it is
//! handled here, rather than silently never reaching the webview.

use std::sync::Arc;

use avcore::timeline::{Timeline, Track};
use avcore::{MediaAsset, Project, Sequence};
use serde::Serialize;
use serde_json::{json, Value};

/// One change to the project JSON: set the value at `p`, or cut the array at `p` to `len`.
/// Setting an index equal to the array's length appends.
pub type Op = Value;

/// What the last publish left behind, to diff the next one against.
#[derive(Default)]
pub struct SyncState {
    /// Number of the last event sent; the webview's copy is at this revision.
    pub rev: u64,
    pub published: Option<Arc<Project>>,
    /// The project epoch `published` belongs to: a different one means another project was
    /// opened, which is sent whole.
    pub epoch: u64,
}

/// The small fields every update carries next to the project.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Flags {
    pub file_path: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
}

/// Event payload of a partial update.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Patch {
    /// The revision this patch applies on top of.
    pub base: u64,
    pub rev: u64,
    #[serde(flatten)]
    pub flags: Flags,
    pub ops: Vec<Op>,
}

/// Moves the coarse waveform out of an asset's JSON: thousands of numbers per asset that the
/// webview would parse and hold for every snapshot. They are served as bytes at
/// `oca://wave/<id>/coarse` instead, and `has_waveform` says whether there are any.
pub fn strip_waveform(asset: &mut Value) {
    if let Value::Object(fields) = asset {
        let present =
            matches!(fields.get("waveform_peaks"), Some(Value::Array(peaks)) if !peaks.is_empty());
        fields.insert("waveform_peaks".into(), Value::Null);
        fields.insert("has_waveform".into(), Value::Bool(present));
    }
}

/// `asset` as the webview receives it (see [`strip_waveform`]), without serializing the peaks.
fn asset_json(asset: &MediaAsset) -> Value {
    let present = asset
        .waveform_peaks
        .as_ref()
        .is_some_and(|peaks| !peaks.is_empty());
    let mut json = serde_json::to_value(MediaAsset {
        waveform_peaks: None,
        ..asset.clone()
    })
    .unwrap_or(Value::Null);
    if let Value::Object(fields) = &mut json {
        fields.insert("has_waveform".into(), Value::Bool(present));
    }
    json
}

/// The whole session as JSON for the webview.
pub fn snapshot(view: &impl Serialize) -> Value {
    let mut json = serde_json::to_value(view).unwrap_or(Value::Null);
    if let Some(Value::Array(assets)) = json.pointer_mut("/project/media_library") {
        assets.iter_mut().for_each(strip_waveform);
    }
    json
}

fn set(ops: &mut Vec<Op>, path: &[Value], value: impl Serialize) {
    ops.push(json!({ "p": path, "v": serde_json::to_value(value).unwrap_or(Value::Null) }));
}

fn truncate(ops: &mut Vec<Op>, path: &[Value], len: usize) {
    ops.push(json!({ "p": path, "len": len }));
}

fn child(path: &[Value], segment: impl Into<Value>) -> Vec<Value> {
    let mut next = path.to_vec();
    next.push(segment.into());
    next
}

/// Sets `key` when `prev` and `next` differ.
fn field<T: PartialEq + Serialize>(
    ops: &mut Vec<Op>,
    path: &[Value],
    key: &str,
    prev: &T,
    next: &T,
) {
    if prev != next {
        set(ops, &child(path, key), next);
    }
}

/// Elements that differ go through `each`; extra ones are appended and surplus ones cut.
fn list<T: PartialEq + Serialize>(
    ops: &mut Vec<Op>,
    path: &[Value],
    prev: &[T],
    next: &[T],
    each: impl Fn(&mut Vec<Op>, &[Value], &T, &T),
    encode: impl Fn(&T) -> Value,
) {
    for (index, (old, new)) in prev.iter().zip(next).enumerate() {
        if old != new {
            each(ops, &child(path, index), old, new);
        }
    }
    for (index, new) in next.iter().enumerate().skip(prev.len()) {
        set(ops, &child(path, index), encode(new));
    }
    if next.len() < prev.len() {
        truncate(ops, path, next.len());
    }
}

/// [`list`] where a changed element is simply replaced.
fn flat_list<T: PartialEq + Serialize>(ops: &mut Vec<Op>, path: &[Value], prev: &[T], next: &[T]) {
    list(
        ops,
        path,
        prev,
        next,
        |ops, at, _, new| set(ops, at, new),
        |new| serde_json::to_value(new).unwrap_or(Value::Null),
    );
}

fn diff_track(ops: &mut Vec<Op>, at: &[Value], prev: &Track, next: &Track) {
    let Track {
        id,
        name,
        kind,
        clips,
        text_clips,
        shape_clips,
        visible,
        audio_role,
        muted,
        solo,
        locked,
        color_label,
        mix,
    } = next;
    field(ops, at, "id", &prev.id, id);
    field(ops, at, "name", &prev.name, name);
    field(ops, at, "kind", &prev.kind, kind);
    flat_list(ops, &child(at, "clips"), &prev.clips, clips);
    flat_list(ops, &child(at, "text_clips"), &prev.text_clips, text_clips);
    flat_list(
        ops,
        &child(at, "shape_clips"),
        &prev.shape_clips,
        shape_clips,
    );
    field(ops, at, "visible", &prev.visible, visible);
    field(ops, at, "audio_role", &prev.audio_role, audio_role);
    field(ops, at, "muted", &prev.muted, muted);
    field(ops, at, "solo", &prev.solo, solo);
    field(ops, at, "locked", &prev.locked, locked);
    field(ops, at, "color_label", &prev.color_label, color_label);
    field(ops, at, "mix", &prev.mix, mix);
}

fn diff_timeline(ops: &mut Vec<Op>, at: &[Value], prev: &Timeline, next: &Timeline) {
    let Timeline {
        tracks,
        playhead_secs,
        markers,
        multicam_groups,
    } = next;
    list(
        ops,
        &child(at, "tracks"),
        &prev.tracks,
        tracks,
        diff_track,
        |track| serde_json::to_value(track).unwrap_or(Value::Null),
    );
    field(ops, at, "playhead_secs", &prev.playhead_secs, playhead_secs);
    field(ops, at, "markers", &prev.markers, markers);
    field(
        ops,
        at,
        "multicam_groups",
        &prev.multicam_groups,
        multicam_groups,
    );
}

fn diff_sequence(ops: &mut Vec<Op>, at: &[Value], prev: &Sequence, next: &Sequence) {
    let Sequence {
        id,
        name,
        timeline,
        export_settings,
    } = next;
    field(ops, at, "id", &prev.id, id);
    field(ops, at, "name", &prev.name, name);
    diff_timeline(ops, &child(at, "timeline"), &prev.timeline, timeline);
    field(
        ops,
        at,
        "export_settings",
        &prev.export_settings,
        export_settings,
    );
}

/// The operations that turn `prev`'s JSON into `next`'s.
pub fn diff(prev: &Project, next: &Project) -> Vec<Op> {
    // `file_path` is `serde(skip)`: the webview gets it through [`Flags`].
    let Project {
        id,
        name,
        last_edited,
        summary,
        media_library,
        sequences,
        active_sequence,
        file_path: _,
        panel_layout,
        smart_bins,
        recent_asset_ids,
        frame_rate,
    } = next;
    let mut ops = Vec::new();
    let root: [Value; 0] = [];
    field(&mut ops, &root, "id", &prev.id, id);
    field(&mut ops, &root, "name", &prev.name, name);
    field(
        &mut ops,
        &root,
        "last_edited",
        &prev.last_edited,
        last_edited,
    );
    field(&mut ops, &root, "summary", &prev.summary, summary);
    list(
        &mut ops,
        &child(&root, "media_library"),
        &prev.media_library,
        media_library,
        |ops, at, _, new| set(ops, at, asset_json(new)),
        asset_json,
    );
    list(
        &mut ops,
        &child(&root, "sequences"),
        &prev.sequences,
        sequences,
        diff_sequence,
        |sequence| serde_json::to_value(sequence).unwrap_or(Value::Null),
    );
    field(
        &mut ops,
        &root,
        "active_sequence",
        &prev.active_sequence,
        active_sequence,
    );
    field(
        &mut ops,
        &root,
        "panel_layout",
        &prev.panel_layout,
        panel_layout,
    );
    field(&mut ops, &root, "smart_bins", &prev.smart_bins, smart_bins);
    field(
        &mut ops,
        &root,
        "recent_asset_ids",
        &prev.recent_asset_ids,
        recent_asset_ids,
    );
    field(&mut ops, &root, "frame_rate", &prev.frame_rate, frame_rate);
    ops
}

/// An audio asset with no waveform, for tests in other modules.
#[cfg(test)]
pub(crate) fn tests_support_asset() -> MediaAsset {
    tests::audio_asset(1, None)
}

#[cfg(test)]
#[path = "session_sync_test.rs"]
mod tests;
